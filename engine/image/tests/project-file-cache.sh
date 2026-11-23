#!/bin/sh
# Supply the stock CLI, empty generated app and paired generated app in that order.
set -eu
stock=$(realpath -- "$1")
empty=$(realpath -- "$2")
pair=$(realpath -- "$3")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
export XDG_CACHE_HOME="$tmp/cache"
printf '%s\n' 'pub fn main(args){42}' >"$tmp/main.rn"
run() {
	"$1" run "$tmp/main.rn" >"$tmp/out"
	printf '42\n' | cmp - "$tmp/out"
}
run "$stock"
run "$empty"
run "$pair"
root=$XDG_CACHE_HOME/rnx-file-images
members() {
	find "$root" -type f \( -name helper -o -name engine \) -printf '%i %P\n' | LC_ALL=C sort
}
members >"$tmp/cold"
test "$(wc -l <"$tmp/cold")" -eq 6
run "$empty"
run "$stock"
run "$pair"
members >"$tmp/warm"
cmp "$tmp/cold" "$tmp/warm"

# A busy old generation survives. The next due sweep removes only owned
# members. A remaining foreign file cannot turn a successful launch into a fault.
name=$(printf '%064d-%064d' 1 2)
old=$root/projects/$name
mkdir -m 700 "$old"
touch "$old/engine" "$old/stray"
touch -d '10 days ago' "$old" "$root/projects"
(
	flock -s 8
	run "$empty"
	test -f "$old/engine"
) 8<"$old"
touch -d '2 days ago' "$root/projects"
run "$empty"
test ! -e "$old/engine"
test -f "$old/stray"
members >"$tmp/after"
cmp "$tmp/cold" "$tmp/after"
echo "PASS shared project generations reused, locked generation preserved, expired member retired"
