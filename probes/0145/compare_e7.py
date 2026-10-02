"""Record 0145: E7's exact gate. Each trace is validated on its own first,
against the expected channels C and days N; then the two are compared, every
value as f64 bits (Rune and Rust may write a value in different notation).

A trace, per channel c = 0 .. C-1 in order, and nothing else:
- `trend\tc\t<N finite values>`;
- `z\tc\t<N finite values>`;
- `event\tc\t<peak day>\t<length>\t<z>`: exactly the frozen rule's events
  recomputed from this trace's own z (so none can be missing); at most 5, peak days in [0, N),
  lengths at least 1, z >= 4 and equal to that day's z, ordered by z
  descending (ties: the earlier peak), the run around the peak exactly
  `length` days of z >= 4, the peak its highest z (ties: the earliest);
- `weekday\tc\t<7 finite values>`;
- `busiest\tc\t<week in [0, N // 7)>\t<finite value>`.

  compare_e7.py SCRIPT_TRACE TWIN_TRACE C N"""
import math, struct, sys


def fail(msg):
    sys.exit(f"FAIL: {msg}")


def f64(text, what):
    try:
        v = float(text)
    except ValueError:
        fail(f"{what}: {text!r} is not a number")
    if not math.isfinite(v):
        fail(f"{what}: {text} is not finite")
    return v


def bits(v):
    return struct.pack("<d", v)


def validate(path, C, N):
    lines = open(path).read().splitlines()
    if not lines:
        fail(f"{path}: empty")
    pos = 0
    out = []

    def take(kind, ch, arity):
        nonlocal pos
        if pos >= len(lines):
            return None
        f = lines[pos].split("\t")
        if f[0] != kind:
            return None
        if len(f) != arity or f[1] != str(ch):
            fail(f"{path} line {pos + 1}: {kind} for channel {ch} malformed")
        pos += 1
        return f

    for ch in range(C):
        f = take("trend", ch, 3) or fail(f"{path}: channel {ch} has no trend line")
        trend = [f64(x, f"{path} trend {ch}") for x in f[2].split(",")]
        f = take("z", ch, 3) or fail(f"{path}: channel {ch} has no z line")
        z = [f64(x, f"{path} z {ch}") for x in f[2].split(",")]
        if len(trend) != N or len(z) != N:
            fail(f"{path}: channel {ch} has {len(trend)} trend and {len(z)} z values, want {N}")
        events = []
        while (f := take("event", ch, 5)):
            try:
                p, length = int(f[2]), int(f[3])
            except ValueError:
                fail(f"{path}: an event's day or length is not an integer")
            zv = f64(f[4], f"{path} event z")
            if not (0 <= p < N) or length < 1 or zv < 4 or bits(zv) != bits(z[p]):
                fail(f"{path}: event {(p, length, zv)} disagrees with channel {ch}'s z")
            s = p
            while s > 0 and z[s - 1] >= 4:
                s -= 1
            e = p
            while e + 1 < N and z[e + 1] >= 4:
                e += 1
            run = z[s:e + 1]
            if e - s + 1 != length or max(run) != zv or run.index(max(run)) + s != p:
                fail(f"{path}: event {(p, length)} is not its run's earliest highest peak")
            events.append((zv, p, length))
        if len(events) > 5 or events != sorted(events, key=lambda e: (-e[0], e[1])):
            fail(f"{path}: channel {ch}'s events are too many or out of order")
        # complete: the events are exactly those the frozen rule gives from
        # this trace's own z (every run of z >= 4, the five largest peaks)
        runs, t = [], 0
        while t < N:
            if z[t] >= 4:
                s0 = t
                while t < N and z[t] >= 4:
                    t += 1
                seg = z[s0:t]
                runs.append((max(seg), s0 + seg.index(max(seg)), t - s0))
            else:
                t += 1
        if events != sorted(runs, key=lambda e: (-e[0], e[1]))[:5]:
            fail(f"{path}: channel {ch}'s events are not the rule's from its z")
        f = take("weekday", ch, 3) or fail(f"{path}: channel {ch} has no weekday line")
        wd = [f64(x, f"{path} weekday {ch}") for x in f[2].split(",")]
        if len(wd) != 7:
            fail(f"{path}: channel {ch} has {len(wd)} weekday values")
        f = take("busiest", ch, 4) or fail(f"{path}: channel {ch} has no busiest line")
        try:
            w = int(f[2])
        except ValueError:
            fail(f"{path}: the busiest week is not an integer")
        if not (0 <= w < N // 7):
            fail(f"{path}: busiest week {w} is outside [0, {N // 7})")
        bv = f64(f[3], f"{path} busiest value")
        out.append((trend, z, events, wd, w, bv))
    if pos != len(lines):
        fail(f"{path} line {pos + 1}: unexpected {lines[pos].split(chr(9))[0]!r}")
    return out


if __name__ == "__main__":
    C, N = int(sys.argv[3]), int(sys.argv[4])
    a, b = validate(sys.argv[1], C, N), validate(sys.argv[2], C, N)
    for ch, (x, y) in enumerate(zip(a, b)):
        for i, name in enumerate(("trend", "z")):
            if [bits(v) for v in x[i]] != [bits(v) for v in y[i]]:
                fail(f"channel {ch}: {name} differs in f64 bits")
        if [(bits(e[0]), e[1], e[2]) for e in x[2]] != [(bits(e[0]), e[1], e[2]) for e in y[2]]:
            fail(f"channel {ch}: events differ")
        if [bits(v) for v in x[3]] != [bits(v) for v in y[3]]:
            fail(f"channel {ch}: the weekday profile differs")
        if (x[4], bits(x[5])) != (y[4], bits(y[5])):
            fail(f"channel {ch}: the busiest week differs")
    events = sum(len(x[2]) for x in a)
    print(f"BIT-EQUAL (each trace validated first, C={C} N={N}): {2 * C * N} trend and z values, "
          f"{events} events, {7 * C} weekday values, {C} busiest weeks, all in f64 bits")
