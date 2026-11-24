#!/bin/sh
set -eu
set -- "$@" "--remap-path-prefix=$RNX_EXPORT_TMP=/rnx-build/tmp" \
	"--remap-path-prefix=$RNX_EXPORT_SOURCE=/rnx-build/source" \
	"--remap-path-prefix=$RNX_EXPORT_TARGET=/rnx-build/target"
if [ -n "${RNX_EXPORT_NESTED-}" ]; then
	set -- "$@" "--remap-path-prefix=$RNX_EXPORT_NESTED=/rnx-build/stock-file"
fi
if [ -n "$RNX_EXPORT_COMPILER_WRAPPER" ]; then
	exec "$RNX_EXPORT_COMPILER_WRAPPER" "$@"
fi
exec "$@"
