#!/bin/sh
# Only inputs to cc are the fixture, header, expected digest and finished DSO.
set -eu
src=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
image=$(realpath -- "$1")
out=$(realpath -m -- "$2")
mkdir -p "$out"
hash=$(sha256sum < "$image" | cut -d' ' -f1)
for role in plain tagged; do
	mkdir "$out/$role"
	cp -- "$image" "$out/$role/libengine_image.so"
	set -- cc -std=c11 -O2 -Wall -Wextra -Werror -fPIE -pie
	if [ "$role" = tagged ]; then set -- "$@" -DTAGGED; fi
	set -- "$@" "-DENGINE_DIGEST=\"$hash\"" "$src/launcher.c" "-L$out/$role" '-Wl,-rpath,$ORIGIN' -lengine_image -lcrypto -ldl -o "$out/$role/rnx"
	printf '%s\n' "$@" > "$out/$role/cc.argv"
	"$@"
	sha256sum "$out/$role/rnx" "$out/$role/libengine_image.so" > "$out/$role/artifacts.sha256"
done
sha256sum "$src/engine_image.h" "$src/launcher.c" "$src/build-launchers.sh" > "$out/inputs.sha256"
