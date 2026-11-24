#!/bin/bash
# Pre-sign control for the stock release export. Keep the three actual shipped
# files and their measurements. Each Cargo target belongs to one cold build.
set -euo pipefail
repo=$(realpath "$1")
rev=$(git -C "$repo" rev-parse --verify "$2^{commit}")
out=$(realpath -m "$3")
mkdir "$out"
trap 's=$?; if (( s )); then echo FAIL > "$out/verdict"; fi' EXIT
export LC_ALL=C TZ=UTC CARGO_INCREMENTAL=0
export CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS:-4}
unset RUSTC_WRAPPER RUSTC_WORKSPACE_WRAPPER
fixture_commit() {
	git -C "$1" -c commit.gpgsign=false -c user.name=fixture -c user.email=fixture@invalid commit -qm "$2" --allow-empty
}
sections() {
	local file=$1 scratch=$2 section
	while read -r section; do
		objcopy --dump-section "$section=$scratch/section" "$file" "$scratch/copy"
		if [[ -f "$scratch/section" ]]; then
			printf '%s %s\n' "$section" "$(sha256sum < "$scratch/section")"
			rm "$scratch/section"
		fi
	done < <(objdump -h "$file" | awk '/^[ ]*[0-9]+ /{print $2}')
	rm -f "$scratch/copy"
}
build() {
	local parent=$1 gitroot=$2 revision=$3 role=$4
	local row="$out/$role" src="$parent/source" tmp="$parent/temporary" target="$parent/target"
	mkdir -p "$parent" "$row" "$src" "$tmp" "$target"
	git -C "$parent" init -q
	fixture_commit "$parent" "$role"
	git -C "$parent" rev-parse HEAD > "$row/enclosing-head"
	(cd "$gitroot"; sh tools/export-source.sh "$revision") | tar -x -C "$src"
	{
		printf 'commit %s\nrune %s\n' "$revision" "$(git -C "$gitroot" rev-parse "$revision:rune")"
		sha256sum "$src/Cargo.lock" "$src/engine/launcher/installed/"*.sh
		rustc +1.98.1 -vV
		cargo +1.98.1 -V
		cc --version
		ld --version
		printf 'CARGO_BUILD_JOBS=%s\nCARGO_INCREMENTAL=%s\n' "$CARGO_BUILD_JOBS" "$CARGO_INCREMENTAL"
	} > "$row/inputs"
	(cd "$src"; TMPDIR="$tmp" CARGO_TARGET_DIR="$target" sh engine/launcher/installed/build-export.sh \
		cargo +1.98.1 build --locked --release --bin rnx) > "$row/build.log" 2>&1
	cp "$target/release/rnx" "$row/rnx"
	local tree
	tree=$(git -C "$gitroot" rev-parse "$revision:rune")
	"$row/rnx" version > "$row/version"
	grep -Fx "rune 0.15.0 (monorepo $revision; engine $tree; dirty=false)" "$row/version"
	printf 'pub fn main(args) { 42 }\n' > "$row/witness.rn"
	XDG_CACHE_HOME="$row/cache" "$row/rnx" run --debug-engine "$row/witness.rn" > "$row/stdout" 2> "$row/stderr"
	grep -Fxq 42 "$row/stdout"
	grep -Fq 'rnx: engine=execution' "$row/stderr"
	local generations=("$row"/cache/rnx-file-images/*)
	[[ ${#generations[@]} == 1 ]]
	local member offset size
	for member in engine helper; do cp "${generations[0]}/$member" "$row/$member"; done
	for member in engine helper rnx; do
		sha256sum < "$row/$member" > "$row/$member.sha256"
		readelf -lW "$row/$member" > "$row/$member.segments"
		grep -q "^[[:space:]]*LOAD " "$row/$member.segments"
		while read -r offset size; do
			dd if="$row/$member" of="$row/segment" iflag=skip_bytes,count_bytes skip="$((offset))" count="$((size))" status=none
			if grep -aF -e "$parent" -e "$src" -e "$target" -e "$tmp" "$row/segment" >/dev/null; then
				echo "$member contains its physical build path" >&2; return 1
			fi
		done < <(awk '$1=="LOAD"{print $2,$5}' "$row/$member.segments")
	done
	rm "$row/segment"
	rm -rf -- "$target" "$row/cache"
}
build "$out/short" "$repo" "$rev" a
build "$out/a-deliberately-longer-enclosing-checkout" "$repo" "$rev" b
! cmp -s "$out/a/enclosing-head" "$out/b/enclosing-head" || exit 1
different=0
for member in engine helper rnx; do
	if ! cmp -s "$out/a/$member" "$out/b/$member"; then
		for role in a b; do sections "$out/$role/$member" "$out/$role" > "$out/$role/$member.sections"; done
		diff -u "$out/a/$member.sections" "$out/b/$member.sections" > "$out/$member.diff" || test "$?" -eq 1
		different=1
	fi
done
(( different == 0 ))
git clone -q --shared --no-checkout "$repo" "$out/plant"
git -C "$out/plant" checkout -q --detach "$rev"
sed -i '/^		unsafe { rnx_installed_start(argc, argv) }$/i\		eprintln!("0331 image behavior control");' "$out/plant/engine/image/src/stock.rs"
printf '\n0331 subtree identity control\n' >> "$out/plant/rune/README.md"
git -C "$out/plant" add engine/image/src/stock.rs rune/README.md
fixture_commit "$out/plant" 'plant observable image behavior and change the rune subtree'
build "$out/planted-export" "$out/plant" "$(git -C "$out/plant" rev-parse HEAD)" plant-result
grep -Fxq '0331 image behavior control' "$out/plant-result/stderr"
if grep -Fxq '0331 image behavior control' "$out/a/stderr"; then exit 1; fi
for member in engine rnx; do
	if cmp -s "$out/a/$member" "$out/plant-result/$member"; then exit 1; fi
done
echo PASS > "$out/verdict"
