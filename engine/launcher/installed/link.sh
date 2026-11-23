#!/bin/sh
# The loader maps the already-opened image through fd 9 before C main runs.
# A project pair links its own provider archive against those same image bytes.
set -eu
image=$1
helper=$2
shift 2
case $# in
	0) set -- ;;
	1)
		archive=$1
		private=$(mktemp -d "$(dirname -- "$helper")/.provider-XXXXXX")
		trap 'rm -rf -- "$private"' EXIT
		sh engine/launcher/registration.sh "$archive" "$private/refusal_table.h"
		set -- -DRNX_FILE_ADAPTERS -Wl,--gc-sections,--exclude-libs,ALL \
			adapters/file-provider/registry.c "-I$private" "$archive" -lstdc++ -lm -lpthread
		;;
	*) exit 2 ;;
esac
cc -std=c11 -O2 -Wall -Wextra -Werror -fPIE -pie \
	engine/launcher/installed/main.c "$@" /proc/self/fd/9 -ldl -o "$helper" 9<"$image"
readelf -dW "$helper" | grep -Fq 'Shared library: [/proc/self/fd/9]'
