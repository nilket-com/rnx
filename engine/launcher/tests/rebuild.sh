#!/bin/sh
# Run against a verified engine bundle. All writes stay under a fresh directory.
set -eu
tool=$(realpath "$1")
root=$(realpath "$2")
bundle=$(realpath "$3")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
mkdir "$tmp/bin"
cc=$(command -v cc)
cat >"$tmp/bin/cc" <<'SH'
#!/bin/sh
if [ "$1" = --version ]; then
	echo 'rebuild control'
	exec "$REAL_CC" --version
fi
case "$LINK_CONTROL" in
	fail) exit 88 ;;
	interrupt) echo ready >"$READY"; exec sleep 120 ;;
	*) exit 1 ;;
esac
SH
chmod +x "$tmp/bin/cc"
out="$tmp/stock"
build() { "$tool" image stock "$root" "$bundle" "$out" --offline; }
state() {
	stat -c '%i %s' "$out" "$out/rnx" "$out/libengine_image.so" "$out/image-package.json"
	sha256sum "$out/rnx" "$out/libengine_image.so" "$out/image-package.json"
}
build
state >"$tmp/before"
build
state >"$tmp/after"
cmp "$tmp/before" "$tmp/after"
export REAL_CC="$cc" PATH="$tmp/bin:$PATH" LINK_CONTROL=fail
if build >"$tmp/stdout" 2>"$tmp/err"; then exit 1; fi
grep -q 'exit status: 88' "$tmp/err"
state >"$tmp/after"
cmp "$tmp/before" "$tmp/after"
export LINK_CONTROL=interrupt READY="$tmp/ready"
mkfifo "$READY"
"$tool" image stock "$root" "$bundle" "$out" --offline >"$tmp/stdout" 2>"$tmp/err" &
pid=$!
if ! timeout 30 grep -Fxq ready "$READY"; then kill -TERM "$pid" || :; wait "$pid" || :; exit 1; fi
kill -TERM "$pid"
status=0
wait "$pid" || status=$?
test "$status" = 143
state >"$tmp/after"
cmp "$tmp/before" "$tmp/after"
printf '%s\n' 'pub fn main(_) { 42 }' >"$tmp/check.rn"
test "$("$out/rnx" run "$tmp/check.rn")" = 42
test -z "$(find "$tmp" -maxdepth 1 -name '.rnx-image-build-*' -print)"
# A damaged copy must never acquire a new launcher or replace a good one.
cp -R "$bundle" "$tmp/stale"
printf x >>"$tmp/stale/libengine_image.so"
if "$tool" image stock "$root" "$tmp/stale" "$out" >"$tmp/stdout" 2>"$tmp/err"; then exit 1; fi
grep -Fq 'stale receipt or altered package' "$tmp/err"
state >"$tmp/after"
cmp "$tmp/before" "$tmp/after"
