use crate::{
	input,
	manifest::{Hook, Manifest},
	wire::{self, Assembly, Handoff, Lock},
};
use std::{
	path::{Path, PathBuf},
	sync::atomic::{AtomicUsize, Ordering},
};
struct Tree(PathBuf);
impl Tree {
	fn new() -> Self {
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let path = std::env::temp_dir().join(format!(
			"rnx-project-input-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		std::fs::create_dir_all(&path).unwrap();
		Self(path.canonicalize().unwrap())
	}
	fn write(&self, path: &str, text: &str) -> PathBuf {
		let path = self.0.join(path);
		std::fs::create_dir_all(path.parent().unwrap()).unwrap();
		std::fs::write(&path, text).unwrap();
		path
	}
}
impl Drop for Tree {
	fn drop(&mut self) {
		std::fs::remove_dir_all(&self.0).unwrap();
	}
}
const APP: &str = "format=1\n[application]\nentry='main.rn'\n[runtime]\npath='../rnx'\n";
fn app() -> Manifest {
	Manifest::parse(APP.as_bytes()).unwrap()
}
fn handoff(root: &Path) -> Handoff {
	Handoff {
		format: 1,
		entry: root.join("main.rn").to_str().unwrap().into(),
		mounts: vec![],
	}
}
fn lock(root: &Path) -> Lock {
	Lock {
		format: 1,
		declarations: app(),
		sources: handoff(root),
		packages: vec![],
		assembly: Assembly::Generated {
			manifest_sha256: "a".repeat(64),
			main_sha256: "b".repeat(64),
			cargo_lock_sha256: "c".repeat(64),
			target: "test-target".into(),
			profile: "release".into(),
			features: vec!["project-sources".into()],
			rustc: "rustc test".into(),
			cargo: "cargo test".into(),
		},
	}
}

#[test]
fn manifests_round_trip_and_reject_conflicts_and_unknowns() {
	let document = app();
	let text = toml::to_string(&document).unwrap();
	assert_eq!(Manifest::parse(text.as_bytes()).unwrap(), document);
	for text in [
		"format=2\n[application]\nentry='x'\n[runtime]\npath='y'",
		"format=1",
		"format=1\nformat=1",
		"format=1\nother=1\n[source]\nroot='.'",
		"format=1\n[source]\nroot='.'\n[runtime]\npath='x'",
		"format=1\n[application]\nentry='x'\n",
		"format=1\n[application]\nentry='x'\n[runtime]\npath='y'\n[executable]\npath='z'",
		"format=1\n[source]\nroot='.'\n[application]\nentry='x'",
		"format=1\n[source]\nroot='.'\nunknown=1",
	] {
		assert!(Manifest::parse(text.as_bytes()).is_err(), "{text}");
	}
	for suffix in [
		"[sources.std]\npath='x'",
		"[sources.'bad name']\npath='x'",
		"[sources.pkg]\npath=''",
		"[native.std]\npath='x'\npackage='a'\nbuilder='build'\nhook='plain'",
		"[native.pg]\npath='x'\npackage='a'\nbuilder='build'\nhook='wrong'",
		"[sources.pg]\npath='x'\n[native.pg]\npath='x'\npackage='a'\nbuilder='build'\nhook='plain'",
	] {
		assert!(
			Manifest::parse(format!("{APP}{suffix}").as_bytes()).is_err(),
			"{suffix}"
		);
	}
	let override_text = "format=1\n[application]\nentry='x'\n[executable]\npath='existing'\n";
	assert!(Manifest::parse(override_text.as_bytes()).is_ok());
	assert!(
		Manifest::parse(
			format!(
				"{override_text}[native.pg]\npath='x'\npackage='a'\nbuilder='build'\nhook='plain'"
			)
			.as_bytes()
		)
		.is_err()
	);
}
#[test]
fn manifest_read_bound_utf8_and_regular_files() {
	let t = Tree::new();
	let path = t.write("rnx.toml", APP);
	let mut bytes = APP.as_bytes().to_vec();
	bytes.resize(input::MANIFEST_LIMIT, b' ');
	std::fs::write(&path, &bytes).unwrap();
	assert!(Manifest::read(&path).is_ok());
	bytes.push(b' ');
	std::fs::write(&path, &bytes).unwrap();
	assert!(Manifest::read(&path).unwrap_err().contains("1048576"));
	std::fs::write(&path, [0xff]).unwrap();
	assert!(Manifest::read(&path).is_err());
	assert!(Manifest::read(&t.0).unwrap_err().contains("regular file"));
	#[cfg(unix)]
	{
		use std::{ffi::CString, os::unix::ffi::OsStrExt};
		let path = t.0.join("fifo");
		let c = CString::new(path.as_os_str().as_bytes()).unwrap();
		assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);
		assert!(Manifest::read(&path).unwrap_err().contains("regular file"));
	}
}
#[test]
fn real_manifests_expand_transitives_diamond_and_cycle() {
	let t = Tree::new();
	let app = t.write(
		"app/rnx.toml",
		&format!("{APP}[sources.left]\npath='../left'\n[sources.right]\npath='../right'\n"),
	);
	for side in ["left", "right"] {
		t.write(
			&format!("{side}/rnx.toml"),
			"format=1\n[source]\nroot='code'\n[sources.shared]\npath='../shared'\n",
		);
	}
	t.write("shared/rnx.toml", "format=1\n[source]\nroot='.'\n");
	let map = Handoff::from_project(&app).unwrap();
	assert_eq!(map.mounts.len(), 4);
	assert_eq!(map.mounts[1].root, map.mounts[3].root);
	assert_eq!(map.mounts[1].prefix, ["left", "shared"]);
	assert_eq!(map.mounts[3].prefix, ["right", "shared"]);
	let bytes = map.encode().unwrap();
	assert_eq!(Handoff::decode(&bytes).unwrap(), map);
	t.write(
		"shared/rnx.toml",
		"format=1\n[source]\nroot='.'\n[sources.back]\npath='../left'\n",
	);
	assert!(
		Handoff::from_project(&app)
			.unwrap_err()
			.contains("left::shared::back")
	);
	t.write("shared/rnx.toml", APP);
	assert!(
		Handoff::from_project(&app)
			.unwrap_err()
			.contains("not a source package")
	);
}
#[test]
fn native_generation_is_ordered_and_cannot_inject_rust() {
	let a = "[native.z]\npath='z'\npackage='pkg-z'\nbuilder='api::build'\nhook='lifecycle'\n";
	let b = "[native.a]\npath='a'\npackage='pkg-a'\nbuilder='build'\nhook='plain'\n";
	let one = Manifest::parse(format!("{APP}{a}{b}").as_bytes()).unwrap();
	let two = Manifest::parse(format!("{APP}{b}{a}").as_bytes()).unwrap();
	let base = Tree::new();
	let (cargo, main) = crate::generate::wrapper(&one, &base.0).unwrap();
	assert_eq!(
		crate::generate::wrapper(&two, &base.0).unwrap(),
		(cargo.clone(), main.clone())
	);
	assert!(
		main.contains(".with(\"a\", native_0::build).with_lifecycle(\"z\", native_1::api::build)")
	);
	let _: toml::Value = toml::from_str(&cargo).unwrap();
	assert!(cargo.contains("project-sources"));
	assert_eq!(one.native["z"].hook, Hook::Lifecycle);
	for bad in [
		"build); panic!()",
		"build()",
		"::build",
		"api::",
		"crate::build",
		"gen",
		"a-b",
		"r#build",
		"\n",
		"_",
		"x::<T>",
	] {
		assert!(!crate::manifest::builder(bad), "{bad}");
	}
}
#[test]
fn locks_are_strict_documents_not_verified_content() {
	let t = Tree::new();
	let doc = lock(&t.0);
	let bytes = doc.encode().unwrap();
	assert_eq!(Lock::decode(&bytes).unwrap(), doc);
	let path = t.0.join("rnx.lock");
	std::fs::write(&path, &bytes).unwrap();
	assert_eq!(Lock::read(&path).unwrap(), doc);
	let mut value = serde_json::to_value(&doc).unwrap();
	value["unknown"] = true.into();
	assert!(Lock::decode(&serde_json::to_vec(&value).unwrap()).is_err());
	let text = String::from_utf8(bytes.clone())
		.unwrap()
		.replacen("{", "{\"format\":1,", 1);
	assert!(Lock::decode(text.as_bytes()).is_err());
	let mut bad = doc.clone();
	bad.format = 2;
	assert!(bad.encode().is_err());
	let mut bad = doc.clone();
	if let Assembly::Generated { main_sha256, .. } = &mut bad.assembly {
		*main_sha256 = "not-a-hash".into();
	}
	assert!(bad.encode().is_err());
	let mut bad = doc.clone();
	bad.assembly = Assembly::Executable {
		path: t.0.join("binary").to_str().unwrap().into(),
		sha256: "a".repeat(64),
	};
	assert!(bad.encode().is_err());
	let mut over = bytes;
	over.resize(input::DOCUMENT_LIMIT, b' ');
	assert!(Lock::decode(&over).is_ok());
	over.push(b' ');
	assert!(Lock::decode(&over).is_err());
	let mut override_doc = doc;
	override_doc.declarations.runtime = None;
	override_doc.declarations.executable = Some(crate::manifest::Location {
		path: "binary".into(),
	});
	override_doc.assembly = bad.assembly;
	assert_eq!(
		Lock::decode(&override_doc.encode().unwrap()).unwrap(),
		override_doc
	);
	let repeated = serde_json::to_string(&override_doc).unwrap().replace(
		"\"sources\":{}",
		"\"sources\":{\"x\":{\"path\":\"one\"},\"x\":{\"path\":\"two\"}}",
	);
	assert!(Lock::decode(repeated.as_bytes()).is_err());
}
#[test]
fn inventory_limits_paths_and_hashes() {
	let t = Tree::new();
	let mut doc = lock(&t.0);
	doc.packages.push(wire::Package {
		root: t.0.to_str().unwrap().into(),
		manifest: t.0.join("Cargo.toml").to_str().unwrap().into(),
		tree_sha256: "d".repeat(64),
		files: vec![wire::File {
			path: "src/lib.rs".into(),
			executable: false,
			bytes: 512 * 1024 * 1024,
			sha256: "e".repeat(64),
		}],
	});
	assert!(doc.validate().is_ok());
	doc.packages[0].files[0].bytes += 1;
	assert!(doc.validate().is_err());
	doc.packages[0].files[0].bytes = 0;
	for bad in ["../x", "a/../b", "/root", "a\\b", "a//b", "./x", ""] {
		doc.packages[0].files[0].path = bad.into();
		assert!(doc.validate().is_err(), "{bad}");
	}
	doc.packages[0].files = (0..100_000)
		.map(|n| wire::File {
			path: format!("f{n}"),
			executable: false,
			bytes: 0,
			sha256: "f".repeat(64),
		})
		.collect();
	assert!(doc.validate().is_ok());
	doc.packages[0].files.push(wire::File {
		path: "extra".into(),
		executable: false,
		bytes: 0,
		sha256: "f".repeat(64),
	});
	assert!(doc.validate().is_err());
}
#[test]
fn declared_table_limits_and_duplicate_json_aliases() {
	let mut doc = app();
	for n in 0..256 {
		doc.sources.insert(
			format!("p{n}"),
			crate::manifest::Location { path: "x".into() },
		);
	}
	assert!(doc.validate().is_ok());
	assert!(Manifest::parse(toml::to_string(&doc).unwrap().as_bytes()).is_ok());
	doc.sources.insert(
		"over".into(),
		crate::manifest::Location { path: "x".into() },
	);
	assert!(doc.validate().is_err());
	assert!(Manifest::parse(toml::to_string(&doc).unwrap().as_bytes()).is_err());
}
#[test]
#[cfg(unix)]
fn capability_refuses_bad_replies_and_retires_timed_out_child() {
	use std::os::unix::fs::PermissionsExt;
	let t = Tree::new();
	let script = t.0.join("executable");
	let set = |body: &str| {
		std::fs::write(&script, format!("#!/bin/sh\n{body}\n")).unwrap();
		std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
	};
	set("test \"$1\" = project-source-version || exit 8\nprintf '%s' '{\"format\":1}'");
	assert!(crate::handshake::check(&script).is_ok());
	set("printf '%s' '{\"format\":1}'; printf '%4084s' ''");
	assert!(crate::handshake::check(&script).is_ok());
	set("printf '%s' '{\"format\":1}'; printf '%4085s' ''");
	assert!(
		crate::handshake::check(&script)
			.unwrap_err()
			.contains("4096")
	);
	for body in [
		"printf '%s' '{\"format\":2}'",
		"printf '%s' '{\"format\":1,\"format\":1}'",
		"printf '%s' '{\"format\":1,\"extra\":true}'",
		"exit 2",
		"printf unexpected >&2",
		"i=0; while test $i -lt 5000; do printf x; i=$((i+1)); done",
		"i=0; while test $i -lt 5000; do printf x >&2; i=$((i+1)); done",
	] {
		set(body);
		assert!(
			crate::handshake::check(&script)
				.unwrap_err()
				.contains("executable")
		);
	}
	let pid = t.0.join("pid");
	set(&format!("echo $$ > '{}'\nexec sleep 30", pid.display()));
	let start = std::time::Instant::now();
	assert!(
		crate::handshake::check(&script)
			.unwrap_err()
			.contains("one second")
	);
	assert!(start.elapsed() < std::time::Duration::from_secs(5));
	let pid: i32 = std::fs::read_to_string(pid)
		.unwrap()
		.trim()
		.parse()
		.unwrap();
	assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
	assert_eq!(
		std::io::Error::last_os_error().raw_os_error(),
		Some(libc::ESRCH)
	);
}

#[test]
#[ignore = "requires an explicitly built project-sources rnx binary"]
fn manifest_to_runner_handoff() {
	let binary = PathBuf::from(
		std::env::var_os("RNX_GATE2_BINARY").expect("RNX_GATE2_BINARY must name the feature build"),
	);
	assert!(binary.is_absolute());
	crate::handshake::check(&binary).unwrap();
	let t = Tree::new();
	let manifest = t.write(
		"app/rnx.toml",
		&format!("{APP}[sources.words]\npath='../words'\n"),
	);
	t.write(
		"app/main.rn",
		"pub mod words; pub fn main(args) { [words::value(), args] }",
	);
	t.write(
		"words/rnx.toml",
		"format=1\n[source]\nroot='src'\n[sources.codec]\npath='../codec'\n",
	);
	t.write(
		"words/src/mod.rn",
		"pub mod codec; pub fn value() { self::codec::value() }",
	);
	t.write("codec/rnx.toml", "format=1\n[source]\nroot='.'\n");
	t.write("codec/mod.rn", "pub fn value() { 42 }");
	let map = Handoff::from_project(&manifest).unwrap();
	let path = t.0.join("map.json");
	std::fs::write(&path, map.encode().unwrap()).unwrap();
	let output = std::process::Command::new(binary)
		.current_dir(std::env::temp_dir())
		.env("TERM", "xterm")
		.env("NO_COLOR", "1")
		.args(["run", "--source-map"])
		.arg(path)
		.arg(&map.entry)
		.arg("sentinel")
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(output.stderr.is_empty());
	assert_eq!(
		String::from_utf8(output.stdout).unwrap(),
		"[42, [\"sentinel\"]]\n"
	);
}

#[test]
fn assembly_stages_without_overwriting_and_verifies_before_launch() {
	let t = Tree::new();
	let manifest = t.write("rnx.toml", APP);
	t.write("main.rn", "pub fn main(_) {42}");
	std::fs::create_dir(t.0.join(".rnx")).unwrap();
	let stage = t.0.join(".rnx/build");
	assert!(crate::assembly::prepare(&manifest, &t.0.join("outside")).is_err());
	crate::assembly::prepare(&manifest, &stage).unwrap();
	let cargo = std::fs::read(stage.join("Cargo.toml")).unwrap();
	assert!(crate::assembly::prepare(&manifest, &stage).is_err());
	assert_eq!(cargo, std::fs::read(stage.join("Cargo.toml")).unwrap());
	let executable = t.write("program", "not an executable");
	let hash = crate::assembly::executable_hash(&executable).unwrap();
	crate::assembly::verify(&executable, &hash).unwrap();
	std::fs::write(&executable, "changed").unwrap();
	let error = crate::assembly::command(
		&executable,
		&hash,
		Some(&t.0.join("missing-map")),
		&t.0.join("main.rn"),
		&[],
	)
	.err()
	.unwrap();
	assert!(error.contains("hash mismatch"), "{error}");
}
