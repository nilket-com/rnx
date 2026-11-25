# Download reuse before the Rust controller exists. The entry holds the area
# lock. Cargo unpacks into an empty private home; only input bytes survive.
# This is the pinned Cargo's crates.io sparse-registry directory.
sparse=index.crates.io-1949cf8c6b5b557f

plan_fetches() {
	local baseline=$1 successor=$2 fixed anchor manifest revision
	manifests=(ci/execute_guard/Cargo.toml Cargo.toml)
	inventory=()
	if [[ $successor == true ]]; then return 0; fi
	manifests+=(adapters/{candle,file-provider,polars,postgres}/Cargo.toml
		demos/{candle,polars}/Cargo.toml jupyter/Cargo.toml tools/project/Cargo.toml
		servers/http-postgres/Cargo.toml rune/hegel_tests/Cargo.toml
		rune/tools/xtask/{measure/harness,column/probe}/Cargo.toml
		probes/0072/extract/Cargo.toml tools/polars-gen/Cargo.toml)
	inventory=(adapter-json adapter adapter-narrow)

	local -a current=("${manifests[@]}")
	read -r fixed < tools/final/clippy-base
	anchor=$(jq -er .anchor tools/column/fixed_route_policy.json)
	# Use the same full source exports as the comparisons. Cargo resolves
	# path dependencies and workspace members relative to each revision.
	for revision in "$baseline" "$fixed" "$anchor"; do
		[[ $revision =~ ^[0-9a-f]{40}$ ]] || return 1
		local source="$run/scratch/fetch-sources/$revision"
		if [[ ! -d $source ]]; then
			mkdir -p -- "$source"
			sh tools/export-source.sh "$revision" | tar -xf - -C "$source" || return 1
		fi
	done
	for manifest in "${current[@]}"; do
		local path="$run/scratch/fetch-sources/$baseline/$manifest"
		if [[ -f $path ]]; then manifests+=("$path"); fi
	done
	manifests+=("$run/scratch/fetch-sources/$fixed/Cargo.toml"
		"$run/scratch/fetch-sources/$fixed/adapters/polars/Cargo.toml"
		"$run/scratch/fetch-sources/$anchor/Cargo.toml")
}

download_plan() {
	# Parse the package tables with the system TOML reader. The lock, never
	# the sparse index or a previous run, supplies an archive's identity.
	/usr/bin/python3 -I -B -S - "$@" <<'PY'
import re
import sys
import tomllib

archives = {}
for lock in sys.argv[1:]:
    with open(lock, "rb") as file:
        packages = tomllib.load(file)["package"]
    for package in packages:
        if package.get("source") != "registry+https://github.com/rust-lang/crates.io-index":
            continue
        name = package["name"] + "-" + package["version"] + ".crate"
        digest = package["checksum"]
        if not re.fullmatch(r"[A-Za-z0-9_.+-]+", name) or not re.fullmatch(r"[0-9a-f]{64}", digest):
            sys.exit("invalid archive identity in " + lock)
        if archives.setdefault(name, digest) != digest:
            sys.exit("conflicting lock checksums for " + name)
for name, digest in sorted(archives.items()):
    print(digest, name)
PY
}

restore_downloads() {
	local store=$1 plan=$2 digest name bundle db
	mkdir -p -- "$store"/{archives,index,git} "$CARGO_HOME/registry/cache/$sparse" "$CARGO_HOME/registry/index/$sparse"
	while read -r digest name; do
		[[ -e $store/archives/$digest ]] || continue
		cp --reflink=auto -- "$store/archives/$digest" "$CARGO_HOME/registry/cache/$sparse/$name"
		# Check the private copy before Cargo can read or unpack it.
		(cd "$CARGO_HOME/registry/cache/$sparse" && sha256sum --status -c <<<"$digest  $name") ||
			{ echo "retained archive checksum mismatch: $name" >&2; return 1; }
	done <"$plan"
	if [[ -d $store/index/.cache ]]; then
		rsync -rt -- "$store/index/.cache" "$CARGO_HOME/registry/index/$sparse/"
		# Registry metadata, not a Cargo configuration file.
		printf '%s\n' '{"dl":"https://static.crates.io/crates","api":"https://crates.io"}' >"$CARGO_HOME/registry/index/$sparse/config.json"
	fi
	for bundle in "$store"/git/*.bundle; do
		[[ -f $bundle ]] || continue
		name=${bundle##*/}
		db="$CARGO_HOME/git/db/${name%.bundle}"
		git init -q --bare --template= "$db"
		git -C "$db" -c fetch.fsckObjects=true fetch -q --no-tags "$bundle" '+refs/*:refs/*' || return 1
		git -C "$db" fsck --full --no-dangling >/dev/null || return 1
	done
}

save_downloads() {
	local store=$1 plan=$2 scratch=$3 digest name archive db candidate stored
	# Only locked archives can enter the store. An interrupted update never
	# replaces a published object with partial bytes.
	while read -r digest name; do
		[[ ! -e $store/archives/$digest ]] || continue
		archive="$CARGO_HOME/registry/cache/$sparse/$name"
		[[ -f $archive ]] || continue
		candidate="$scratch/$digest"
		cp --reflink=auto -- "$archive" "$candidate"
		(cd "$scratch" && sha256sum --status -c <<<"$digest  $digest") ||
			{ echo "downloaded archive checksum mismatch: $name" >&2; return 1; }
		mv -T -- "$candidate" "$store/archives/$digest"
	done <"$plan"
	if [[ -d $CARGO_HOME/registry/index/$sparse/.cache ]]; then
		rsync -rt -- "$CARGO_HOME/registry/index/$sparse/.cache" "$store/index/"
	fi
	for db in "$CARGO_HOME"/git/db/*; do
		[[ -d $db ]] || continue
		# A bundle contains objects and refs, never a checkout, hooks, or a
		# repository configuration. Cargo resolves the lock's precise commit
		# against this database and materializes its tree in the private home.
		stored="$store/git/${db##*/}.bundle"
		git -C "$db" bundle create "$scratch/git.bundle" --all &&
			mv -T -- "$scratch/git.bundle" "$stored" || return 1
	done
}

fetch_locked() {
	# Cargo accepts CARGO_NET_OFFLINE for both normal and inventory fetches.
	if CARGO_NET_OFFLINE=true "$@" >"$run/offline-fetch.log" 2>&1; then return 0; fi
	cat "$run/offline-fetch.log" >&2
	went_online=true
	CARGO_NET_OFFLINE=false "$@" || return 1
	save_downloads "$downloads" "$run/download-plan" "$run/scratch"
}
