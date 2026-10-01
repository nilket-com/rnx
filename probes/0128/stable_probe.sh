#!/usr/bin/env bash
# Record 0128: a bounded compile probe for the Polars shipping decision.
# Does a useful Polars v2 configuration build on STABLE Rust (the v2 scratch
# uses the Python wheel's feature set, whose `nightly`/`simd` need nightly)?
#   A  Polars v2 (git da47b74) alone, the shipped adapter's features + pivot
#   B  the hand-written adapter (no generated bindings) against A's v2
#   C  Polars 0.55.2 (the shipped pin) with `pivot` added: the adapter build
# Each build has a 45-minute limit; results go to out/results.json.
set -uo pipefail
root=$(cd "$(dirname "$0")/../.." && pwd); here="$root/probes/0128"; out="$here/out"
mkdir -p "$out" "$here/locks"
TOOL="+stable"; LIMIT=2700
REV="da47b7405f0e2d188e71cce7c2d588c695f4098d"; URL="https://github.com/pola-rs/polars"
FEATS='"lazy", "csv", "parquet", "json", "pivot", "dtype-i8", "dtype-i16", "dtype-u8", "dtype-u16"'
results="$out/results.jsonl"; CASES="${PROBE_CASES:-A B C}"; [ "$CASES" = "A B C" ] && : > "$results"
record() { # name, exit, seconds, log
	python3 - "$1" "$2" "$3" "$4" "$results" <<'PY'
import json, sys, re
name, code, secs, log, res = sys.argv[1:6]
text = open(log, errors="replace").read()
errs = [l for l in text.splitlines() if l.startswith("error")][:8]
json.dump({"case": name, "exit": int(code), "seconds": int(secs), "timed_out": int(code) == 124,
           "errors": errs}, open(res, "a")); open(res, "a").write("\n")
PY
}
run() { # name, dir, cargo args...
	local name=$1 dir=$2; shift 2
	local t0=$(date +%s)
	( cd "$dir" && timeout $LIMIT cargo $TOOL "$@" ) > "$out/$name.log" 2>&1
	local code=$?
	record "$name" $code $(( $(date +%s) - t0 )) "$out/$name.log"
	echo "$name: exit $code in $(( $(date +%s) - t0 ))s"
}
rustc $TOOL --version > "$out/toolchain.txt"

# A: Polars v2 alone on stable
if [[ " $CASES " == *" A "* ]]; then
a="$root/target/0128/hostA"; rm -rf "$a"; mkdir -p "$a/src"; echo > "$a/src/lib.rs"
cat > "$a/Cargo.toml" <<TOML
[package]
name = "host"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
polars = { git = "$URL", rev = "$REV", default-features = false, features = [$FEATS] }
TOML
[ -f "$here/locks/A.lock" ] && cp "$here/locks/A.lock" "$a/Cargo.lock"
CARGO_TARGET_DIR="$root/target/0128/targetA" run A "$a" build --release
cp "$a/Cargo.lock" "$here/locks/A.lock" 2>/dev/null
( cd "$a" && cargo $TOOL metadata --format-version 1 2>/dev/null ) | python3 -c '
import json,sys; m=json.load(sys.stdin); p=[x for x in m["packages"] if x["name"]=="polars"][0]
r=[n for n in m["resolve"]["nodes"] if n["id"]==p["id"]][0]; print(json.dumps(sorted(r["features"])))' > "$out/A-features.json"
fi

# B: the hand-written adapter against the same v2, generated bindings off
if [[ " $CASES " == *" B "* ]]; then
b="$root/target/0128/adapterB/polars"; rm -rf "$(dirname "$b")"; mkdir -p "$b"
cp -r "$root/adapters/polars/"{Cargo.toml,src,tests} "$b/"
python3 - "$b/Cargo.toml" "$root" "$URL" "$REV" "$FEATS" <<'PY'
import re, sys
p, root, url, rev, feats = sys.argv[1:6]
t = open(p).read().replace('path = "../.."', f'path = "{root}"')
t, n = re.subn(r'polars = \{ version = "=0\.55\.2", default-features = false, features = \[[^\]]*\] \}',
               f'polars = {{ git = "{url}", rev = "{rev}", default-features = false, features = [{feats}] }}', t)
assert n == 1
t = re.sub(r'(polars-[a-z]+) = \{ version = "=0\.55\.2"', lambda m: f'{m.group(1)} = {{ git = "{url}", rev = "{rev}"', t)
open(p, "w").write(t)
PY
cp "$here/locks/A.lock" "$b/Cargo.lock" 2>/dev/null
CARGO_TARGET_DIR="$root/target/0128/targetA" run B "$b" build --release --no-default-features --lib
fi

# C: 0.55.2 with pivot added, the full shipped adapter (generated on)
if [[ " $CASES " == *" C "* ]]; then
c="$root/target/0128/adapterC/polars"; rm -rf "$(dirname "$c")"; mkdir -p "$c"
cp -r "$root/adapters/polars/"{Cargo.toml,Cargo.lock,src,tests} "$c/"
sed -i 's|^rnx = { path = "../.."|rnx = { path = "'"$root"'"|; s|features = \["lazy", "csv", "parquet", "json",|features = ["lazy", "csv", "parquet", "json", "pivot",|' "$c/Cargo.toml"
CARGO_TARGET_DIR="$root/target/0128/targetC" run C "$c" build --release
( cd "$c" && cargo $TOOL metadata --format-version 1 2>/dev/null ) | python3 -c '
import json,sys; m=json.load(sys.stdin); p=[x for x in m["packages"] if x["name"]=="polars"][0]
r=[n for n in m["resolve"]["nodes"] if n["id"]==p["id"]][0]; print(json.dumps(sorted(r["features"])))' > "$out/C-features.json"
python3 - "$root/adapters/polars/Cargo.lock" "$c/Cargo.lock" > "$out/C-lock-diff.txt" <<'PY'
import re, sys
pk = lambda f: set(re.findall(r'name = "([^"]+)"\nversion = "([^"]+)"', open(f).read()))
a, b = pk(sys.argv[1]), pk(sys.argv[2])
print("removed", sorted(a - b)); print("added", sorted(b - a))
PY
fi
echo "done: $results"
