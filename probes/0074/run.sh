#!/usr/bin/env bash
# Record 0074: the frozen 0073 generator against Polars 2.0 rc2.
#
#   probes/0074/run.sh              # a replay: frozen inputs verified, locks required, fresh outputs
#   probes/0074/run.sh report       # report only, from this run's outputs (nothing rebuilt)
#   PROBE_INIT=1 probes/0074/run.sh # first run: creates missing locks (never on replay)
#   PROBE_EXPERIMENTAL=1 ...        # skips the frozen-input check; every output is labeled experimental
#
# Stages record their status in out/status.json. An infrastructure failure
# (tool build, documentation, extraction, generation, oracle controls, a
# test that did not run) invalidates the run and exits nonzero. A failed
# compilation of the generated module against rc2, or oracle cases that
# fail, are results and are reported. PROBE_CARGO, PROBE_GEN, PROBE_SCRATCH
# and PROBE_SKIP_DOC exist for the controls.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
REV="da47b7405f0e2d188e71cce7c2d588c695f4098d"
GIT_URL="https://github.com/pola-rs/polars"
CARGO="${PROBE_CARGO:-cargo}"
# every override is made absolute: stages cd into the scratch adapter
abs() { case "$1" in /*) echo "$1" ;; *) echo "$PWD/$1" ;; esac; }
release="${PROBE_RELEASE:+$(abs "$PROBE_RELEASE")}"   # record 0075: a release file; required, recorded in status
out="$(abs "${PROBE_OUT:-$here/out}")"; status="$out/status.json"
locks="$(abs "${PROBE_LOCKS:-$here/locks}")"; frozen_file="$(abs "${PROBE_FROZEN:-$here/frozen.json}")"
scratch="$(abs "${PROBE_SCRATCH:-$root/target/0074-adapter}")"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
experimental="${PROBE_EXPERIMENTAL:-0}"

if [ "${1:-}" = report ]; then
  [ -f "$status" ] || { echo "no run to report"; exit 1; }
  python3 "$here/report.py" "$here" "$root" "$out"; exit $?
fi

mkdir -p "$out"
# fresh outputs: nothing from an earlier run can be reported by this one
rm -f "$out"/gen.log "$out"/build.json "$out"/build.stderr "$out"/oracle*.log "$out"/oracle-results-rc2.json "$out"/surface-rc2.json "$out"/report.md "$out"/diag-joins.json "$out"/status.json
rm -rf "$out/rc2/result"
declare -A stage
write_status() {
  python3 - "$status" "$run_id" "$experimental" "$scratch" "$root" "$release" "${!stage[@]}" -- "${stage[@]}" <<'PY'
import json, sys
p, run_id, exp, scratch, root, release = sys.argv[1:7]
rest = sys.argv[7:]; i = rest.index("--"); keys, vals = rest[:i], rest[i+1:]
frozen = {k: root + "/" + v for k, v in {
    "baseline_inventory": "probes/0072/out/0.55.2-adapter/result/inventory.json",
    "baseline_summary": "probes/0072/out/0.55.2-adapter/result/summary.json",
    "baseline_pins": "probes/0072/out/0.55.2-adapter/pins.json",
    "baseline_surface": "adapters/polars/surface.json",
    "baseline_oracle_results": "adapters/polars/oracle-results.json"}.items()}
frozen["doc_lock"] = "probes/0074/locks/doc-rc2.lock"; frozen["adapter_lock"] = "probes/0074/locks/adapter.lock"
# record 0075: the API-crate subset is release-specific; both release files are recorded
frozen["baseline_release"] = root + "/tools/polars-gen/releases/0.55.2-joins.toml"
frozen["release"] = release
reg = 0
try:
    reg = sum(1 for l in open(root + "/probes/0074/locks/adapter.lock") if l.startswith('source = "registry'))
except Exception: pass
json.dump({"run_id": run_id, "experimental": exp == "1", "scratch": scratch, "frozen": frozen, "registry_packages": reg, "stages": dict(zip(keys, vals))}, open(p, "w"), indent=1)
PY
}
fail() { stage[$1]="$2"; write_status; echo "RUN INVALID at $1: $2"; exit 1; }
step() { echo; echo "== $*"; }

step "frozen inputs"
if [ "$experimental" = 1 ]; then
  echo "EXPERIMENTAL: frozen-input check skipped"; stage[frozen]="skipped_experimental"
else
  python3 "$here/freeze.py" verify "$root" "$frozen_file" || fail frozen "inputs differ from frozen.json (set PROBE_EXPERIMENTAL=1 to run anyway, labeled)"
  stage[frozen]=ok
fi
for f in "$locks/doc-rc2.lock" "$locks/adapter.lock"; do
  [ -f "$f" ] || [ "${PROBE_INIT:-0}" = 1 ] || fail locks "missing $(basename "$f"); a replay needs the committed lock (PROBE_INIT=1 creates it once)"
done
write_status

step "tools at their frozen revisions"
( cd "$root/probes/0072/extract" && CARGO_TARGET_DIR="$root/target/0072/extract" "$CARGO" build -q --locked ) || fail tools "extractor build failed"
( cd "$root/tools/polars-gen" && CARGO_TARGET_DIR="$root/target/0073" "$CARGO" build -q --locked ) || fail tools "generator build failed"
extract="$root/target/0072/extract/debug/surface"; gen="${PROBE_GEN:-$root/target/0073/debug/polars-gen}"
stage[tools]=ok; write_status

step "document rc2 ($REV)"
if [ "${PROBE_SKIP_DOC:-0}" = 1 ] && [ -f "$out/rc2/pins.json" ]; then
  echo "skipped (PROBE_SKIP_DOC): using the existing rc2 documentation"; stage[doc]="reused"
else
  "$here/doc.sh" "$REV" rc2 || fail doc "documentation failed"
  python3 -c "import json,sys; p=json.load(open('$out/rc2/pins.json')); sys.exit(1 if p['failed'] or p['rev']!='$REV' else 0)" || fail doc "documentation incomplete or wrong revision"
  stage[doc]=ok
fi
write_status

step "inventory rc2 with the frozen extractor"
"$extract" "$out/rc2" polars "$out/rc2/result" > "$out/rc2/summary.txt" || fail extract "extraction failed"
stage[extract]=ok; write_status

step "generate with the frozen generator into a scratch adapter pinned to rc2"
rm -rf "$scratch"; mkdir -p "$scratch/polars"
for f in Cargo.toml src tests; do cp -r "$root/adapters/polars/$f" "$scratch/polars/"; done
rm -f "$scratch/polars/src/generated/"*.rs "$scratch/polars/tests/generated_oracle.rs"   # nothing of the baseline module survives
cp "$root/adapters/polars/src/generated/support.rs" "$scratch/polars/src/generated/"      # hand-written support is part of the adapter
python3 - "$scratch/polars/Cargo.toml" "$root" "$GIT_URL" "$REV" "$release" <<'PY'
import re, sys
p, root, url, rev, release = sys.argv[1:6]
t = open(p).read().replace('path = "../.."', f'path = "{root}"')
t = re.sub(r'(polars(?:-[a-z]+)?) = \{ version = "=0\.55\.2"', lambda m: f'{m.group(1)} = {{ git = "{url}", rev = "{rev}"', t)
# every API crate the release names is a direct dependency, so generated
# code can spell its items; crates the 0.55.2 manifest lacks are added
import tomllib
api = tomllib.load(open(release, "rb")).get("api_crates", [])
for crate in api:
    dep = crate.replace("_", "-")
    if f"\n{dep} = " not in t:
        t = t.replace("[dependencies]", f'[dependencies]\n{dep} = {{ git = "{url}", rev = "{rev}", default-features = false }}', 1)
open(p, "w").write(t)
PY
n=$(grep -c "rev = \"$REV\"" "$scratch/polars/Cargo.toml"); [ "$n" -ge 11 ] || fail generate "expected at least 11 crates pinned to rc2, found $n"; echo "crates pinned to rc2: $n"
[ -n "$release" ] && [ -f "$release" ] || fail generate "PROBE_RELEASE must name the release file for rc2 (tools/polars-gen/releases/rc2.toml)"
"$gen" "$out/rc2/result/inventory.json" "$scratch/polars" --release "$release" --buckets mechanical,conversion,option_struct > "$out/gen.log" 2>&1 || fail generate "generator failed: $(tail -1 "$out/gen.log")"
[ -f "$scratch/polars/src/generated/functions.rs" ] && [ -f "$scratch/polars/tests/generated_oracle.rs" ] || fail generate "generator produced no module"
cp "$scratch/polars/surface.json" "$out/surface-rc2.json"
tail -5 "$out/gen.log"; stage[generate]=ok; write_status

step "build the scratch adapter against rc2 (--locked)"
lock="$locks/adapter.lock"
mkdir -p "$(dirname "$lock")"
if [ -f "$lock" ]; then cp "$lock" "$scratch/polars/Cargo.lock"; else ( cd "$scratch/polars" && "$CARGO" generate-lockfile -q ) && cp "$scratch/polars/Cargo.lock" "$lock" && echo "INIT: lock saved to $lock"; fi
( cd "$scratch/polars" && CARGO_TARGET_DIR="$root/target/0074-adapter-target" "$CARGO" build -q --locked --release --features test-support --message-format=json > "$out/build.json" 2> "$out/build.stderr" )
build=$?
cmp -s "$lock" "$scratch/polars/Cargo.lock" || fail build "adapter lock changed during the build"
if [ "$build" = 0 ]; then stage[build]=ok; else
  # a compile failure of the generated module is a result; an absent
  # diagnostic (network, toolchain, cargo itself) is infrastructure
  if grep -q '"level":"error"' "$out/build.json"; then stage[build]="failed_with_diagnostics"; else fail build "build failed without compiler diagnostics (see out/build.stderr)"; fi
fi
echo "build: ${stage[build]}"; write_status

if [ "${stage[build]}" = ok ]; then
  step "oracle controls, then the cases"
  tdir="$root/target/0074-adapter-target"
  ( cd "$scratch/polars" && CARGO_TARGET_DIR="$tdir" "$CARGO" test -q --locked --release --features test-support --test generated_oracle oracle_runner_fails_closed > "$out/oracle-controls.log" 2>&1 ) || fail oracle_controls "runner controls failed or did not run (see out/oracle-controls.log)"
  stage[oracle_controls]=ok
  rm -f "$scratch/polars/oracle-results.json"
  ( cd "$scratch/polars" && CARGO_TARGET_DIR="$tdir" "$CARGO" test -q --locked --release --features test-support --test generated_oracle generated_bindings_match_polars > "$out/oracle.log" 2>&1 )
  oracle=$?
  [ -f "$scratch/polars/oracle-results.json" ] || fail oracle "the oracle test emitted no results (see out/oracle.log)"
  grep -q "^test result" "$out/oracle.log" || fail oracle "the oracle test did not run to a result (see out/oracle.log)"
  python3 - "$scratch/polars/oracle-results.json" "$out/oracle-results-rc2.json" "$run_id" "$out/surface-rc2.json" <<'PY' || fail oracle "oracle results are not this run's or do not match the generated cases"
import json, sys
src, dst, run_id, surface = sys.argv[1:5]
r = json.load(open(src)); s = json.load(open(surface))
assert r["cases"] == s["oracle_cases"], (r["cases"], s["oracle_cases"])
r["run_id"] = run_id
json.dump(r, open(dst, "w"), indent=1)
PY
  if [ "$oracle" = 0 ]; then stage[oracle]=ok; else stage[oracle]="cases_failed"; fi
  echo "oracle: ${stage[oracle]}"; write_status
  step "labeled diagnostic: join row multisets"
  cp "$here/diag_joins.rs" "$scratch/polars/tests/diag_joins.rs"
  ( cd "$scratch/polars" && DIAG_OUT="$out/diag-joins.json" DIAG_RUN_ID="$run_id" CARGO_TARGET_DIR="$tdir" "$CARGO" test -q --locked --release --features test-support --test diag_joins > "$out/diag-joins.log" 2>&1 ) && stage[diag_joins]=ok || stage[diag_joins]="failed (see out/diag-joins.log)"
  write_status
else
  stage[oracle]="not_run_build_failed"; write_status
fi

step "report"
python3 "$here/report.py" "$here" "$root" "$out" | tee "$out/report.md"
