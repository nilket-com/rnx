"""Record 0140: missing methods on adapter types, named through the real
binary, in a session, in eval and in run. Exits 1 if any expected sentence is
missing or any hash remains for a provable case.

  names.py RNX MODEL_DIR WORK_DIR

RNX is a binary with the Polars and Candle adapters (probes/0134/runner,
or a pushed binary under RNX_DEP=1, which runs `:dep polars candle` first)."""
import os, pathlib, pty, re, select, subprocess, sys, time

rnx, model, work = sys.argv[1], sys.argv[2], pathlib.Path(sys.argv[3])
work.mkdir(parents=True, exist_ok=True)
dep = os.environ.get("RNX_DEP") == "1"
plain = lambda b: re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07", "", b.decode("utf-8", "replace")).replace("\r", "")

HINT = "(this value is a `Result`; did you mean to unwrap it with `?` first?)"
cases = [
    # (label, session input, expected sentence)
    ("make a frame", 'let df = polars::DataFrame::new(1, [polars::Series::from_iter_i64([1]).unwrap().with_name("a").into_column()]).unwrap();', None),
    ("0139: DataFrame has no clone", "df.clone()", "no method `clone` on `::polars::DataFrame`"),
    ("0139: slice without ?", 'df.slice(0, 1).select_(["a"])', f"no method `select_` on `::std::result::Result` {HINT}"),
    ("0139: with_order_descending_multi without ?", "polars::SortMultipleOptions::new().with_order_descending_multi([true]).with_maintain_order(true)", f"no method `with_maintain_order` on `::std::result::Result` {HINT}"),
    ("Series", "polars::Series::from_iter_i64([1]).unwrap().frobnicate()", "no method `frobnicate` on `::polars::Series`"),
    ("Tensor", 'candle::Tensor::zeros([2], "f32").unwrap().frobnicate()', "no method `frobnicate` on `::candle::Tensor`"),
    ("TextEncoder", f'candle::TextEncoder::load("{model}").unwrap().frobnicate()', "no method `frobnicate` on `::candle::TextEncoder`"),
    ("LazyFrame", "df.lazy().frobnicate()", "no method `frobnicate` on `::polars::LazyFrame`"),
]


def session():
    pid, fd = pty.fork()
    if pid == 0:
        os.execv(rnx, ["rnx"] + ([] if dep else ["repl"]))
    out = b""

    def pump(limit, want=None):
        nonlocal out
        end = time.time() + limit
        while time.time() < end:
            r, _, _ = select.select([fd], [], [], 0.2)
            if r:
                try:
                    chunk = os.read(fd, 65536)
                except OSError:
                    return
                if not chunk:
                    return
                out += chunk
                if want and re.search(want, plain(out[-4000:].replace(b"\r", b""))):
                    # a prompt can be drawn before the error text arrives:
                    # read on until the output is idle for 0.5 s
                    while select.select([fd], [], [], 0.5)[0]:
                        try:
                            more = os.read(fd, 65536)
                        except OSError:
                            return
                        if not more:
                            return
                        out += more
                    return

    pump(30, r"\[\d+\] > $")
    if dep:
        os.write(fd, b":dep polars candle\n")
        pump(1500, r"\n\[\d+\] > $")
    marks = []
    for label, line, _ in cases:
        marks.append(len(out))
        os.write(fd, line.encode() + b"\n")
        pump(120, r"\n\[\d+\] > $")
    marks.append(len(out))
    os.write(fd, b":quit\n")
    pump(5)
    os.close(fd)
    os.waitpid(pid, 0)
    return [plain(out[marks[i]:marks[i + 1]]) for i in range(len(cases))]


ok = True
lines = []
for (label, line, want), text in zip(cases, session()):
    errors = [l for l in text.splitlines() if "runtime error" in l or "no method" in l or "Missing instance function" in l]
    shown = errors[-1] if errors else "(no error)"
    if want is None:
        good = not errors
    else:
        good = any(want in l for l in errors) and not any("Missing instance function" in l for l in errors)
    ok &= good
    lines.append(f"{'ok' if good else 'WRONG'}: session, {label}: {shown.strip()}")
    if not good:
        (work / f"segment-{len(lines)}.txt").write_text(text)

# eval and run: only with a binary that has the adapters linked; under
# RNX_DEP=1 the adapters exist only in the session that ran :dep
if dep:
    print("\n".join(lines + ["(eval and run skipped: a :dep binary has no adapters outside a session)"]))
    print("all named as expected" if ok else "FAILED")
    sys.exit(0 if ok else 1)
env = dict(os.environ)
e = subprocess.run([rnx, "eval", "polars::Series::from_iter_i64([1]).unwrap().frobnicate()"], capture_output=True, text=True)
want = "no method `frobnicate` on `::polars::Series`"
good = want in e.stderr + e.stdout
ok &= good
lines.append(f"{'ok' if good else 'WRONG'}: eval, Series: {(e.stderr.strip().splitlines() or ['(nothing)'])[0]}")
script = work / "clone.rn"
script.write_text('pub fn main(_) {\n    let df = polars::DataFrame::new(1, [polars::Series::from_iter_i64([1])?.with_name("a").into_column()])?;\n    df.clone()\n}\n')
r = subprocess.run([rnx, "run", str(script)], capture_output=True, text=True)
want = "no method `clone` on `::polars::DataFrame`"
good = want in r.stderr
ok &= good
lines.append(f"{'ok' if good else 'WRONG'}: run, DataFrame clone: {(r.stderr.strip().splitlines() or ['(nothing)'])[0]}")
print("\n".join(lines))
print("all named as expected" if ok else "FAILED")
sys.exit(0 if ok else 1)
