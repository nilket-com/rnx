"""Record 0143: E6's NumPy gates, kept separate from the exact twin gate.

1. The round trip (exact): np.load reads e6.npz and centroids.npy; the
   centroids and assignments equal the twin's trace bit for bit, and the
   two centroid files are identical.
2. NumPy's own k-means (declared tolerances): the frozen algorithm in
   float32 NumPy, on the exported embeddings. Assignments must match the
   twin's except where a ticket's top two similarities differ by less than
   1e-6 (each listed); centroids within 1e-5 absolute; iterations equal.
   No whole-k-means bit equality is promised across libraries.

  numpy_e6.py DIR TWIN_TRACE"""
import struct, sys, time
import numpy as np

d, twin = sys.argv[1], sys.argv[2]
lines = open(twin).read().splitlines()
t_assign = np.array([int(l.split("\t")[2]) for l in lines if l.startswith("assign\t")], dtype=np.uint32)
t_cent = np.array([[float(x) for x in l.split("\t")[2].split(",")] for l in lines if l.startswith("centroid\t")], dtype=np.float32)
t_iter = int([l for l in lines if l.startswith("iterations\t")][0].split("\t")[1])
t_init = [int(x) for x in lines[0].split("\t")[1].split(",")]

z = np.load(f"{d}/e6.npz")
print("archive:", {k: (z[k].dtype.str, z[k].shape) for k in z.files})
assert z.files == ["embeddings", "centroids", "assignments"], z.files
assert z["centroids"].dtype == np.float32 and z["assignments"].dtype == np.uint32
assert z["centroids"].tobytes() == t_cent.tobytes(), "centroids differ from the twin's bits"
assert np.array_equal(z["assignments"], t_assign), "assignments differ from the twin's"
c = np.load(f"{d}/centroids.npy")
assert c.tobytes() == z["centroids"].tobytes(), "centroids.npy differs from the archive"
print("round trip: EXACT (centroids and assignments equal the twin's bits; centroids.npy equals the archive)")

U = z["embeddings"]
k = 6
def unit(x):
    n = np.sqrt((x * x).sum(axis=1, keepdims=True, dtype=np.float32))
    if not (np.all(n > 0) and np.all(np.isfinite(n))):
        raise SystemExit("a zero or non-finite norm")
    return x / n
t0 = time.perf_counter()
g = unit(U.mean(axis=0, keepdims=True, dtype=np.float32))
chosen = [int(np.argmax(U @ g.T))]
while len(chosen) < k:
    chosen.append(int(np.argmin((U @ U[chosen].T).max(axis=1))))
C = U[chosen]
prev, it, moves = None, 0, []
while it < 50:
    it += 1
    S = U @ C.T
    a = np.argmax(S, axis=1).astype(np.uint32)
    sizes = np.bincount(a, minlength=k)
    for c in range(k):
        if sizes[c] == 0:
            own = S[np.arange(len(a)), a]
            ok = sizes[a] >= 2
            dnr = int(np.flatnonzero(ok)[np.argmin(own[ok])])
            sizes[a[dnr]] -= 1; a[dnr] = c; sizes[c] = 1
            moves.append((it, c, dnr))
    if prev is not None and np.array_equal(prev, a):
        break
    sums = np.zeros((k, U.shape[1]), dtype=np.float32)
    np.add.at(sums, a, U)
    C = unit(sums / np.bincount(a, minlength=k)[:, None].astype(np.float32))
    prev = a.copy()
t1 = time.perf_counter()
S = U @ C.T
top2 = np.sort(S, axis=1)[:, -2:]
near = np.flatnonzero(top2[:, 1] - top2[:, 0] < 1e-6)
diff = np.flatnonzero(a != t_assign)
unexplained = [int(i) for i in diff if i not in set(near.tolist())]
dc = float(np.abs(C - t_cent).max())
print(f"numpy k-means: init {chosen} (twin {t_init}), {it} iterations (twin {t_iter}), {len(moves)} moves, {1e3 * (t1 - t0):.1f} ms")
print(f"assignments differing: {len(diff)}; near-ties (< 1e-6): {near.tolist()}; unexplained: {unexplained}")
print(f"centroid max abs difference: {dc:.3e} (bound 1e-5)")
ok = it == t_iter and not unexplained and dc <= 1e-5
print("numpy gate: PASS" if ok else "numpy gate: FAIL")
sys.exit(0 if ok else 1)
