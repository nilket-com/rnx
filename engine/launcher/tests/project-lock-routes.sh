#!/bin/sh
# Compare genuine old locks with new locks on the same pre-handoff runtime.
set -eu
old=$(realpath -- "$1")
manager=$(realpath -- "$2")
tmp=$(mktemp -d)
trap 'rm -rf -- "$tmp"' EXIT
runtime="$tmp/runtime"
mkdir -p "$runtime/src"
cat >"$runtime/Cargo.toml" <<'TOML'
[package]
name="rnx"
version="0.0.0"
edition="2024"
[features]
count-allocations=[]
project-sources=[]
TOML
cat >"$runtime/src/lib.rs" <<'RS'
pub struct Extensions;
impl Extensions { pub fn none() -> Self { Self } }
pub fn main_with(_: Extensions) -> Result<(), Box<dyn std::error::Error>> {
    assert!(std::env::var_os("RNX_FILE_ROUTE_FD").is_none());
    println!("script effect");
    Ok(())
}
RS
git -C "$runtime" init -q
git -C "$runtime" add Cargo.toml src
export RNX_PROJECT_CACHE="$tmp/cache"
for format in 1 2; do
	app="$tmp/app-$format"
	mkdir -p "$app/src"
	printf 'pub fn main(_){}\n' >"$app/src/main.rn"
	printf 'format=%s\n[application]\nentry="src/main.rn"\n[runtime]\npath="../runtime"\n' "$format" >"$app/rnx.toml"
	for reason in historical_lock runtime_without_handoff; do
		if [ "$reason" = historical_lock ]; then generator=$old; else generator=$manager; fi
		"$generator" lock --manifest "$app/rnx.toml" --offline
		"$manager" build --manifest "$app/rnx.toml" --offline
		RNX_FILE_ROUTE_FD=9 "$manager" run --manifest "$app/rnx.toml" 9>"$tmp/route" >"$tmp/out"
		printf 'script effect\n' | cmp - "$tmp/out"
		case "$reason" in historical_lock) route=historical_lock ;; *) route=manifest_ineligible ;; esac
		jq -es --arg reason "$reason" --arg route "$route" 'length == 1 and .[0].reason == $reason and .[0].route == $route' "$tmp/route" >/dev/null
	done
	printf '// changed after lock\n' >>"$app/src/main.rn"
	if RNX_FILE_ROUTE_FD=9 "$manager" run --manifest "$app/rnx.toml" 9>"$tmp/route" >"$tmp/out" 2>"$tmp/err"; then exit 1; fi
	test ! -s "$tmp/route" && test ! -s "$tmp/out"
	grep -qi changed "$tmp/err"
done
echo 'PASS historical and retained-runtime events through both project managers and stale-lock checks'
