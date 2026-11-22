#!/bin/sh
# Link a frozen provider archive and engine image. Neither is rebuilt here.
set -eu
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
image=$(realpath -- "$1")
archive=$(realpath -- "$2")
out=$(realpath -m -- "$3")
oracle=$(realpath -- "${4:?stock_file_inventory is required}")
mkdir -p "$out"
env -u POLARS_PANIC_ON_ERR "$oracle" --refusal-header >"$out/refusal_table.h"
cp -- "$image" "$out/libengine_image.so"
hash=$(sha256sum <"$image" | cut -d' ' -f1)
set -- cc -std=c11 -O2 -Wall -Wextra -Werror -fPIE -pie -Wl,--gc-sections,--exclude-libs,ALL \
	"-DENGINE_DIGEST=\"$hash\"" "$here/adapter_launcher.c" "$here/../../demos/candle/file-provider/registry.c" "-I$out" "$archive" "-L$out" '-Wl,-rpath,$ORIGIN' \
	-lengine_image -lcrypto -lstdc++ -ldl -lm -lpthread -o "$out/rnx"
printf '%s\n' "$@" > "$out/cc.argv"
"$@"
sha256sum "$oracle" "$out/refusal_table.h" "$archive" "$out/rnx" "$out/libengine_image.so" >"$out/artifacts.sha256"
