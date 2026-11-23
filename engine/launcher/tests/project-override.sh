#!/bin/sh
# Both lock formats verify their artifact before reporting a manager-owned event.
set -eu
manager=$(realpath -- "$1")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
for format in 1 2; do
	app=$tmp/v$format
	mkdir -p "$app/src"
	printf 'pub fn main(_){}\n' >"$app/src/main.rn"
	cat >"$app/override" <<'RUN'
#!/bin/sh
test -z "${RNX_FILE_ROUTE_FD+x}" || exit 91
printf 'effect %s\n' "$1"
RUN
	chmod +x "$app/override"
	printf 'format=%s\n[application]\nentry="src/main.rn"\n[executable]\npath="override"\n' "$format" >"$app/rnx.toml"
	for step in lock build; do "$manager" "$step" --manifest "$app/rnx.toml" --offline; done
	RNX_FILE_ROUTE_FD=9 "$manager" run --manifest "$app/rnx.toml" 9>"$tmp/events" >"$tmp/stdout"
	printf 'effect run\n' | cmp - "$tmp/stdout"
	jq -es 'length == 1 and .[0].route == "manifest_ineligible" and .[0].reason == "executable_override"' "$tmp/events" >/dev/null
	printf '// now stale\n' >>"$app/src/main.rn"
	if RNX_FILE_ROUTE_FD=9 "$manager" run --manifest "$app/rnx.toml" 9>"$tmp/events" >"$tmp/stdout" 2>"$tmp/error"; then exit 1; fi
	test ! -s "$tmp/events" && test ! -s "$tmp/stdout"
	grep -qi 'changed' "$tmp/error"
done
echo 'PASS both executable override formats emit once and stale locks fail before selection'
