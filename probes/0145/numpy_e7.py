"""Record 0145: E7's frozen method in NumPy (float64), independent of rnx and
Candle. Used (1) before implementation, to state the synthetic cases'
expected outcomes in the plan, and (2) later as the tolerance gate.

  numpy_e7.py synthetic            print the synthetic cases' outcomes"""
import sys
import numpy as np

W = 29  # the centred trend window


def method(views, names=None):
    """views: (channels, days) positive counts. Returns a dict or raises."""
    v = np.asarray(views, dtype=np.float64)
    if np.any(v <= 0):
        raise ValueError("a non-positive count")
    x = np.log(v)
    k = np.ones(W) / W
    trend = np.stack([np.convolve(r, k, mode="same") for r in x])
    ones = np.convolve(np.ones(x.shape[1]), k, mode="same")
    trend = trend / ones
    r = x - trend
    med = np.stack([np.median(row) for row in r])
    mad = 1.4826 * np.stack([np.median(np.abs(row - m)) for row, m in zip(r, med)])
    if np.any(mad == 0):
        raise ValueError("zero MAD")
    z = (r - med[:, None]) / mad[:, None]
    events = []
    for c, row in enumerate(z):
        ev, t = [], 0
        while t < len(row):
            if row[t] >= 4:
                s = t
                while t < len(row) and row[t] >= 4:
                    t += 1
                seg = row[s:t]
                p = s + int(np.argmax(seg))  # first maximum: ties to the earliest
                ev.append((float(row[p]), p, t - s))
            else:
                t += 1
        ev.sort(key=lambda e: (-e[0], e[1]))
        events.append(ev[:5])
    weeks = x.shape[1] // 7
    xw = x[:, : weeks * 7].reshape(x.shape[0], weeks, 7)
    wmean = xw.mean(axis=2, keepdims=True)
    profile = np.expm1((xw - wmean).mean(axis=1))  # by position in the week
    raw = v[:, : weeks * 7].reshape(v.shape[0], weeks, 7).mean(axis=2)
    busiest = raw.argmax(axis=1)  # first maximum: ties to the earliest week
    return dict(trend=trend, z=z, events=events, profile=profile, busiest=busiest)


def base(n):
    t = np.arange(n)
    return 1000 * (1 + 0.05 * np.sin(2 * np.pi * t / 7) + 0.03 * np.sin(2 * np.pi * t / 29.5)
                   + 0.02 * np.sin(2 * np.pi * t / 3.3))


def synthetic():
    cases = {}
    a = base(400); a[200] *= 5
    cases["one-day"] = a
    b = base(400); b[250] *= 4; b[251] *= 4
    cases["two-day"] = b
    w = np.array([1.0, 1.2, 1.3, 1.25, 1.1, 0.8, 0.7])
    cases["weekly"] = 1000 * np.tile(w, 28)
    cases["constant"] = np.full(400, 1000.0)
    f = np.full(400, 1000.0); f[200] = 5000
    cases["flat-spike"] = f
    return cases


if __name__ == "__main__" and sys.argv[1] == "synthetic":
    for name, s in synthetic().items():
        try:
            out = method(s[None, :])
            ev = [(p, length, round(zv, 3)) for zv, p, length in out["events"][0]]
            prof = np.round(out["profile"][0], 6).tolist()
            print(f"{name}: events (day, length, z) {ev}; profile {prof}; busiest week {int(out['busiest'][0])}")
        except ValueError as e:
            print(f"{name}: refused: {e}")
