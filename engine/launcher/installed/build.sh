#!/bin/sh
# Runs only for the installed stock CLI. The image's dependency on rnx has
# default features disabled and cannot reach this build again.
set -eu
out=$1
shift
work=$(mktemp -d "$out/.stock-file-XXXXXX")
trap 'rm -rf -- "$work"' EXIT
if [ -n "${RNX_EXPORT_SOURCE-}" ]; then export RNX_EXPORT_NESTED="$work"; fi
sh engine/launcher/installed/image.sh "$work" "$1"
image=$work/target/$TARGET/release/examples/libengine_image.so
helper=$work/stock-file
sh engine/launcher/installed/link.sh "$image" "$helper"
for file in "$image" "$helper"; do
	name=$(basename "$file")
	sha256sum <"$file" | cut -d' ' -f1 | tr -d '\n' >"$work/$name.sha256"
	mv "$work/$name.sha256" "$out/$name.sha256"
	mv "$file" "$out/$name"
done
