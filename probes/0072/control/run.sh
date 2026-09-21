#!/usr/bin/env bash
# Extraction control: document the control facade with and without the
# `extra` feature, run the extractor, and compare with expected.json.
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../../.." && pwd)"
export RUSTDOCFLAGS="-Z unstable-options --output-format json --document-hidden-items"
surface="$root/target/0072/extract/debug/surface"
for cfg in default extra; do
  tgt="$root/target/0072/control-$cfg"; export CARGO_TARGET_DIR="$tgt"
  feat=""; [ "$cfg" = extra ] && feat="--features extra"
  ( cd "$here/facade" && cargo +nightly-2026-09-20 rustdoc -q --locked --lib $feat; for c in ctrl_dep ctrl_dep2; do cargo +nightly-2026-09-20 rustdoc -q --locked --lib -p "$c"; done )
  mkdir -p "$here/out/$cfg"; cp "$tgt"/doc/*.json "$here/out/$cfg/"
  "$surface" "$here/out/$cfg" ctrl "$here/out/$cfg/result" > /dev/null
done
python3 "$here/check.py" "$here"
