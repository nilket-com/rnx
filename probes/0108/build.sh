#!/usr/bin/env bash
# Record 0108: the current generator against the Polars v2 inventory, in a
# scratch copy of the adapter pinned to the v2 commit with the Python
# wheel's feature set. The production adapter is never touched.
#
#   probes/0108/build.sh               # generate, build, run the oracle
#   PROBE_INIT=1 probes/0108/build.sh  # first run only: creates locks/adapter.lock
#   PROBE_STAGE=no_default probes/0108/build.sh  # only the no-default-features build and tests, on the existing scratch
#
# Stages record their status in out/build-status.json. Infrastructure
# failures (inputs, tools, generation, a test that did not run) exit
# nonzero. A compile failure of the generated module, or failing oracle
# cases, are results.
set -uo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
REV="da47b7405f0e2d188e71cce7c2d588c695f4098d"
GIT_URL="https://github.com/pola-rs/polars"
NIGHTLY="nightly-2026-09-20"     # the documentation toolchain; the wheel's `nightly`/`simd` features need one
CARGO="${PROBE_CARGO:-cargo +$NIGHTLY}"   # PROBE_CARGO: for controls.sh only
out="${PROBE_OUT:-$here/out}"; status="$out/build-status.json"   # PROBE_OUT: for controls.sh only
inv="$out/v2py/result/inventory.json"; pins="$out/v2py/pins.json"
release="$here/v2.toml"; lock="$here/locks/adapter.lock"
scratch="$root/target/0108/adapter"; tdir="$root/target/0108/adapter-target"
mkdir -p "$out"
[ "${PROBE_STAGE:-}" = no_default ] || rm -f "$out"/gen.log "$out"/build.json "$out"/build.stderr "$out"/oracle*.log "$out"/oracle-results-v2.json "$out"/surface-v2.json "$out"/no-default.log "$status"
declare -A stage
write_status() {
  python3 - "$status" "${!stage[@]}" -- "${stage[@]}" <<'PY'
import json, sys
p = sys.argv[1]; rest = sys.argv[2:]; i = rest.index("--")
json.dump({"stages": dict(zip(rest[:i], rest[i+1:]))}, open(p, "w"), indent=1)
PY
}
fail() { stage[$1]="$2"; write_status; echo "RUN INVALID at $1: $2"; exit 1; }
step() { echo; echo "== $*"; }

no_default() {
  # record 0108 review: the second adapter configuration, the hand-written
  # adapter alone (`generated` off), built and tested against v2
  step "no-default-features build and tests (--locked, $NIGHTLY)"
  [ -f "$scratch/polars/Cargo.toml" ] && [ -f "$lock" ] || fail no_default "no scratch adapter or lock: run build.sh first"
  cp "$lock" "$scratch/polars/Cargo.lock"
  ( cd "$scratch/polars" && CARGO_TARGET_DIR="$tdir-nodefault" $CARGO test -q --locked --release --no-default-features > "$out/no-default.log" 2>&1 )
  local r=$?
  cmp -s "$lock" "$scratch/polars/Cargo.lock" || fail no_default "adapter lock changed during the build"
  grep -q "^test result" "$out/no-default.log" || fail no_default "the no-default tests did not run (see out/no-default.log)"
  [ "$r" = 0 ] || fail no_default "tests failed (see out/no-default.log)"   # a required check: the run is invalid
  stage[no_default]=ok; echo "no_default: ok"; write_status
}
if [ "${PROBE_STAGE:-}" = no_default ]; then
  [ -f "$status" ] && rm -f "$status"
  no_default; echo "done: $status"; exit 0
fi

step "inputs"
python3 - "$pins" "$here/features.txt" "$inv" "$REV" <<'PY' || fail inputs "documentation, features or inventory disagree"
import json, sys
pins, feats, inv, rev = sys.argv[1:5]
p = json.load(open(pins)); f = open(feats).read().split()
assert p["rev"] == rev and p["cfg"] == "v2py" and not p["failed"], "pins"
assert sorted(p["resolved_polars_features"]) == sorted(f), "pins resolved features != features.txt"
i = json.load(open(inv))["provenance"]
assert i.get("rev") == rev and i.get("cfg") == "v2py", i
assert sorted(i["features"]) == sorted(f), "inventory features != features.txt"
PY
[ -f "$lock" ] || [ "${PROBE_INIT:-0}" = 1 ] || fail inputs "missing locks/adapter.lock (PROBE_INIT=1 creates it once)"
stage[inputs]=ok; write_status

step "current generator"
( cd "$root/tools/polars-gen" && cargo build -q --release --locked ) || fail tools "generator build failed"
gen="$root/tools/polars-gen/target/release/polars-gen"
stage[tools]=ok; write_status

step "scratch adapter pinned to v2"
rm -rf "$scratch"; mkdir -p "$scratch/polars"
for f in Cargo.toml src tests; do cp -r "$root/adapters/polars/$f" "$scratch/polars/"; done
rm -f "$scratch/polars/src/generated/"*.rs "$scratch/polars/tests/generated_oracle.rs"
cp "$root/adapters/polars/src/generated/support.rs" "$scratch/polars/src/generated/"
python3 - "$scratch/polars/Cargo.toml" "$root" "$GIT_URL" "$REV" "$release" "$here/features.txt" <<'PY' || fail generate "manifest rewrite failed"
import re, sys, tomllib
p, root, url, rev, release, feats = sys.argv[1:7]
t = open(p).read().replace('path = "../.."', f'path = "{root}"')
f = ", ".join(f'"{x}"' for x in open(feats).read().split())
t, n = re.subn(r'polars = \{ version = "=0\.55\.2", default-features = false, features = \[[^\]]*\] \}',
               f'polars = {{ git = "{url}", rev = "{rev}", default-features = false, features = [{f}] }}', t)
assert n == 1, "polars facade dependency not found"
t = re.sub(r'(polars-[a-z]+) = \{ version = "=0\.55\.2"', lambda m: f'{m.group(1)} = {{ git = "{url}", rev = "{rev}"', t)
assert "0.55.2" not in t.split("[dependencies]")[1], "a 0.55.2 dependency survived"
for crate in tomllib.load(open(release, "rb"))["api_crates"]:
    dep = crate.replace("_", "-")
    if f"\n{dep} = " not in t:
        t = t.replace("[dependencies]", f'[dependencies]\n{dep} = {{ git = "{url}", rev = "{rev}", default-features = false }}', 1)
# record 0116 (rule 6): generated time-zone conversions name chrono_tz::Tz,
# which Polars does not re-export; pinned to the version its lock resolves
if "\nchrono-tz = " not in t:
    t = t.replace("[dependencies]", '[dependencies]\nchrono-tz = "=0.10.4"', 1)
open(p, "w").write(t)
PY
"$gen" "$inv" "$scratch/polars" --release "$release" --buckets mechanical,conversion,option_struct,callback,generic_fn,generic > "$out/gen.log" 2>&1 || fail generate "generator failed: $(tail -1 "$out/gen.log")"
[ -f "$scratch/polars/src/generated/functions.rs" ] && [ -f "$scratch/polars/tests/generated_oracle.rs" ] || fail generate "generator produced no module"
cp "$scratch/polars/surface.json" "$out/surface-v2.json"
tail -3 "$out/gen.log"; stage[generate]=ok; write_status

step "build (--locked, $NIGHTLY)"
mkdir -p "$(dirname "$lock")"
if [ -f "$lock" ]; then cp "$lock" "$scratch/polars/Cargo.lock"
else ( cd "$scratch/polars" && $CARGO generate-lockfile -q ) && cp "$scratch/polars/Cargo.lock" "$lock" && echo "INIT: lock saved"; fi
( cd "$scratch/polars" && CARGO_TARGET_DIR="$tdir" $CARGO build -q --locked --release --features test-support --message-format=json > "$out/build.json" 2> "$out/build.stderr" )
build=$?
cmp -s "$lock" "$scratch/polars/Cargo.lock" || fail build "adapter lock changed during the build"
if [ "$build" = 0 ]; then stage[build]=ok
elif grep -q '"level":"error"' "$out/build.json"; then stage[build]=failed_with_diagnostics
else fail build "build failed without compiler diagnostics (see out/build.stderr)"; fi
echo "build: ${stage[build]}"; write_status

if [ "${stage[build]}" = ok ]; then
  step "oracle controls, then the cases"
  ( cd "$scratch/polars" && CARGO_TARGET_DIR="$tdir" $CARGO test -q --locked --release --features test-support --test generated_oracle oracle_runner_fails_closed > "$out/oracle-controls.log" 2>&1 ) || fail oracle_controls "runner controls failed (see out/oracle-controls.log)"
  stage[oracle_controls]=ok
  rm -f "$scratch/polars/oracle-results.json"
  ( cd "$scratch/polars" && CARGO_TARGET_DIR="$tdir" $CARGO test -q --locked --release --features test-support --test generated_oracle generated_bindings_match_polars > "$out/oracle.log" 2>&1 )
  oracle=$?
  [ -f "$scratch/polars/oracle-results.json" ] && grep -q "^test result" "$out/oracle.log" || fail oracle "the oracle did not run to a result (see out/oracle.log)"
  python3 - "$scratch/polars/oracle-results.json" "$out/oracle-results-v2.json" "$out/surface-v2.json" <<'PY' || fail oracle "oracle results do not match the generated cases"
import json, sys
src, dst, surface = sys.argv[1:4]
r = json.load(open(src)); s = json.load(open(surface))
assert r["cases"] == s["oracle_cases"], (r["cases"], s["oracle_cases"])
json.dump(r, open(dst, "w"), indent=1)
PY
  if [ "$oracle" = 0 ]; then stage[oracle]=ok; else stage[oracle]=cases_failed; fi
  echo "oracle: ${stage[oracle]}"; write_status
else
  stage[oracle]=not_run_build_failed; write_status
fi
no_default
echo "done: $status"
