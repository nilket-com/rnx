"""Record 0143: NumPy-written fixtures for the interchange controls.

  fixtures.py OUT_DIR

Valid files (every adapter dtype, ranks 0 to 6, a zero-length axis, a
non-contiguous source, np.savez with several arrays) and the malformed
archives only NumPy or Python's zipfile produce (a deflated entry, a
duplicate name, an archive comment, a forged end record inside one)."""
import pathlib, sys, zipfile, io
import numpy as np

out = pathlib.Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
rng = np.random.default_rng(143)
dtypes = {"f32": np.float32, "f64": np.float64, "i64": np.int64, "u32": np.uint32, "u8": np.uint8}
shapes = [(), (5,), (2, 3), (2, 0, 3), (1, 2, 1, 3), (2, 1, 2, 1, 2), (1, 2, 1, 2, 1, 2)]
for name, dt in dtypes.items():
    for s in shapes:
        n = int(np.prod(s)) if s else 1
        if dt in (np.float32, np.float64):
            v = rng.standard_normal(n).astype(dt)
            if n >= 3:
                v[0], v[1], v[2] = np.nan, np.inf, -0.0
        else:
            v = rng.integers(0, np.iinfo(dt).max if dt != np.int64 else 2**62, n, dtype=dt)
        a = v.reshape(s)
        tag = "x".join(map(str, s)) or "scalar"
        np.save(out / f"{name}-{tag}.npy", a)
# a transposed (non-contiguous) array: np.save writes it C-ordered
np.save(out / "f32-transposed.npy", np.ascontiguousarray(np.arange(6, dtype=np.float32).reshape(2, 3).T))
np.savez(out / "several.npz", weights=rng.standard_normal((3, 4)).astype(np.float32),
         ids=np.arange(7, dtype=np.int64), mask=np.array([1, 0, 1], dtype=np.uint8))
np.save(out / "f16.npy", np.zeros(3, dtype=np.float16))
np.save(out / "i32.npy", np.zeros(3, dtype=np.int32))
np.save(out / "bigendian.npy", np.zeros(3, dtype=">f4"))
np.save(out / "fortran.npy", np.asfortranarray(np.zeros((2, 3), dtype=np.float32)))
np.savez_compressed(out / "compressed.npz", a=np.zeros(3, dtype=np.float32))
one = io.BytesIO(); np.save(one, np.zeros(3, dtype=np.float32)); one = one.getvalue()
with zipfile.ZipFile(out / "duplicate.npz", "w") as z:
    import warnings
    warnings.simplefilter("ignore")
    z.writestr("a.npy", one); z.writestr("a.npy", one)
with zipfile.ZipFile(out / "comment.npz", "w") as z:
    z.writestr("a.npy", one); z.comment = b"hello"
# a forged end record inside the comment: a second, empty directory
forged = b"PK\x05\x06" + b"\x00" * 18
with zipfile.ZipFile(out / "forged.npz", "w") as z:
    z.writestr("a.npy", one); z.comment = forged
with zipfile.ZipFile(out / "notnpy.npz", "w") as z:
    z.writestr("a.txt", b"hello")
print(len(list(out.iterdir())), "fixtures")
