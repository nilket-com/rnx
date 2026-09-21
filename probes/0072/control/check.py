import json, sys
here = sys.argv[1]
exp = json.load(open(f"{here}/expected.json"))
ok = True
def report(label, got, want):
    global ok
    got, want = sorted(got), sorted(want)
    if got != want:
        ok = False
        print(f"MISMATCH {label}\n  want {want}\n  got  {got}")
    else:
        print(f"ok {label} ({len(got)})")
for cfg in ("default", "extra"):
    inv = json.load(open(f"{here}/out/{cfg}/result/inventory.json"))
    calls = {c["canonical_path"]: c for c in inv["callables"]}
    want = list(exp["default"]["callables"]) + (exp["extra"]["additional_callables"] if cfg == "extra" else [])
    report(f"{cfg} callables", calls.keys(), want)
    excluded = {p for p, c in calls.items() if c["bucket"] == "unsupported"}
    report(f"{cfg} excluded", excluded, exp["default"]["excluded"].keys())
    eligible = len(calls) - len(excluded)
    print(("ok" if eligible == exp[cfg]["eligible"] else "MISMATCH") + f" {cfg} eligible {eligible} (want {exp[cfg]['eligible']})")
    if eligible != exp[cfg]["eligible"]: ok = False
    thing = [s for s in inv["supporting"] if s["canonical_path"] == "ctrl_dep::Thing"][0]
    report(f"{cfg} Thing found_paths", thing["found_paths"], exp["default"]["found_paths_for_Thing"])
    for tr, impls in exp["default"]["trait_implementors"].items():
        ms = [c for c in inv["callables"] if c["owner"] == tr]
        report(f"{cfg} implementors of {tr}", ms[0]["implementors"] if ms else [], impls)
    for kind, names in exp["default"]["supporting"].items():
        report(f"{cfg} supporting {kind}", [s["canonical_path"] for s in inv["supporting"] if s["kind"] == kind], names)
    for p in exp["default"]["unreachable_public"]:
        if p in calls or any(s["canonical_path"] == p for s in inv["supporting"]):
            ok = False; print(f"MISMATCH {cfg}: {p} should be unreachable")
    report(f"{cfg} unknown", inv["unknown"], exp["default"]["unknown"])
    for path, want in exp["default"]["buckets"].items():
        got = calls.get(path, {}).get("bucket")
        if got != want:
            ok = False; print(f"MISMATCH {cfg} bucket of {path}: want {want}, got {got} rules={calls.get(path, {}).get('rules')}")
    print(f"ok {cfg} buckets checked ({len(exp['default']['buckets'])})")
print("CONTROL PASS" if ok else "CONTROL FAIL")
sys.exit(0 if ok else 1)
