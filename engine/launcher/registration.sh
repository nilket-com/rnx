#!/bin/sh
# Build-time tool linked to the selected archive, with no fixture dependency.
set -eu
archive=$1
out=$2
tmp=$(mktemp -d "$(dirname -- "$out")/.registration.XXXXXX")
trap 'rm -rf -- "$tmp"' EXIT
printf '%s\n' 'extern int rnx_write_file_registration(void);' 'int main(void) { return rnx_write_file_registration(); }' >"$tmp/registration.c"
cc -O2 -Wl,--gc-sections "$tmp/registration.c" "$archive" -lstdc++ -ldl -lm -lpthread -o "$tmp/registration"
env -u POLARS_PANIC_ON_ERR "$tmp/registration" >"$out"
