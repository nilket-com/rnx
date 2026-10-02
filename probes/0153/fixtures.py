"""Record 0153: synthetic fixtures and threshold-boundary controls (plans/0153
section 5b), run before any D3 inference. The production code
(evaluate.py, pure Python) is checked against an independent computation
(numpy with explicit f64 and f32 casts).

  fixtures.py"""
import math, os, struct, sys
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import evaluate as ev

ok = True


def check(name, good, detail=""):
    global ok
    ok &= bool(good)
    print(f"{'pass' if good else 'WRONG'}: {name}{': ' + str(detail) if detail else ''}")


rng = np.random.default_rng(153)


def unit(v):
    v = np.asarray(v, dtype=np.float32)
    return (v / np.sqrt((v.astype(np.float64) ** 2).sum())).astype(np.float32)


# --- centroids: the independent computation ---------------------------------
E = {n: [float(x) for x in unit(rng.standard_normal(384))] for n in range(1, 13)}
train = [(1, "bug"), (5, "bug"), (3, "bug"), (2, "feature"), (4, "feature"), (6, "question")]
got = ev.centroids(train, E)


def numpy_centroid(members):
    m = np.zeros(384, dtype=np.float64)
    for n in sorted(members):
        m = m + np.asarray(E[n], dtype=np.float32).astype(np.float64)
    norm = math.sqrt(float((m * m).sum()))
    return (m / norm).astype(np.float32)


for q, members in (("bug", [1, 5, 3]), ("feature", [2, 4]), ("question", [6])):
    want = numpy_centroid(members)
    check(f"centroid {q} equals numpy's f64 sum / norm rounded to f32, bit for bit",
          [struct.pack("<f", x) for x in got[q]] == [struct.pack("<f", float(x)) for x in want])
check("a queue with no training ticket is absent", "documentation" not in got)
zero = {1: [1.0] + [0.0] * 383, 2: [-1.0] + [0.0] * 383, 3: [0.0, 1.0] + [0.0] * 382}
gz = ev.centroids([(1, "bug"), (2, "bug"), (3, "feature")], zero)
check("a queue whose sum has norm 0 is absent", "bug" not in gz and "feature" in gz)
check("fewer than two present queues gives mandatory REVIEW", ev.score_c(3, zero, gz) is None)
check("a ticket without an embedding gives mandatory REVIEW", ev.score_c(99, E, got) is None)

# cosine and margin against numpy
pred = ev.score_c(7, E, got)
s = {q: float(np.dot(np.asarray(E[7], np.float32).astype(np.float64), np.asarray(got[q], np.float32).astype(np.float64)))
     for q in got}
top = max(ev.QUEUES, key=lambda q: (s.get(q, -9), -ev.QUEUES.index(q)))
second = max(v for q, v in s.items() if q != top)
check("C's prediction and margin match numpy's f64 dot products (to 1e-12)",
      pred[0] == top and abs(pred[1] - (s[top] - second)) <= 1e-12, (pred, top))

# ties to the queue order
check("an exact tie goes to the earlier queue", ev.argmax_queue({"question": 0.5, "feature": 0.5, "bug": 0.1})[0] == "feature")

# folds: zero-based rank of (sha256 hex, number) mod 5
import hashlib
nums = list(range(100, 140))
f = ev.folds(nums)
order = sorted(nums, key=lambda n: (hashlib.sha256(f"rnx-0153-fold:{n}".encode()).hexdigest(), n))
check("folds are the zero-based (hash, number) rank mod 5", all(f[n] == i % 5 for i, n in enumerate(order)))

# cross-fit: no ticket is scored by a centroid containing it
E2 = {n: [float(x) for x in unit(rng.standard_normal(384))] for n in nums}
ref = {n: ev.QUEUES[n % 4] for n in nums}
cf = ev.system_c_crossfit(nums, E2, ref)
leak = False
fo = ev.folds(nums)
for n in nums:
    cents = ev.centroids([(m, ref[m]) for m in nums if fo[m] != fo[n]], E2)
    leak |= cf[n] != ev.score_c(n, E2, cents)
    leak |= n in [m for m in nums if fo[m] != fo[n]]
check("cross-fit scores each ticket with centroids from the other four folds only", not leak)

# N: stable softmax
check("p(entail) is the stable 3-way softmax in f64",
      abs(ev.p_entail([1.0, 3.0, -2.0]) - math.exp(3) / (math.exp(1) + math.exp(3) + math.exp(-2))) <= 1e-15)
check("p(entail) is stable for large logits", ev.p_entail([1000.0, 1001.0, 999.0]) > 0.5)

# --- the policy -------------------------------------------------------------
ref = {1: "bug", 2: "bug", 3: "feature", 4: "question", 5: "bug"}
pred = {1: ("bug", 0.5), 2: ("bug", 0.5), 3: ("bug", 0.25), 4: None, 5: ("bug", 0.75)}
r = ev.at_threshold(pred, ref, 0.5)
check("confidence == t auto-routes", set(r["auto_set"]) == {1, 2, 5}, r["auto_set"])
below = math.nextafter(0.5, 0.0)
r2 = ev.at_threshold({**pred, 1: ("bug", below)}, ref, 0.5)
check("the next lower representable confidence does not", 1 not in r2["auto_set"])
check("tie blocks move together (both 0.5 tickets in or out)",
      {1, 2} <= set(ev.at_threshold(pred, ref, 0.5)["auto_set"]) and not ({1, 2} & set(ev.at_threshold(pred, ref, 0.6)["auto_set"])))
check("mandatory REVIEW never auto-routes, even at t = 0", 4 not in ev.at_threshold(pred, ref, 0.0)["auto_set"])
check("mandatory REVIEW counts in the denominator", ev.at_threshold(pred, ref, 0.0)["N"] == 5)
check("mandatory REVIEW adds no threshold candidate", ev.candidates(pred) == [0.25, 0.5, 0.75])
check("candidates are the distinct finite confidences", ev.candidates({1: ("bug", 0.5), 2: ("bug", 0.5)}) == [0.5])
none = {n: None for n in ref}
r3 = ev.at_threshold(none, ref, 0.0)
check("zero auto-routes leave E undefined, not 0", r3["E"] is None)
check("a system with zero auto-routes is ineligible", ev.choose(none, ref, ev.candidates(none)) == (None, None))
t, rr = ev.choose(pred, ref, ev.candidates(pred))
check("the threshold is the lowest candidate with E <= 0.10", t == 0.5 and rr["E"] == 0.0, (t, rr and rr["E"]))
check("K's only candidate is 1", ev.candidates(pred, only_one=True) == [1.0])
lo, hi = ev.wilson(0, 10)
check("Wilson interval for 0/10, clamped to [0, 1]", lo == 0.0 and abs(hi - 0.2775) < 1e-3, (lo, hi))

# K: exactly one queue matches
rules = ev.keyword_rules()
k = ev.system_k({1: "Panics on empty array", 2: "How do I use modules?", 3: "Something", 4: "Docs typo in book",
                 5: "Add support for X, it panics"}, rules)
check("K: exactly one match routes; none or several give mandatory REVIEW",
      k[1] == ("bug", 1.0) and k[2] == ("question", 1.0) and k[3] is None and k[4] == ("documentation", 1.0) and k[5] is None, k)

# --- review R1: an invalid embedding through the evaluation path ------------
# ticket 3's embedding is zero_norm: validate.validate leaves it out of E and
# C (its rows are absent) while its NLI rows and status stay; Z and C must
# give it mandatory REVIEW, K and N must be unaffected, it stays in every
# denominator, and it adds no Z or C threshold candidate
nums = [1, 2, 3, 4]
Ev = {n: [float(x) for x in unit(rng.standard_normal(384))] for n in (1, 2, 4)}
Cv = {n: [0.1, 0.2, 0.3, 0.05, 0.01] for n in (1, 2, 4)}
Lv = [(n, 0, lab, [0.0, 1.0 if lab == "bug" else 0.0, 0.0]) for n in nums for lab in ev.LABELS]
status = {n: "ok" for n in nums}
zv = ev.system_z(nums, Cv, ev.LABELS)
nv = ev.system_n(nums, status, Lv, ev.LABELS)
cv = ev.system_c_crossfit(nums, Ev, {1: "bug", 2: "feature", 3: "bug", 4: "question"})
kv = ev.system_k({1: "x", 2: "y", 3: "Panics on start", 4: "z"}, rules)
check("an invalid embedding gives Z mandatory REVIEW", zv[3] is None and all(zv[n] is not None for n in (1, 2, 4)))
check("an invalid embedding gives C mandatory REVIEW", cv[3] is None)
check("N is unaffected by an invalid embedding", nv[3] == ("bug", ev.p_entail([0.0, 1.0, 0.0])))
check("K is unaffected by an invalid embedding", kv[3] == ("bug", 1.0))
refv = {1: "bug", 2: "feature", 3: "bug", 4: "question"}
check("the invalid-embedding ticket stays in every denominator", ev.at_threshold(zv, refv, 0.0)["N"] == 4)
check("it adds no Z or C threshold candidate", len(ev.candidates(zv)) == len({zv[n][1] for n in (1, 2, 4)}))
check("C's centroids are trained without it", 3 not in Ev and ev.centroids([(n, refv[n]) for n in nums if n in Ev], Ev) is not None)
nan_c = {1: [float("nan"), 0.2, 0.3, 0.05, 0.01]}
try:
    ev.system_z([1], nan_c, ev.LABELS)
    stopped = False
except AssertionError:
    stopped = True
check("a non-finite derived Z score where an embedding is valid stops (5a)", stopped)

print("all fixtures and boundary controls behave" if ok else "CONTROLS FAILED")
sys.exit(0 if ok else 1)
