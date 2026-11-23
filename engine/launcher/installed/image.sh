#!/bin/sh
# Build the common process-owning image independently of the selected provider.
set -eu
work=$1
shift
# Give the nested Cargo its own target lock and the outer build's jobserver.
export MAKEFLAGS="${CARGO_MAKEFLAGS-}"
# Cargo sets enabled feature variables but child processes inherit the rest.
unset CARGO_FEATURE_STOCK_MANAGEMENT CARGO_FEATURE_CROSS_VERIFICATION CARGO_FEATURE_TEST_SUPPORT CARGO_FEATURE_PROJECT_SOURCES
# Clippy owns the outer lint invocation. This command links a production cdylib.
case ${RUSTC_WORKSPACE_WRAPPER-} in
	*/clippy-driver|clippy-driver) unset RUSTC_WORKSPACE_WRAPPER CLIPPY_ARGS ;;
esac
# The compiler emits the C main for an ordinary Rust process. Rename that
# entry so the C helper can verify its pinned descriptors first. Link only
# the entry object into the image, so startup and script execution share std.
"$RUSTC" --edition=2024 --crate-name rnx_installed_process --target "$TARGET" \
	-O -C relocation-model=pic --emit=obj engine/launcher/installed/process.rs -o "$work/process.o"
objcopy --redefine-sym main=rnx_installed_start "$work/process.o" "$work/start.o"
ar crs "$work/libinstalled_start.a" "$work/start.o"
# The native object is opaque to rustc's ThinLTO. Keep its Rust references
# visible using the symbol names from this build of rustc. These declarations
# take addresses only. The compiler-generated object makes the actual calls.
nm --undefined-only --format=posix "$work/start.o" >"$work/imports"
awk '$1 ~ /^_R[A-Za-z0-9_]+$/ {
	printf "unsafe extern \"C\" { #[link_name=\"%s\"] fn import_%d(); }\n", $1, NR
	printf "#[used] static KEEP_%d: unsafe extern \"C\" fn() = import_%d;\n", NR, NR
}' "$work/imports" >"$work/runtime_imports.rs"
export RNX_INSTALLED_RUNTIME_IMPORTS="$work/runtime_imports.rs"
"$CARGO" rustc --locked --release --manifest-path engine/image/Cargo.toml \
	--target "$TARGET" --target-dir "$work/target" --example engine_image --features "$@" -- --cfg rnx_installed_start -L "native=$work"
