#!/bin/sh
# Use an owned detached checkout so the provider can change at the same
# canonical path. Keep the outputs for callers that measure the two launchers.
set -eu
if [ "$#" != 4 ]; then echo "usage: $0 PROJECT-TOOL CHECKOUT REVISION NEW-OUTPUT" >&2; exit 2; fi
tool=$(realpath "$1")
checkout=$(realpath "$2")
revision=$(git -C "$checkout" rev-parse --verify "$3^{commit}")
mkdir -m 700 -- "$4"
trial=$(realpath "$4")
root="$trial/source"
git -C "$checkout" worktree add --quiet --detach "$root" "$revision"
cleanup() { git -C "$checkout" worktree remove --force "$root"; }
trap cleanup EXIT
mkdir "$trial/project" "$trial/project/src"
mkdir "$trial/tools"
REAL_CARGO=$(command -v cargo)
export REAL_CARGO RNX_PACKAGE_CARGO_TRACE
cat >"$trial/tools/cargo" <<'SH'
#!/bin/sh
printf '%s\n' "$*" >>"$RNX_PACKAGE_CARGO_TRACE"
exec "$REAL_CARGO" "$@"
SH
chmod +x "$trial/tools/cargo"
export PATH="$trial/tools:$PATH"
printf '%s\n' 'pub fn main(_) { 42 }' >"$trial/project/src/main.rn"
# Encode the path as a TOML basic string, including spaces and quotes.
encoded=$(jq -Rn --arg s "$root" '$s')
manifest="$trial/project/rnx.toml"
printf 'format=1\n[application]\nentry="src/main.rn"\n[runtime]\npath=%s\n' "$encoded" >"$manifest"
for alias in candle polars; do
	encoded=$(jq -Rn --arg s "$root/adapters/$alias" '$s')
	printf '[native.%s]\npath=%s\npackage="rnx-%s"\nbuilder="build"\nhook="plain"\npresentation=true\n' "$alias" "$encoded" "$alias" >>"$manifest"
done
timed() {
	label=$1; shift
	RNX_PACKAGE_CARGO_TRACE="$trial/$label-cargo"
	/usr/bin/time -a -f "$label %e" -o "$trial/wall-seconds" "$@" 2>"$trial/$label.log"
}
project_build() {
	timed "$1" "$tool" build --manifest "$manifest" --engine-bundle "$trial/engine" --image-package "$trial/pair" --offline
}
state() { stat -c '%i %s' "$trial/pair" "$trial/pair/"*; }
timed engine "$tool" image engine "$root" "$trial/engine" --offline
timed stock "$tool" image stock "$root" "$trial/engine" "$trial/stock" --offline
grep -q '^build .* -p rnx-engine-image ' "$trial/engine-cargo"
"$tool" lock --manifest "$manifest" --offline
project_build pair
cp -R -- "$trial/pair" "$trial/original"
state >"$trial/generation"
project_build unchanged
state | cmp "$trial/generation" -
sh "$root/engine/launcher/tests/packages.sh" "$trial/stock" "$trial/pair" "$root/demos/data"
sh "$root/engine/launcher/tests/rebuild.sh" "$tool" "$root" "$trial/engine"
# Packaging uses a private target, but the provider's own Cargo controls use
# its default target. Exercise both locations in this detached checkout.
provider_target=$(env -u CARGO_TARGET_DIR cargo metadata --offline --locked --no-deps --format-version 1 \
	--manifest-path "$root/adapters/file-provider/Cargo.toml" | jq -er .target_directory)
test "$provider_target" = "$root/adapters/file-provider/target"
mkdir -p "$provider_target"
touch "$provider_target/.rnx-build-control"
test -z "$(git -C "$root" status --porcelain=v1 --untracked-files=all)"
printf '%s\n' 'pub fn main(args) { candle::Mlp::load(args[0])?.dims() }' >"$trial/dims.rn"
dims() { "$trial/pair/rnx" run "$trial/dims.rn" "$root/demos/data/mlp.safetensors"; }
dims >"$trial/dims-before"
provider=adapters/file-provider/src/lib.rs
sed -i 's/c + i64::from(cfg!(feature = "image-witness"))/c + 1 + i64::from(cfg!(feature = "image-witness"))/' "$root/$provider"
test "$(git -C "$root" diff --name-only)" = "$provider"
"$tool" lock --manifest "$manifest" --offline
project_build changed
dims >"$trial/dims-after"
# The unmodified fixture reports its three dimensions as a Rune tuple.
awk -F '[^0-9]+' 'NR==1 { a=$2; b=$3; c=$4 } NR==2 { if ($2!=a || $3!=b || $4!=c+1) exit 1 } END { if (NR!=2) exit 1 }' "$trial/dims-before" "$trial/dims-after"
cmp "$trial/original/libengine_image.so" "$trial/pair/libengine_image.so"
grep -q '^build .* --manifest-path adapters/file-provider/Cargo.toml ' "$trial/changed-cargo"
if grep -q '^build .*rnx-engine-image' "$trial/changed-cargo" || grep -q '^build ' "$trial/unchanged-cargo"; then exit 1; fi
if cmp -s "$trial/original/rnx" "$trial/pair/rnx"; then exit 1; fi
test -z "$(find "$trial" -maxdepth 1 -name '.rnx-image-build-*' -print)"
sha256sum "$trial/"*/rnx "$trial/"*/libengine_image.so >"$trial/artifacts.sha256"
du -s -B1 "$trial/stock" "$trial/pair" "$trial/engine" >"$trial/allocated"
test "$(git -C "$root" status --porcelain=v1 --untracked-files=all)" = " M $provider"
