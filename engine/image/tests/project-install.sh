#!/bin/sh
# Exercise the public artifact after moving it, without its checkout or build tools.
set -eu
app=$(realpath -- "$1")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
mkdir "$tmp/cache" "$tmp/out"
cp "$app" "$tmp/application"
cat >"$tmp/entry.rn" <<'RN'
mod other;
pub fn main(args) { [other::value(), args, env::args()] }
RN
printf 'pub fn value() { 42 }\n' >"$tmp/other.rn"
for mode in cold warm memfd; do
	if [ "$mode" = memfd ]; then
		# nano's bwrap profile denies exec from a memfd. Exercise that
		# kernel path in the invoking profile, still with an empty PATH.
		(cd "$tmp" && exec /usr/bin/env -i HOME=/tmp PATH=/nonexistent \
			XDG_CACHE_HOME=/proc/rnx-unavailable RNX_FILE_ROUTE_FD=9 \
			./application run entry.rn "two words" "" 9>out/event) \
			>"$tmp/out/stdout" 2>"$tmp/out/stderr" || { cat "$tmp/out/stderr"; exit 1; }
	else
		bwrap --die-with-parent --unshare-all --ro-bind /usr /usr --symlink usr/lib /lib \
			--symlink usr/lib64 /lib64 --dev /dev --proc /proc --tmpfs /tmp \
			--bind "$tmp" /case --chdir /case --clearenv --setenv HOME /tmp \
			--setenv PATH /nonexistent --setenv XDG_CACHE_HOME /case/cache \
			/usr/bin/sh -c 'exec 9>/case/out/event; export RNX_FILE_ROUTE_FD=9; exec ./application run entry.rn "two words" ""' \
			>"$tmp/out/stdout" 2>"$tmp/out/stderr" || { cat "$tmp/out/stderr"; exit 1; }
	fi
	printf '[42, ["two words", ""], ["two words", ""]]\n' | cmp - "$tmp/out/stdout"
	test ! -s "$tmp/out/stderr"
	jq -es 'length == 1 and .[0].route == "execution"' "$tmp/out/event" >/dev/null
	find "$tmp/cache" -type f \( -name engine -o -name helper \) -printf '%P %i\n' | LC_ALL=C sort >"$tmp/out/$mode"
done
test "$(wc -l <"$tmp/out/cold")" -eq 2
cmp "$tmp/out/cold" "$tmp/out/warm"
cmp "$tmp/out/cold" "$tmp/out/memfd"
echo 'PASS relocated app, offline module and argv, cold, warm, and sealed memory'
