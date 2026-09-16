import json, os, pathlib, signal, subprocess, sys, tempfile, time
from worker_parent import Parent, WAIT, CAP, LIVE
import threading


# A harness watchdog, not a worker/cell timeout or notebook policy.
def watchdog():
    for w in list(LIVE):
        if w.p.poll() is None:
            w.p.kill()
    os._exit(1)


watch = threading.Timer(120, watchdog)
watch.daemon = True
watch.start()
BINARY = sys.argv[1]


def check(condition, detail=None):
    assert condition, detail


def basic():
    with tempfile.TemporaryDirectory() as tmp:
        config = pathlib.Path(tmp) / "bad.rn"
        config.write_text('println("CONFIG MUST NOT RUN");')
        env = dict(
            os.environ,
            RNX_TEST_CONFIG_READS=str(pathlib.Path(tmp) / "reads"),
            RNX_CONFIG=str(config),
            TERM="xterm-256color",
            RNX_HISTORY=str(pathlib.Path(tmp) / "history"),
        )
        w = Parent(BINARY, env)
        try:

            def run(src):
                return w.execute(src)[0]

            check(run("let x = 41;")["text_plain"] is None)
            check(run("x + 1")["text_plain"] == "42")
            check(run("let broken = ;")["failure"]["category"] == "compile")
            check(run('panic!("boom")')["failure"]["category"] == "runtime")
            check(run("x")["text_plain"] == "41")
            before = run("fn old(v) { v.missing() }")["input"]
            call = run("old(1)")
            check(call["failure"]["origin"]["input"] == before, call)
            check("no method" in call["failure"]["diagnostic"])
            over, msg, _ = w.execute("x" * 32769)
            check(over["input"] is None)
            check(len(msg) == 1, msg)
            check(run("x")["input"] == call["input"] + 1)
            check(run("io::stdin()?")["text_plain"] == '""')
            check(run("io::stdin()?")["failure"] is not None)
            check(run("process::exit(0)?")["failure"] is not None)
            r, _, streams = w.execute(
                'print!("no newline"); io::eprint("stderr tail")?; 7'
            )
            check(r["text_plain"] == "7")
            check(streams["stdout"].data == b"no newline")
            check(streams["stderr"].data == b"stderr tail")
            check(not any(w.late.values()), w.late)
            r, _, raw = w.execute(r'print!("\u{1b}[2J"); 1')
            check(raw["stdout"].data == b"\x1b[2J")
            check(r["text_plain"] == "1")
            # No next execute is sent until retained bytes have been handed off.
            w.begin("42")
            r, _ = w.settled()
            time.sleep(0.03)
            check(w.messages.empty())
            check(all(s.identity[1] == w.id for s in w.streams.values()))
            w.handoff()
            w.begin(op="reset")
            r, _ = w.settled()
            check(r["epoch"] == 2)
            w.handoff()
            check(run("x")["failure"]["category"] == "compile")
            check(run("io::stdin()?")["failure"] is not None)
            check(run("let y=9;")["input"] == 3)
            for module in ["mod a;", "mod a { pub fn value() { 42 } }"]:
                failed = run(module)["failure"]
                check("modules, imports, macro declarations, and impl blocks work in files" in failed["diagnostic"], failed)
            w.begin(op="shutdown")
            r, _ = w.settled()
            check(r["failure"] is None)
            w.handoff()
            check(w.p.wait(timeout=WAIT) == 0)
            check(not (pathlib.Path(tmp) / "history").exists())
            reads = pathlib.Path(tmp) / "reads"
            check(
                reads.read_text() == "0"
                if "--test-support" in sys.argv
                else not reads.exists()
            )
        finally:
            w.close()
    print(
        "persistence, source origins, refusal admission, stdin, reset, barriers, config isolation: pass"
    )


def namespaces():
    w = Parent(BINARY)
    try:
        for name in ["json_parse", "json_stringify", "stdin", "eprint", "exit", "process", "process_bytes", "process_bytes_input", "test_pending", "test_allocation_peak", "test_reset_allocation_peak"]:
            check(w.execute("host::" + name)[0]["failure"]["category"] == "compile")
        source = 'json::parse(json::stringify(18446744073709551615u64)?)? == 18446744073709551615u64'
        check(w.execute(source)[0]["text_plain"] == "true")
        check(w.execute("process::exit(0).is_err()")[0]["text_plain"] == "true")
        r, _, streams = w.execute('io::eprint("tail\\0")?; 42')
        check(r["text_plain"] == "42")
        check(streams["stderr"].data == b"tail\0")
    finally:
        w.close()
    print("domain namespaces and removed host names in worker: pass")


def malformed():
    for value in [
        b"{\n",
        b"\xff\n",
        b"x" * 262145 + b"\n",
        json.dumps(dict(id=0, op="reset", nonce="a" * 64)).encode() + b"\n",
        json.dumps(dict(id=1, op="reset", nonce="A" * 64)).encode() + b"\n",
        b'{"op":"reset","id":1,"nonce":"' + b"a" * 64 + b'","extra":0}\n',
    ]:
        w = Parent(BINARY)
        try:
            try:
                w.write.write(value)
            except BrokenPipeError:
                pass
            check(w.p.wait(timeout=WAIT) == 1)
            check(w.messages.get(timeout=WAIT) is None)
        finally:
            w.close()
    for msg in [
        dict(op="ack", id=2),
        dict(op="execute", id=2, source="42", nonce="b" * 64),
    ]:
        w = Parent(BINARY)
        try:
            w.begin("1")
            w.settled()
            w.send(msg)
            check(w.p.wait(timeout=WAIT) == 1)
        finally:
            w.close()
    print("malformed/oversized frames and missing/wrong acknowledgement: pass")


def interrupt():
    if os.name != "posix":
        return
    for source in [
        "loop {}",
        "time::sleep(10000).await?",
        'process::run("/bin/sleep", ["10"], #{})?',
    ]:
        w = Parent(BINARY)
        try:
            # An early request is queued by the parent, sent only upon armed.
            w.begin(source)
            m = w.message()
            check(m["type"] == "armed", m)
            start = time.monotonic()
            os.kill(w.p.pid, signal.SIGINT)
            r, _ = w.settled()
            elapsed = time.monotonic() - start
            check(r["failure"]["category"] == "interrupted", (source, r))
            check(elapsed < 1, elapsed)
            w.handoff()
            check(w.execute("6*7")[0]["text_plain"] == "42")
        finally:
            w.close()
    # An async CPU loop has the existing gap; parent can still kill/reap it.
    w = Parent(BINARY)
    try:
        w.begin("async fn spin() { loop {} } spin().await")
        check(w.message()["type"] == "armed")
        os.kill(w.p.pid, signal.SIGINT)
        time.sleep(0.03)
        check(w.p.poll() is None)
        w.p.kill()
        w.p.wait(timeout=WAIT)
        check(w.messages.get(timeout=WAIT) is None)
    finally:
        w.close()
    print("armed interrupt, sync/await/process recovery, async CPU hard stop: pass")


def ceiling():
    w = Parent(BINARY, dict(os.environ, RNX_MEMORY_CEILING="1"))
    try:
        r, m, _ = w.execute("42")
        check(r["failure"]["category"] == "over_ceiling")
        check(r["input"] is None)
        check(len(m) == 1)
    finally:
        w.close()
    print("allocation sample before admission: pass")


def volume_and_failure():
    w = Parent(BINARY)
    try:
        text = "x" * 8192
        source = (
            'for n in 0..300 { print!("' + text + '"); io::eprint("' + text + '")?; }'
        )
        r, m, streams = w.execute(source)
        check(r["failure"] is None, r)
        for stream in streams.values():
            check(len(stream.data) == CAP)
            check(stream.data == b"x" * CAP)
            check(stream.discarded == 300 * 8192 - CAP, stream.discarded)
        # The cap never blocks collection of either barrier or the next request.
        check(w.execute("42")[0]["text_plain"] == "42")
    finally:
        w.close()
    w = Parent(BINARY)
    try:
        w.begin("time::sleep(10000).await?")
        check(w.message()["type"] == "armed")
        w.write.close()  # Reader EOF does not interrupt a running cell.
        start = time.monotonic()
        w.p.kill()
        w.p.wait(timeout=WAIT)
        check(time.monotonic() - start < WAIT)
        check(w.messages.get(timeout=WAIT) is None)
        check(not all(s.ended for s in w.streams.values()))
    finally:
        w.close()
    print("output caps continue draining, control EOF and independent hard reap: pass")


def budget():
    import threading

    w = Parent(BINARY)
    timer = threading.Timer(30, w.p.kill)
    timer.start()
    try:
        r, _, _ = w.execute("loop {}")
        check(r["failure"]["category"] == "budget", r)
        check(r["failure"]["budget"] == 2000000000, r)
        check(w.execute("42")[0]["text_plain"] == "42")
    finally:
        timer.cancel()
        w.close()
    print("whole default budget halts and the next input runs: pass")


def startup():
    for args, code in [
        (["worker"], 2),
        (["worker", "--control-read", "0", "--control-write", "1"], 1),
        (["worker", "--control-read", "x", "--control-write", "7"], 2),
    ]:
        p = subprocess.run([BINARY, *args], capture_output=True, timeout=WAIT)
        check(p.returncode == code, (args, p))
    if os.name == "posix":
        read, write = os.pipe()
        capture, output = os.pipe()
        try:
            child = subprocess.Popen(
                [
                    BINARY,
                    "worker",
                    "--control-read",
                    str(read),
                    "--control-write",
                    str(output),
                ],
                pass_fds=(read, output),
                stdin=subprocess.DEVNULL,
                stdout=output,
                stderr=subprocess.PIPE,
            )
            _, error = child.communicate(timeout=WAIT)
            check(
                child.returncode == 1 and b"aliases a standard stream" in error, error
            )
        finally:
            for fd in (read, write, capture, output):
                os.close(fd)
    print("invalid startup arguments/endpoints: pass")


def phase_deaths():
    if "--test-support" not in sys.argv:
        return
    for phase in ["stdout", "stderr", "settled"]:
        with tempfile.TemporaryDirectory() as tmp:
            file = pathlib.Path(tmp) / "phase"
            w = Parent(
                BINARY,
                dict(
                    os.environ,
                    RNX_TEST_WORKER_PAUSE=phase,
                    RNX_TEST_WORKER_PHASE_FILE=str(file),
                ),
            )
            try:
                w.begin("42")
                check(w.message(WAIT)["type"] == "armed")
                deadline = time.monotonic() + WAIT
                while not file.exists():
                    check(time.monotonic() < deadline)
                    time.sleep(0.002)
                w.p.kill()
                w.p.wait(timeout=WAIT)
                check(w.messages.get(timeout=WAIT) is None)
            finally:
                w.close()
    print("worker killed before each barrier and before settled: pass")


def blocked_handoff():
    w = Parent(BINARY)
    try:
        w.begin('print!("held"); 42')
        w.settled()
        time.sleep(WAIT + 0.10)
        check(
            w.p.poll() is not None,
            "independent deadline must fire without handoff resuming",
        )
        try:
            w.handoff()
        except TimeoutError:
            pass
        else:
            raise AssertionError("blocked forwarding was acknowledged")
        check(w.p.poll() is not None)
    finally:
        w.close()
    print("blocked forwarding expires without ack and reaps worker: pass")


def shutdown_drain_failure():
    if "--test-support" not in sys.argv:
        return
    w = Parent(BINARY)
    try:
        check(w.execute('rnx_test::test_shutdown_task().await')[0]['failure'] is None)
        start = time.monotonic()
        w.begin(op='shutdown')
        reply, _ = w.settled()
        check(reply['state_lost'], reply)
        check(reply['failure']['category'] == 'runtime', reply)
        check('runtime shutdown did not finish within 100 ms' in str(reply), reply)
        check(time.monotonic() - start < .5)
        # Both stream barriers and settlement have arrived; only now ack.
        check(w.p.poll() is None)
        w.handoff()
        check(w.p.wait(timeout=WAIT) == 1)
    finally:
        w.close()
    print("failed final drain settles state loss before ack and exits nonzero: pass")


for test in [
    shutdown_drain_failure,
    basic,
    namespaces,
    malformed,
    interrupt,
    ceiling,
    volume_and_failure,
    startup,
    budget,
    phase_deaths,
    blocked_handoff,
]:
    test()

watch.cancel()
