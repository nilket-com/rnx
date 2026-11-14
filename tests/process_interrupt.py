"""Real child readiness; session-only terminal cancellation never leaks a marker."""
import json
import os
from pathlib import Path
import select
import signal
import subprocess
import sys
import tempfile

from worker_parent import Parent


def ready(fd):
    assert select.select([fd], [], [], 10)[0], "child did not reach readiness"
    assert os.read(fd, 16) == b"yes"


def check(binary, fixture, mode):
    with tempfile.TemporaryDirectory(prefix="rnx-process-interrupt-") as temp:
        directory = Path(temp)
        marker = directory / "started"
        os.mkfifo(marker)
        fd = os.open(marker, os.O_RDONLY | os.O_NONBLOCK)
        call = f"process::run({json.dumps(fixture)},[\"sleep\"],#{{cwd:{json.dumps(temp)}}})?"
        source = f'let unpublished=1;let r={call};println!("cancelled={{}}",r.cancelled);7'
        process = None
        worker = None
        try:
            if mode == "worker":
                worker = Parent(binary, engine="execution")
                worker.begin(source)
                assert worker.message()["type"] == "armed"
                process = worker.p
            else:
                if mode == "run":
                    file = directory / "input.rn"
                    file.write_text(f"pub fn main(_){{{source}}}")
                    arguments = ["run", str(file)]
                elif mode == "eval":
                    arguments = ["eval", source]
                else:
                    arguments = ["--no-splash", "repl", "--engine=execution"]
                process = subprocess.Popen(
                    [binary, "--color=never", *arguments],
                    stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                    env=dict(os.environ, RNX_CONFIG="", RNX_HISTORY="/dev/null"),
                )
                if mode == "session":
                    process.stdin.write((source + "\n").encode())
                    process.stdin.flush()
            ready(fd)
            process.send_signal(signal.SIGINT)
            if worker:
                reply, _ = worker.settled()
                streams = worker.handoff()
                assert reply["failure"]["category"] == "interrupted", reply
                assert "host call interrupted" not in json.dumps(reply), reply
                assert all(b"cancelled=" not in bytes(s.data) for s in streams.values())
                assert worker.execute("unpublished")[0]["failure"]["category"] == "compile"
                assert worker.execute("42")[0]["text_plain"] == "42"
            else:
                if mode == "session":
                    assert select.select([process.stderr], [], [], 5)[0], "no interrupt diagnostic"
                    diagnostic = process.stderr.readline()
                    assert diagnostic == b"interrupted\n", diagnostic
                    output, error = process.communicate(b"unpublished\n42\n:quit\n", timeout=10)
                    assert process.returncode == 0 and b"42" in output, (output, error)
                    assert b"unpublished" in error and b"cancelled=" not in output, (output, error)
                else:
                    output, error = process.communicate(timeout=10)
                    expected = 0 if mode == "run" else 130
                    assert process.returncode == expected, (mode, process.returncode, output, error)
                    if mode == "run":
                        assert output == b"cancelled=true\n7\n" and error == b"", (output, error)
                    else:
                        assert error == b"interrupted\n", (output, error)
                assert b"host call interrupted" not in output + error, (output, error)
        finally:
            if worker:
                worker.close()
            elif process is not None and process.poll() is None:
                process.kill()
                process.communicate(timeout=5)
            os.close(fd)


if __name__ == "__main__":
    for mode in ("run", "eval", "session", "worker"):
        check(sys.argv[1], sys.argv[2], mode)
    print("PASS process cancellation across run, eval, session and worker")
