//! Exercise the shell entry's real directory claim, Cargo clean and recovery.
use rnx_final::receipt::invocation_id;
use std::{
	fs,
	path::{Path, PathBuf},
	process::Command,
};

struct Temp(PathBuf);
impl Temp {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!("rnx-clean-control-{}", invocation_id()));
		fs::create_dir(&path).unwrap();
		Self(path)
	}
}
impl Drop for Temp {
	fn drop(&mut self) {
		fs::remove_dir_all(&self.0).unwrap();
	}
}

fn shell(script: &str) {
	let tmp = Temp::new();
	let clean = Path::new(env!("CARGO_MANIFEST_DIR")).join("clean");
	let out = Command::new("bash")
		.args(["-euc", script, "clean-test"])
		.arg(&tmp.0)
		.arg(clean)
		.env_remove("CARGO_TARGET_DIR")
		.env_remove("CARGO_BUILD_BUILD_DIR")
		.output()
		.unwrap();
	assert!(
		out.status.success(),
		"{}\n{}",
		String::from_utf8_lossy(&out.stdout),
		String::from_utf8_lossy(&out.stderr)
	);
}

#[test]
fn passing_failed_and_interrupted_runs_release_targets_and_preserve_a_foreign_sibling() {
	shell(
		r#"
		set -o pipefail
		base=$(realpath "$1")
		clean=$2
		source "$clean/ownership.sh"
		area=$base/area
		claim_directory "$area"
		exec 7<"$area"
		flock -nx 7
		unit=$(sed -n 's/^0:://p' /proc/self/cgroup)
		run=$area/run
		export RNX_FINAL_AREA=$area RNX_FINAL_CANDIDATE=tested RNX_FINAL_BASELINE=published
		for status in pass fail interrupt; do
			out=$base/$status
			mkdir "$out"
			[[ ! -e $run ]]
			claim_run "$area" "$unit"
			export RNX_FINAL_STARTED=$(cut -d' ' -f1 /proc/uptime)
			echo null >"$run/limits"
			echo '{"seconds":7,"cargo_home_allocated_bytes":8192}' >"$run/fetch.json"
			bash "$clean/space.sh" start "$run" "$out"
			mkdir -p "$run/source/src"
			printf '[package]\nname="disposable"\nversion="0.1.0"\nedition="2024"\n' >"$run/source/Cargo.toml"
			echo 'fn main(){if std::env::args().nth(1).as_deref()==Some("fail"){eprintln!("planted failure");std::process::exit(17)}}'  >"$run/source/src/main.rs"
			cargo +1.98.1 build --offline --manifest-path "$run/source/Cargo.toml" --target-dir "$run/source/target/final-controller"
			# Cargo has never used this declared group.
			mkdir -p "$run/source/target/final-builds/unused" "$run/source/target/build-recipes"
			jq -nc --arg t "$run/source/target/final-builds/unused" '{unused:{target:$t}}' >"$run/source/target/build-recipes/unused.json"
			touch "$base/foreign"
			if [[ $status == interrupt ]]; then
				sleep 300 & child=$!
				kill "$child"
				wait "$child" || true
				[[ ! -e /proc/$child ]]
			fi
			if [[ $status == fail ]]; then
				if "$run/source/target/final-controller/debug/disposable" fail 2>"$out/diagnostic"; then exit 1; else [[ $? == 17 ]]; fi
			fi
			[[ -z $(find "$out" -name FETCH.json) ]]
			bash "$clean/release.sh" "$run" "$out" terminal
			jq -e '.seconds==7 and .cargo_home_allocated_bytes==8192' "$out/FETCH.json"
			[[ ! -e $run && ! -e $area/attempt.json && -f $base/foreign ]]
			jq -e '.removed and .targets_cleaned==1 and .finished>=.started and (.peak_bytes|has("memory"))' "$out/CLEANUP.json"
		done
		[[ $(cat "$base/fail/diagnostic") == 'planted failure' ]]
	"#,
	);
}

#[test]
fn recovery_checks_the_abandoned_inode_before_deletion() {
	shell(
		r#"
		area=$1/area
		source "$2/ownership.sh"
		claim_directory "$area"
		claim_run "$area" "$(sed -n 's/^0:://p' /proc/self/cgroup)"
		jq '.boot="earlier boot"|.unit="/a.service"' "$area/attempt.json" >"$1/record"
		mv "$1/record" "$area/attempt.json"
		mv "$area/run" "$area/abandoned"
		claim_directory "$area/run"
		touch "$area/run/sentinel"
		! (set -e; recover_run "$area")
		[[ -f $area/run/sentinel && -d $area/abandoned ]]
		mv "$area/run" "$area/foreign"
		mv "$area/abandoned" "$area/run"
		recover_run "$area"
		[[ ! -e $area/run && ! -e $area/attempt.json && -f $area/foreign/sentinel ]]
	"#,
	);
}

#[test]
fn later_limits_come_from_the_publication_and_must_remain_identical() {
	shell(
		r#"
		source "$2/ownership.sh"
		cd "$1"
		git init -q
		git config user.name test
		git config user.email test@example.invalid
		git config commit.gpgsign false
		git commit --allow-empty -qm baseline
		! read_limits "$PWD" "$(git rev-parse HEAD)" false limits
		! read_limits "$PWD" b8d5ad340653623cc5e1855bc4d2c85ce00ccb8f false limits
		mkdir -p plans/impl
		path=plans/impl/0338-final-limits.json
		jq -nc --arg sha "$(printf '%064d' 0)" '{seconds:120,bytes:{build:200,memory:0},receipt_sha256:$sha}' >"$path"
		git add "$path"
		git commit -qm 'calibration'
		read_limits "$PWD" "$(git rev-parse HEAD)" false limits
		jq '.seconds=180' limits >"$path"
		! read_limits "$PWD" "$(git rev-parse HEAD)" false limits
		git commit -qam 'changed'
		! read_limits "$PWD" "$(git rev-parse HEAD)" false limits
	"#,
	);
}

#[test]
fn verified_downloads_fetch_locked_without_a_network_or_shared_checkout() {
	shell(
		r#"
		set -o pipefail
		run=$1
		downloads=$run/downloads
		source "$2/downloads.sh"
		cd "$run"
		export CARGO_HOME="$run/cold" CARGO_HTTP_PROXY=http://127.0.0.1:9
		export CARGO_NET_GIT_FETCH_WITH_CLI=true
		mkdir -p app/src origin/src example-1.0.0/src scratch
		echo 'pub fn answer() -> u32 {42}' >origin/src/lib.rs
		cp origin/src/lib.rs example-1.0.0/src/lib.rs
		touch app/src/lib.rs
		for package in origin example-1.0.0 app; do
			printf '[package]\nname="%s"\nversion="1.0.0"\nedition="2024"\n' "${package%-1.0.0}" > "$package/Cargo.toml"
		done
		git -C origin init -q
		git -C origin add .
		git -C origin -c user.name=fixture -c user.email=fixture@example.invalid -c commit.gpgsign=false commit -qm fixture
		lock=$(git -C origin rev-parse HEAD)
		tree=$(git -C origin rev-parse 'HEAD^{tree}')
		printf '[dependencies]\norigin={git="file://%s/origin", rev="%s"}\n' "$run" "$lock" >> app/Cargo.toml
		CARGO_NET_OFFLINE=false GIT_ALLOW_PROTOCOL=file cargo +1.98.1 generate-lockfile --manifest-path app/Cargo.toml
		# A tiny crates.io-style archive and the pinned Cargo's sparse cache
		# envelope. Cargo must resolve it, verify it, and unpack its source.
		index=$CARGO_HOME/registry/index/$sparse
		cache=$CARGO_HOME/registry/cache/$sparse
		mkdir -p "$index/.cache/ex/am" "$cache"
		tar czf "$cache/example-1.0.0.crate" example-1.0.0
		digest=$(sha256sum "$cache/example-1.0.0.crate" | cut -d' ' -f1)
		echo '{"dl":"https://static.crates.io/crates"}' > "$index/config.json"
		entry=$(jq -nc --arg sha "$digest" '{name:"example",vers:"1.0.0",deps:[],cksum:$sha,features:{},yanked:false}')
		printf '\003\002\000\000\000etag: "fixture"\0001.0.0\000%s\000' "$entry" > "$index/.cache/ex/am/example"
		echo 'example="=1.0.0"' >>app/Cargo.toml
		cargo +1.98.1 generate-lockfile --offline --manifest-path app/Cargo.toml
		download_plan app/Cargo.lock > "$run/download-plan"
		mkdir -p "$downloads"/{archives,index,git}
		save_downloads "$downloads" "$run/download-plan" "$run/scratch"
		mv origin unavailable
		export CARGO_HOME="$run/fresh"
		restore_downloads "$downloads" "$run/download-plan"
		[[ ! -e $CARGO_HOME/registry/src && ! -e $CARGO_HOME/git/checkouts && ! -e $CARGO_HOME/config.toml ]]
		went_online=false
		fetch_locked cargo +1.98.1 fetch --locked --manifest-path app/Cargo.toml
		[[ $went_online == false ]]
		read -r -d '' checkout < <(find "$CARGO_HOME/git/checkouts" -name .git -print0)
		[[ $(git -C "${checkout%/.git}" rev-parse HEAD) == "$lock" ]]
		[[ $(git -C "${checkout%/.git}" rev-parse 'HEAD^{tree}') == "$tree" ]]
		cmp example-1.0.0/src/lib.rs "$CARGO_HOME/registry/src/$sparse/example-1.0.0/src/lib.rs"
		echo broken >"$downloads/archives/$digest"
		export CARGO_HOME="$run/reject"
		if restore_downloads "$downloads" "$run/download-plan"; then exit 1; fi
		[[ ! -e $CARGO_HOME/registry/src ]]
	"#,
	);
}

#[test]
fn bootstrap_includes_the_external_comparisons_at_their_own_revisions() {
	shell(
		r#"
		set -o pipefail
		source "$2/downloads.sh"
		exporter=$(realpath "$2/../../export-source.sh")
		run=$1
		cd "$run"
		git init -q
		git config user.name test
		git config user.email test@example.invalid
		git config commit.gpgsign false
		echo anchor >Cargo.toml
		git add .
		git commit -qm anchor
		anchor=$(git rev-parse HEAD)
		mkdir -p adapters/polars
		echo fixed >Cargo.toml
		cp Cargo.toml adapters/polars/Cargo.toml
		git add .
		git commit -qm fixed
		fixed=$(git rev-parse HEAD)
		echo published >adapters/polars/Cargo.toml
		git commit -qam published
		baseline=$(git rev-parse HEAD)
		mkdir -p tools/final tools/column
		cp "$exporter" tools/export-source.sh
		echo "$fixed" >tools/final/clippy-base
		jq -nc --arg anchor "$anchor" '{anchor:$anchor}' >tools/column/fixed_route_policy.json
		plan_fetches "$baseline" true
		[[ "${manifests[*]}" == "ci/execute_guard/Cargo.toml Cargo.toml" && ! -e $run/scratch ]]
		plan_fetches "$baseline" false
		in_plan() { printf '%s\n' "${manifests[@]}" | grep -Fx -- "$1"; }
		for item in "$fixed/Cargo.toml" "$fixed/adapters/polars/Cargo.toml" "$baseline/adapters/polars/Cargo.toml" "$anchor/Cargo.toml"; do
			path=$run/scratch/fetch-sources/$item
			in_plan "$path"
			git show "${item%%/*}:${item#*/}" | cmp - "$path"
		done
	"#,
	);
}

#[test]
fn socket_path_admission_counts_bytes_before_claiming_the_area() {
	shell(
		r#"
		source "$2/ownership.sh"
		root=$(printf '/%031d' 0)
		check_socket_room "$root"
		# Two UTF-8 bytes still fit when replacing two ASCII bytes.
		root="${root%00}é"
		check_socket_room "$root"
		if check_socket_room "${root}a"; then exit 1; fi
		export RNX_FINAL_RUNS_ROOT="$1/${root#/}"
		cd "$2/../../.."
		if bash tools/final/clean/entry.sh 0 --complete --out "$1/receipts" 2>"$1/error"; then exit 1; fi
		grep -F 'stage TMPDIR must leave 47 bytes' "$1/error"
		[[ ! -e $RNX_FINAL_RUNS_ROOT && ! -e $1/receipts ]]
	"#,
	);
}
