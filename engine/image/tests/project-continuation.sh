#!/bin/sh
# Test the actual paired app with fd 9 occupied by the route observer.
set -eu
app=$(realpath -- "$1")
here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
cc -std=c11 -O2 -Wall -Werror -Wextra "$here/support/project-exec.c" -o "$tmp/control"
export XDG_CACHE_HOME="$tmp/cache"
for mode in Native SourceLimit interrupt; do
	dir="$tmp/$mode"
	if [ "$mode" = SourceLimit ]; then
		dir="$dir/$(printf %0140d 0)/$(printf %0100d 0)/$(printf %0040d 0)"
	fi
	mkdir -p "$dir"
	if [ "$mode" = interrupt ]; then
		cat >"$dir/main.rn" <<'RN'
pub async fn main(args) { polars::version(); time::sleep(30000).await?; Ok(()) }
RN
	else
		cat >"$dir/main.rn" <<'RN'
mod witness;
pub fn main(args) {
	let version = polars::version();
	println("effect");
	print!("{} {} {}", witness::tag(), version, args[0]);
	process::exit(17)?;
	Ok(())
}
RN
	fi
	printf '%s\n' 'pub fn tag(){325}' >"$dir/witness.rn"
	"$tmp/control" "$app" "$dir" "$mode" 9>"$dir/route" >"$dir/stdout" 2>"$dir/stderr" || {
		cat "$dir/stderr" >&2
		exit 1
	}
	category=Native
	case "$mode" in
	interrupt)
		printf 'interrupted\n' | cmp - "$dir/stderr"
		test ! -s "$dir/stdout"
		;;
	*)
		test ! -s "$dir/stderr"
		printf 'effect\n325 0.55.2 with space' | cmp - "$dir/stdout"
		if [ "$mode" = SourceLimit ]; then category=SourceLimit; fi
		;;
	esac
	jq -es --arg category "$category" \
		'length == 1 and .[0].route == "legacy_refusal" and .[0].category == $category' "$dir/route" >/dev/null
done
echo "PASS paired app continuation, saved inputs, single effect, output, and pending SIGINT"
