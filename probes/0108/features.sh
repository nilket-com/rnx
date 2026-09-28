#!/usr/bin/env bash
# Record 0108: the `polars` crate features the Python 2.0 wheel enables.
# Resolved by cargo from the pinned workspace itself: the wheel's runtime
# crate (polars-runtime-32, the default `pip install polars` runtime; default features = "full" + "nightly"; runtime-64 adds only bigidx) and
# everything polars-python turns on for it. Written to features.txt.
#
#   features.sh [checkout]      # default: target/0108/polars
set -euo pipefail
here="$(cd "$(dirname "$0")" && pwd)"; root="$(cd "$here/../.." && pwd)"
REV="da47b7405f0e2d188e71cce7c2d588c695f4098d"
src="${1:-$root/target/0108/polars}"
if [ ! -d "$src/.git" ]; then git clone -q --filter=blob:none --no-checkout https://github.com/pola-rs/polars "$src"; fi
git -C "$src" checkout -q "$REV"
[ "$(git -C "$src" rev-parse HEAD)" = "$REV" ] || { echo "checkout is not $REV" >&2; exit 1; }
[ -z "$(git -C "$src" status --porcelain)" ] || { echo "checkout is dirty" >&2; exit 1; }
( cd "$src" && cargo tree -q --locked -p polars-runtime-32 -e features -i polars --prefix none ) \
  | sed -n 's/^polars feature "\([^"]*\)".*/\1/p' | sort -u > "$here/features.txt"
n=$(wc -l < "$here/features.txt"); [ "$n" -ge 100 ] || { echo "only $n features resolved" >&2; exit 1; }
echo "$n polars features for polars-runtime-32 at $REV -> features.txt"
