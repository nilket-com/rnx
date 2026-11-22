#!/bin/sh
# Link the chosen provider and bind the launcher's entry to the adjacent DSO.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
out=$1
shift
hash=$(sha256sum <"$out/libengine_image.so" | cut -d' ' -f1)
case $# in
	0) set -- ;;
	2) archive=$1; private=$2
		sh "$src/registration.sh" "$archive" "$private/refusal_table.h"
		set -- -DRNX_FILE_ADAPTERS "$src/../../adapters/file-provider/registry.c" "-I$private" "$archive" ;;
	*) exit 2 ;;
esac
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIE -pie -Wl,--gc-sections,--exclude-libs,ALL \
	"-DENGINE_DIGEST=\"$hash\"" "$src/main.c" "$@" "-L$out" '-Wl,-rpath,$ORIGIN' \
	-lengine_image -lcrypto -lstdc++ -ldl -lm -lpthread -o "$out/rnx"
