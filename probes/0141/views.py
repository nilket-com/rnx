"""Record 0141: one frame, many views, in a real session. Five views are
derived from one DataFrame and the frame is shown after each, unchanged;
then an in-place sort on a clone leaves it unchanged, and the same sort
through an alias changes it. The whole terminal is kept as the transcript.

  views.py RNX TRANSCRIPT

RNX is a binary with the Polars adapter (probes/0134/runner), or a pushed
binary under RNX_DEP=1, which runs `:dep polars` first."""
import os, pty, re, select, sys, time

rnx, transcript = sys.argv[1], sys.argv[2]
dep = os.environ.get("RNX_DEP") == "1"
plain = lambda b: re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]|\x1b\][^\x07]*\x07", "", b.decode("utf-8", "replace")).replace("\r", "")
DESC = "polars::SortMultipleOptions::default_().with_order_descending(true)"
lines = [
    ("frame", 'let df = polars::DataFrame::new(6, [polars::Series::from_iter_i64([101, 102, 103, 104, 105, 106])?.with_name("ticket").into_column(), polars::Series::from_iter_str(["api", "ui", "api", "data", "ui", "api"])?.with_name("team").into_column(), polars::Series::from_iter_f64([3.5, 1.0, 2.5, 4.0, 0.5, 1.5])?.with_name("hours").into_column()])?;'),
    ("show", "df"),
    ("view 1: lazy group-by", 'df.lazy().group_by([polars::col("team")])?.agg([polars::col("hours").sum().alias("total")])?.sort(["team"])?.collect()?'),
    ("show", "df"),
    ("view 2: sort by hours, descending", f'df.sort(["hours"], {DESC})?'),
    ("show", "df"),
    ("view 3: sort by team", 'df.sort(["team"], polars::SortMultipleOptions::default_())?'),
    ("show", "df"),
    ("view 4: head", "df.head(Some(2))?"),
    ("show", "df"),
    ("view 5: lazy filter", 'df.lazy().filter(polars::col("hours").gt(polars::lit(2.0)?)).collect()?'),
    ("show", "df"),
    ("clone, then sort the clone in place", f'let copy = df.clone(); copy.sort_in_place(["hours"], {DESC})?; copy'),
    ("show", "df"),
    ("alias, then sort the alias in place", f'let same = df; same.sort_in_place(["hours"], {DESC})?; df'),
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
                if want and re.search(want, plain(out[-4000:])):
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
        os.write(fd, b":dep polars\n")
        pump(1500, r"\n\[\d+\] > $")
    marks = []
    for _, line in lines:
        marks.append(len(out))
        os.write(fd, line.encode() + b"\n")
        pump(120, r"\n\[\d+\] > $")
    marks.append(len(out))
    os.write(fd, b":quit\n")
    pump(5)
    os.close(fd)
    os.waitpid(pid, 0)
    return plain(out), [plain(out[marks[i]:marks[i + 1]]) for i in range(len(lines))]


def body(text):
    # the displayed value: from its `[N] ` header (input number dropped)
    # to the blank line before the next prompt
    out, on = [], False
    for l in text.splitlines():
        m = re.match(r"^\[\d+\] (?!>)(.*)$", l)
        if not on and m:
            on, l = True, m.group(1)
        elif on and (l.strip() == "" or re.match(r"^\[\d+\] > ", l)):
            break
        if on:
            out.append(l)
    return "\n".join(out)


whole, parts = session()
errs = lambda t: [l for l in t.splitlines() if "error" in l.lower()]
blocks = [f"# {label}\n> {line}\n{body(t) or chr(10).join(errs(t)) or '(no value)'}" for (label, line), t in zip(lines, parts)]
open(transcript, "w").write("\n\n".join(blocks) + "\n")
shows = [body(t) for (label, _), t in zip(lines, parts) if label == "show"]
errors = [l for t in parts for l in t.splitlines() if "error" in l.lower()]
ok = not errors and len(set(shows)) == 1 and shows[0] != ""
copy = body(parts[-3])
alias = body(parts[-1])
ok &= copy == alias and copy != shows[0]
print(f"frame shown {len(shows)} times, identical: {len(set(shows)) == 1}")
print(f"the clone's in-place sort left the frame unchanged: {shows[-1] == shows[0]}")
print(f"the alias's in-place sort changed the frame, to the clone's sorted value: {alias == copy and alias != shows[0]}")
print(f"errors: {len(errors)}")
print("OK" if ok else "FAILED")
sys.exit(0 if ok else 1)
