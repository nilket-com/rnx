#!/usr/bin/env bash
# Produce rustdoc JSON for the polars facade and every polars-* crate it
# activates, for one (release, configuration) pair.
#
#   doc.sh <release> <adapter|full>
#
# Output: probes/0072/out/<release>-<cfg>/<crate>.json plus pins.json. The
# output directory is cleared first so nothing stale can be reused. The
# resolved Cargo.lock is kept in probes/0072/inventory/locks/ and reused on
# replay so the dependency graph is reproducible.
set -euo pipefail
release="$1"; cfg="$2"
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../../.." && pwd)"
NIGHTLY="nightly-2026-09-20"          # pinned by date; the evidence quotes it
host="$root/target/0072/host-$release-$cfg"
out="$root/probes/0072/out/$release-$cfg"
lock="$here/locks/$release-$cfg.lock"
rm -rf "$out" "$host"; mkdir -p "$host/src" "$out" "$here/locks"
# Full means every feature the crate declares, minus the ones named here
# with a reason; the omitted list is written to pins.json.
FULL_OMIT=""
case "$cfg" in
  adapter) features='default-features = false, features = ["lazy", "csv", "parquet"]' ;;
  # Record 0081: the adapter's feature set with the four narrow integer dtypes.
  adapter-narrow) features='default-features = false, features = ["lazy", "csv", "parquet", "dtype-i8", "dtype-i16", "dtype-u8", "dtype-u16"]' ;;
  full)
    cat > "$host/Cargo.toml" <<TOML
[package]
name = "host"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
polars = { version = "=$release", default-features = false }
TOML
    echo '' > "$host/src/lib.rs"
    all=$(cd "$host" && cargo +$NIGHTLY metadata --format-version 1 -q 2>/dev/null | python3 -c '
import json,sys
m=json.load(sys.stdin); p=[p for p in m["packages"] if p["name"]=="polars"][0]
omit=set(sys.argv[1].split())
print(",".join("\"%s\"" % f for f in sorted(p["features"]) if f not in omit))' "$FULL_OMIT")
    features="default-features = false, features = [$all]" ;;
  *) echo "unknown cfg $cfg" >&2; exit 2 ;;
esac
cat > "$host/Cargo.toml" <<TOML
[package]
name = "host"
version = "0.0.0"
edition = "2024"
publish = false
[workspace]
[dependencies]
polars = { version = "=$release", $features }
TOML
echo '' > "$host/src/lib.rs"
export CARGO_TARGET_DIR="$root/target/0072/target-$release-$cfg"
export RUSTDOCFLAGS="-Z unstable-options --output-format json --document-hidden-items"
cd "$host"
# A saved lock is used as is and must survive the run byte for byte; a
# lock is created only for a configuration that has none yet.
if [ -f "$lock" ]; then cp "$lock" Cargo.lock; else cargo +$NIGHTLY generate-lockfile -q; cp Cargo.lock "$lock"; echo "new configuration: lock saved to $lock"; fi
LOCKED="--locked"
# Crates activated by this feature set. `cargo tree` respects features.
crates=$(cargo +$NIGHTLY tree $LOCKED -e normal --prefix none -q | awk '{print $1}' | sort -u | grep -E '^polars(-|$)' | tr '\n' ' ')
echo "documenting: $crates"
failed=""
for c in $crates; do
  rm -f "$CARGO_TARGET_DIR/doc/${c//-/_}.json"
  if cargo +$NIGHTLY rustdoc -q $LOCKED -p "$c" --lib 2>"$out/$c.log" && [ -f "$CARGO_TARGET_DIR/doc/${c//-/_}.json" ]; then
    cp "$CARGO_TARGET_DIR/doc/${c//-/_}.json" "$out/$c.json"
  else
    echo "FAILED $c (see $out/$c.log)"; failed="$failed $c"
  fi
done
python3 - "$out" "$release" "$cfg" "$crates" "$failed" "$NIGHTLY" "$FULL_OMIT" <<'PY'
import json,subprocess,sys,glob,os
out,release,cfg,crates,failed,nightly,omit=sys.argv[1:8]
def sh(c): return subprocess.check_output(c,shell=True,text=True).strip()
fv=None
for j in glob.glob(out+"/*.json"):
    if os.path.basename(j)=="pins.json": continue
    fv=json.load(open(j))["format_version"]; break
meta=json.loads(sh(f"cargo +{nightly} metadata --locked --format-version 1"))
pol=[p for p in meta["packages"] if p["name"]=="polars"][0]
res=[n for n in meta["resolve"]["nodes"] if n["id"]==pol["id"]][0]
declared=sorted(pol["features"])
json.dump({"release":release,"cfg":cfg,"toolchain":nightly,"rustc":sh(f"rustc +{nightly} --version"),
  "rustdoc":sh(f"rustdoc +{nightly} --version"),"format_version":fv,
  "target":sh(f"rustc +{nightly} -vV | sed -n 's/^host: //p'"),
  "declared_polars_features":declared,
  "resolved_polars_features":sorted(res["features"]),
  "declared_not_resolved":sorted(set(declared)-set(res["features"])),
  "omitted_by_choice":omit.split(),
  "crates":crates.split(),"failed":failed.split(),
  "lock":f"probes/0072/inventory/locks/{release}-{cfg}.lock"},
  open(out+"/pins.json","w"),indent=1)
PY
cmp -s Cargo.lock "$lock" || { echo "FAIL: Cargo.lock changed during the replay of $release-$cfg"; exit 1; }
echo "done: $out"
