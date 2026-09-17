use super::*;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
struct Temp(PathBuf);
impl Temp {
	fn new() -> Self {
		static N: AtomicUsize = AtomicUsize::new(0);
		let p = std::env::temp_dir().join(format!(
			"rnx-cargo-inventory-{}-{}",
			std::process::id(),
			N.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&p).unwrap();
		Self(p.canonicalize().unwrap())
	}
	fn write(&self, p: &str, s: &str) {
		let p = self.0.join(p);
		fs::create_dir_all(p.parent().unwrap()).unwrap();
		fs::write(p, s).unwrap();
	}
	fn command(&self, args: &[&str]) -> Vec<u8> {
		let o = Command::new(args[0])
			.args(&args[1..])
			.current_dir(&self.0)
			.output()
			.unwrap();
		assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
		o.stdout
	}
	fn setup(&self) {
		self.write(".gitignore", "wrapper/target/\nwrapper/Cargo.lock\n");
		self.write("workspace/Cargo.toml","[workspace]\nmembers=['adapter','transitive']\nresolver='2'\n[workspace.package]\nversion='0.1.0'\n");
		self.write("workspace/adapter/Cargo.toml","[package]\nname='adapter'\nversion.workspace=true\nedition='2024'\n[dependencies]\ntransitive={path='../transitive'}\n");
		self.write(
			"workspace/adapter/src/lib.rs",
			"pub fn value()->i32 {transitive::value()}\n",
		);
		self.write(
			"workspace/transitive/Cargo.toml",
			"[package]\nname='transitive'\nversion.workspace=true\nedition='2024'\n",
		);
		self.write(
			"workspace/transitive/src/lib.rs",
			"pub fn value()->i32 {42}\n",
		);
		self.write("wrapper/Cargo.toml","[package]\nname='wrapper'\nversion='0.1.0'\nedition='2024'\n[workspace]\n[dependencies]\nadapter={path='../workspace/adapter'}\n");
		self.write(
			"wrapper/src/main.rs",
			"fn main(){println!(\"{}\",adapter::value());}\n",
		);
		self.command(&["git", "init", "--quiet"]);
		self.command(&["git", "add", "."]);
	}
	fn metadata(&self) -> Vec<u8> {
		self.command(&[
			"cargo",
			"metadata",
			"--offline",
			"--format-version",
			"1",
			"--manifest-path",
			"wrapper/Cargo.toml",
		])
	}
	fn inventory(&self, bytes: &[u8]) -> Inventory {
		native(
			bytes,
			&self.0.join("wrapper"),
			&self.0.join("wrapper"),
			&self.0.join("cargo-home"),
			&mut Allowance::default(),
		)
		.unwrap()
	}
}
impl Drop for Temp {
	fn drop(&mut self) {
		fs::remove_dir_all(&self.0).unwrap();
	}
}
#[test]
fn real_cargo_transitive_mutation_preserves_lock_but_changes_identity() {
	let t = Temp::new();
	t.setup();
	let metadata = t.metadata();
	let before = t.inventory(&metadata);
	assert_eq!(
		t.command(&[
			"cargo",
			"run",
			"--quiet",
			"--offline",
			"--manifest-path",
			"wrapper/Cargo.toml"
		]),
		b"42\n"
	);
	let lock = fs::read(t.0.join("wrapper/Cargo.lock")).unwrap();
	assert_eq!(before.packages.len(), 2);
	assert_eq!(before.trees.len(), 2);
	assert!(
		before
			.external
			.iter()
			.any(|f| f.path == t.0.join("workspace/Cargo.toml") && f.file.is_some())
	);
	t.write(
		"workspace/transitive/src/lib.rs",
		"pub fn value()->i32 {43}\n",
	);
	let after = t.inventory(&t.metadata());
	assert_eq!(
		t.command(&[
			"cargo",
			"run",
			"--quiet",
			"--offline",
			"--manifest-path",
			"wrapper/Cargo.toml"
		]),
		b"43\n"
	);
	assert_ne!(before, after);
	assert_eq!(lock, fs::read(t.0.join("wrapper/Cargo.lock")).unwrap());
	assert_eq!(before.trees[0], after.trees[0]);
	assert_ne!(before.trees[1], after.trees[1]);
	t.write("workspace/Cargo.toml","[workspace]\nmembers=['adapter','transitive']\nresolver='2'\n[workspace.package]\nversion='0.1.0'\n# dirty ancestor edit\n");
	assert_ne!(after.external, t.inventory(&metadata).external);
}
#[test]
fn shared_roots_keep_associations_but_hash_once() {
	let t = Temp::new();
	t.setup();
	let mut metadata: serde_json::Value = serde_json::from_slice(&t.metadata()).unwrap();
	let mut duplicate = metadata["packages"]
		.as_array()
		.unwrap()
		.iter()
		.find(|p| p["name"] == "adapter")
		.unwrap()
		.clone();
	duplicate["id"] = serde_json::json!("second-association");
	duplicate["name"] = serde_json::json!("second-name");
	metadata["packages"].as_array_mut().unwrap().push(duplicate);
	let inventory = t.inventory(&serde_json::to_vec(&metadata).unwrap());
	assert_eq!(inventory.packages.len(), 3);
	assert_eq!(inventory.trees.len(), 2);
}
#[test]
fn external_configuration_creation_and_include_stop() {
	let t = Temp::new();
	t.setup();
	let metadata = t.metadata();
	let before = t.inventory(&metadata);
	t.write(
		".cargo/config.toml",
		"[build]\nrustflags=['--cfg','my_flag']\n",
	);
	let after = t.inventory(&metadata);
	assert_ne!(before.external, after.external);
	t.write(".cargo/config.toml", "include=['external.toml']\n");
	let e = native(
		&metadata,
		&t.0.join("wrapper"),
		&t.0.join("wrapper"),
		&t.0.join("cargo-home"),
		&mut Allowance::default(),
	)
	.unwrap_err();
	assert!(e.contains("stop:"), "{e}");
}
#[test]
fn external_workspace_redirect_is_recorded() {
	let t = Temp::new();
	t.setup();
	t.write("other/Cargo.toml", "[workspace]\n");
	t.write(
		"workspace/adapter/Cargo.toml",
		"[package]\nname='adapter'\nversion='0.1.0'\nworkspace='../../other'\n",
	);
	// Metadata was resolved before the edit: this unit case isolates the audit's
	// explicit redirect traversal; build pre/post validation re-resolves separately.
	let fake = serde_json::json!({"workspace_root":t.0.join("wrapper"),"packages":[{"id":"adapter","name":"adapter","source":null,"manifest_path":t.0.join("workspace/adapter/Cargo.toml")}]});
	let i = t.inventory(&serde_json::to_vec(&fake).unwrap());
	assert!(
		i.external
			.iter()
			.any(|x| x.path == t.0.join("other/Cargo.toml") && x.file.is_some())
	);
}

#[test]
fn source_diamond_hashes_once_and_outside_manifest_is_an_input() {
	let t = Temp::new();
	t.write("app/rnx.toml","format=1\n[application]\nentry='src/main.rn'\n[executable]\npath='../rnx'\n[sources.left]\npath='../dep'\n[sources.right]\npath='../dep'\n");
	t.write(
		"app/src/main.rn",
		"mod left; mod right; pub fn main(_){42}\n",
	);
	t.write("dep/rnx.toml", "format=1\n[source]\nroot='src'\n");
	t.write("dep/src/mod.rn", "pub fn value(){42}\n");
	let path = t.0.join("app/rnx.toml");
	let before = sources(&path, &mut Allowance::default()).unwrap();
	assert_eq!(before.packages.len(), 2);
	assert_eq!(before.trees.len(), 2);
	assert_eq!(before.outside_manifests.len(), 2);
	t.write(
		"dep/rnx.toml",
		"format=1\n[source]\nroot='src'\n# changed\n",
	);
	let after = sources(&path, &mut Allowance::default()).unwrap();
	assert_eq!(before.trees, after.trees);
	assert_ne!(before.outside_manifests, after.outside_manifests);
}

#[test]
#[ignore = "requires the explicitly named, staged rnx repository and an external evidence path"]
fn repository_audit() {
	let repo = PathBuf::from(std::env::var_os("RNX_GATE3_REPO").expect("RNX_GATE3_REPO"));
	let output =
		PathBuf::from(std::env::var_os("RNX_GATE3_INVENTORY").expect("RNX_GATE3_INVENTORY"));
	assert!(repo.is_absolute() && output.is_absolute() && !output.starts_with(&repo));
	let m = Command::new("cargo")
		.current_dir(&repo)
		.args([
			"metadata",
			"--locked",
			"--offline",
			"--format-version",
			"1",
			"--manifest-path",
			"adapters/postgres/Cargo.toml",
		])
		.output()
		.unwrap();
	assert!(m.status.success(), "{}", String::from_utf8_lossy(&m.stderr));
	let home = std::env::var_os("CARGO_HOME")
		.map(PathBuf::from)
		.unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").expect("HOME")).join(".cargo"));
	let inventory = native(
		&m.stdout,
		&repo.join(".rnx/gate3-generated"),
		&repo,
		&home,
		&mut Allowance::default(),
	)
	.unwrap();
	assert_eq!(
		inventory
			.packages
			.iter()
			.map(|p| p.name.as_str())
			.collect::<BTreeSet<_>>(),
		BTreeSet::from(["rnx", "rnx-postgres"])
	);
	println!(
		"{} associations, {} native roots, {} external candidates, {} present external files",
		inventory.packages.len(),
		inventory.trees.len(),
		inventory.external.len(),
		inventory
			.external
			.iter()
			.filter(|x| x.file.is_some())
			.count()
	);
	fs::write(output, serde_json::to_vec_pretty(&inventory).unwrap()).unwrap();
}

#[cfg(unix)]
#[test]
fn declared_source_root_symlink_is_not_hidden_by_canonicalization() {
	use std::os::unix::fs::symlink;
	let t = Temp::new();
	t.write(
		"rnx.toml",
		"format=1\n[application]\nentry='link/main.rn'\n[executable]\npath='rnx'\n",
	);
	t.write("real/main.rn", "pub fn main(_){42}\n");
	symlink("real", t.0.join("link")).unwrap();
	assert!(
		sources(&t.0.join("rnx.toml"), &mut Allowance::default())
			.unwrap_err()
			.contains("symlink source root")
	);
}
