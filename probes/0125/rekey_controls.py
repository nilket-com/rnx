#!/usr/bin/env python3
"""Record 0125 (review): rekey.py's controls on synthetic inventories. A
renumbered, unchanged callable maps; a semantic change (impl_assoc,
trait_lifetimes, or any field outside NOT_IDENTITY) is refused, never mapped
as unchanged; an ambiguous identity on either side is refused; a keyed
release row whose path is not the mapped callable's is refused."""
import json
import os
import subprocess
import sys
import tempfile

here = os.path.dirname(os.path.abspath(__file__))
base = {"canonical_path": "polars_x::T as core::ops::Add", "kind": "foreign_trait_impl", "owner": "polars_x::T",
        "name": "add", "params": [{"name": "rhs", "ty_canonical": "polars_x::T"}], "ret": "Self::Output",
        "impl_assoc": [["Output", "polars_x::T"]], "found_paths": ["polars::T"], "bucket": "mechanical"}


def run(old, new, frozen_keys, release_rows=()):
    d = tempfile.mkdtemp(prefix="rekey0125-")
    paths = {}
    for name, cs in (("old", old), ("new", new)):
        paths[name] = os.path.join(d, f"{name}.json")
        json.dump({"callables": cs}, open(paths[name], "w"))
    frozen = os.path.join(d, "frozen.json")
    json.dump([{"key": k, "id": f"id{i}", "rune": f"r{i}", "summary": None} for i, k in enumerate(frozen_keys)], open(frozen, "w"))
    release = os.path.join(d, "release.toml")
    open(release, "w").write("".join(f'[[x]]\nkey = "{k}"\npath = "{p}"\n' for k, p in release_rows))
    r = subprocess.run([sys.executable, os.path.join(here, "rekey.py"), paths["old"], paths["new"], frozen, release],
                       capture_output=True, text=True)
    return r.returncode, r.stdout + r.stderr, json.load(open(frozen)) if r.returncode == 0 else None


def c(key, **over):
    x = dict(base, key=key)
    x.update(over)
    return x


# a renumbered, unchanged callable maps; reachability may grow
code, out, frozen = run([c("k:1")], [c("k:9", found_paths=["polars::T", "polars::prelude::T"])], ["k:1"])
assert code == 0 and frozen[0]["key"] == "k:9", out
# semantic drift is refused, never mapped as unchanged
for field, value in (("impl_assoc", [["Output", "polars_x::U"]]), ("trait_lifetimes", ["'a"]), ("future_field", 1)):
    code, out, _ = run([c("k:1")], [c("k:9", **{field: value})], ["k:1"])
    assert code != 0 and "0 new callables with its identity" in out, (field, out)
# an ambiguous identity is refused, old side and new side
code, out, _ = run([c("k:1"), c("k:2")], [c("k:9")], ["k:1"])
assert code != 0 and "has 2 old and 1 new" in out, out
code, out, _ = run([c("k:1")], [c("k:8"), c("k:9")], ["k:1"])
assert code != 0 and "has 1 old and 2 new" in out, out
# a keyed release row must name the mapped callable's path
code, out, _ = run([c("k:1")], [c("k:9")], [], [("k:1", "polars_x::Other")])
assert code != 0 and "names polars_x::Other" in out, out
print("rekey controls: ok (renumbering maps; impl_assoc, trait_lifetimes and a new field refuse; ambiguity refuses both sides; a mismatched release path refuses)")
