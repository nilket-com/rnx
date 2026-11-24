#!/bin/sh
# Cargo fingerprints include RUSTFLAGS. Supply physical prefix mappings to
# the compiler after Cargo has chosen its target names and crate metadata.
set -eu
export RNX_EXPORT_SOURCE="$(pwd -P)" RNX_EXPORT_TARGET="${CARGO_TARGET_DIR:?}"
export RNX_EXPORT_TMP="${TMPDIR:-/tmp}" RNX_EXPORT_COMPILER_WRAPPER="${RUSTC_WRAPPER-}"
export RUSTC_WRAPPER="$RNX_EXPORT_SOURCE/engine/launcher/installed/export-rustc.sh"
exec "$@"
