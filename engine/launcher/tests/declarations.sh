#!/bin/sh
# Exercise the project parser before any native build or tool lookup.
set -eu
tool=$(realpath "$1")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
cd "$tmp"
mkdir bin
for name in cargo ld cc rustc; do
	printf '%s\n' '#!/bin/sh' 'touch "$TRIPPED"; exit 93' >"bin/$name"
	chmod +x "bin/$name"
done
export TRIPPED="$tmp/tripped"
export PATH="$tmp/bin:$PATH"
for set in candle polars postgres 'polars postgres' 'polars candle postgres' 'polars candle'; do
	printf '%s\n' 'format = 1' '[application]' 'entry="main.rn"' '[runtime]' 'path="."' > project.toml
	for alias in $set; do
		builder=build
		if [ "$set" = 'polars candle' ] && [ "$alias" = candle ]; then builder=other_builder; fi
		printf '[native.%s]\npath="."\npackage="rnx-%s"\nhook="plain"\nbuilder="%s"\npresentation=true\n' "$alias" "$alias" "$builder" >> project.toml
	done
	if "$tool" image project project.toml engine output >out 2>err; then exit 1; fi
	case "$set" in
		'polars candle') reason="unsupported native declaration for 'candle'; expected the reviewed rnx-candle provider" ;;
		*) names=$(printf '%s\n' $set | sort | paste -sd, | sed 's/,/, /g'); reason="unsupported native adapters [$names]; supported sets are [] and [candle, polars]" ;;
	esac
	test "$(cat err)" = "rnx-project: image package: $reason"
	test ! -e "$TRIPPED" && test ! -e output
done
