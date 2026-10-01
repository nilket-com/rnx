"""Record 0133: run a workflow in an ordinary `:dep polars candle` session,
as a user would: start the stock `rnx`, request the adapters, paste the
script's lines at the prompt (there is no `:load`: finding F6), then call its
`main` with the arguments. Everything the terminal shows is logged.

  session.py RNX SCRIPT LOG ARG...

Exit status 0 only when the run printed its `WORKFLOW OK` marker with no
error after the call; the log is the evidence either way.
"""
import os, pty, re, select, sys, time

rnx, script, log_path, *args = sys.argv[1:]
# RNX_SESSION_ARGS, when set (for example "project session --manifest M"),
# starts that session instead, and skips the :dep step: the adapters are
# already the project's (used to check unpushed adapter code).
extra = os.environ.get("RNX_SESSION_ARGS", "").split()
pid, fd = pty.fork()
if pid == 0:
    os.execv(rnx, ["rnx", *extra])
log = open(log_path, "wb")
buf = b""


def strip(b):
    return re.sub(rb"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07|\r", b"", b)


def pump(until_idle=0.3, limit=30.0, want=None):
    """Read until `want` appears (if given) or output goes idle."""
    global buf
    start = len(buf)
    end = time.time() + limit
    last = time.time()
    while time.time() < end:
        r, _, _ = select.select([fd], [], [], 0.1)
        if r:
            try:
                chunk = os.read(fd, 65536)
            except OSError:
                return False
            buf += chunk
            log.write(chunk)
            log.flush()
            last = time.time()
            # the prompt is coloured: match against the text without escapes
            if want is not None and re.search(want, strip(buf[start:])):
                return True
        elif want is None and time.time() - last > until_idle and len(buf) > start:
            return True
    return False


def send(line):
    os.write(fd, line.encode() + b"\r")


def prompt_after_newline(limit):
    """Wait for a fresh prompt on a line after the one just entered."""
    return pump(limit=limit, want=rb"\n\[\d+\] > $")


pump(until_idle=1.0, limit=30)
if not extra:
    t = time.time()
    send(":dep polars candle")
    ok = prompt_after_newline(1500)
    log.write(f"\n### :dep returned to a prompt: {ok} after {time.time() - t:.1f} s\n".encode())
for line in open(script).read().splitlines():
    if line.strip() == "" or line.strip().startswith("//"):
        continue
    send(line)
    pump(until_idle=0.15, limit=10)
quoted = ", ".join('"' + a.replace("\\", "\\\\").replace('"', '\\"') + '"' for a in args)
time.sleep(1)
t = time.time()
send(f"run([{quoted}])")
ok = prompt_after_newline(3600)
log.write(f"\n### run returned to a prompt: {ok} after {time.time() - t:.1f} s\n".encode())
send(":quit")
pump(limit=3)
log.close()
# success is explicit: the workflow's own marker after the run call, and no
# compile or runtime error there (a return to a prompt alone proves nothing)
text = strip(buf).decode(errors="replace")
after = text[text.rfind("run(["):]
errors = [l for l in after.splitlines() if re.match(r"^(\[\d+\] > )?(error|runtime error)", l.strip())]
marker = re.search(r"WORKFLOW OK \w+", after)
# the run's own final evaluation must be the successful `Ok(())` every
# workflow returns: a marker followed by a returned `Err` is a failure
final = re.search(r"^\[\d+\] (Ok|Err)\(", after, re.M)
returned_ok = bool(final) and final.group(1) == "Ok"
passed = bool(ok and marker and returned_ok and not errors)
print(f"session: {'OK' if passed else 'FAILED'}"
      f" (prompt {ok}, marker {bool(marker)}, returned {final.group(1) if final else 'nothing'}, errors {len(errors)})")
sys.exit(0 if passed else 1)
