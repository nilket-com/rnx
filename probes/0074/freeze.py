#!/usr/bin/env python3
"""Record 0074: the frozen inputs, as digests.

  freeze.py write <repo root>    # (re)write probes/0074/frozen.json from the tree; only for the accepted revision
  freeze.py verify <repo root>   # exit nonzero if anything the probe depends on differs from frozen.json
"""
import glob, hashlib, json, os, subprocess, sys
mode, root = sys.argv[1], sys.argv[2]
here = os.path.join(root, "probes/0074")
frozen_path = sys.argv[3] if len(sys.argv) > 3 else os.path.join(here, "frozen.json")
INPUTS = {
    "generator": "tools/polars-gen/src/*.rs",
    "generator_manifest": "tools/polars-gen/Cargo.toml",
    "generator_lock": "tools/polars-gen/Cargo.lock",
    "extractor": "probes/0072/extract/src/*.rs",
    "extractor_manifest": "probes/0072/extract/Cargo.toml",
    "extractor_lock": "probes/0072/extract/Cargo.lock",
    "adapter_sources": "adapters/polars/src/*.rs",
    "adapter_generated": "adapters/polars/src/generated/*.rs",
    "adapter_tests": "adapters/polars/tests/*",
    "adapter_manifest": "adapters/polars/Cargo.toml",
    "baseline_inventory": "probes/0072/out/0.55.2-adapter/result/inventory.json",
    "baseline_summary": "probes/0072/out/0.55.2-adapter/result/summary.json",
    "baseline_pins": "probes/0072/out/0.55.2-adapter/pins.json",
    "baseline_surface": "adapters/polars/surface.json",
    "baseline_oracle_results": "adapters/polars/oracle-results.json",
    "doc_lock": "probes/0074/locks/doc-rc2.lock",
    "adapter_lock": "probes/0074/locks/adapter.lock",
    "doc_script": "probes/0074/doc.sh",
}
def digest(paths):
    h = hashlib.sha256()
    for p in sorted(paths):
        h.update(os.path.relpath(p, root).encode()); h.update(b"\0"); h.update(open(p, "rb").read()); h.update(b"\0")
    return h.hexdigest()
def current():
    out = {"accepted_generator_revision": "98885e0", "written_at_revision": subprocess.run(["git", "-C", root, "rev-parse", "HEAD"], capture_output=True, text=True).stdout.strip(), "inputs": {}}
    for k, pat in INPUTS.items():
        paths = sorted(glob.glob(os.path.join(root, pat)))
        out["inputs"][k] = {"pattern": pat, "files": len(paths), "sha256": digest(paths) if paths else None}
    return out
if mode == "write":
    json.dump(current(), open(frozen_path, "w"), indent=1)
    print("frozen.json written")
elif mode == "verify":
    want = json.load(open(frozen_path))
    have = current()
    bad = [k for k in want["inputs"] if want["inputs"][k]["sha256"] != have["inputs"].get(k, {}).get("sha256")]
    if bad:
        print("frozen inputs differ from frozen.json: " + ", ".join(bad)); sys.exit(1)
    print(f"frozen inputs verified ({len(want['inputs'])} groups, generator accepted at {want['accepted_generator_revision']})")
