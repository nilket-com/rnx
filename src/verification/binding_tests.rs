use super::*;
struct Fixture {
	dir: PathBuf,
	request: Value,
}
impl Fixture {
	fn new() -> Self {
		use std::sync::atomic::{AtomicUsize, Ordering};
		static NEXT: AtomicUsize = AtomicUsize::new(0);
		let dir = std::env::temp_dir().join(format!(
			"rnx-binding-{}-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos(),
			NEXT.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&dir).unwrap();
		let rnx = dir.join("rnx");
		let rune = dir.join("rune");
		let out = dir.join("out");
		for p in [&rnx, &rune, &out] {
			fs::create_dir(p).unwrap();
		}
		let manifest = dir.join("original.toml");
		let lock = dir.join("original.lock");
		fs::write(&manifest, "original manifest").unwrap();
		fs::write(&lock, "[[package]]\nname='old'\n").unwrap();
		fs::copy(&manifest, rnx.join("Cargo.toml")).unwrap();
		fs::copy(&lock, rnx.join("Cargo.lock")).unwrap();
		fs::copy(&manifest, out.join("Cargo.toml")).unwrap();
		fs::copy(&lock, out.join("Cargo.lock")).unwrap();
		let entry = "#[cfg(all(test, feature = \"cross-verification\"))]\nmod verification;\n#[cfg(all(test, feature = \"cross-verification\"))]\nfn main(){verification::main();}\npub use rune;\n";
		for (path, bytes) in [
			(rnx.join("src/lib.rs"), entry),
			(rnx.join("src/verification/observer.rs"), "actual observer"),
			(rnx.join("tools/verify.py"), "launcher only"),
			(rnx.join("tools/project/Cargo.toml"), "project manifest"),
			(rune.join("crates/rune/src/lib.rs"), "Rune source"),
		] {
			fs::create_dir_all(path.parent().unwrap()).unwrap();
			fs::write(path, bytes).unwrap();
		}
		let request = json!({"rnx_root":rnx,"controller_root":rnx,"rune_root":rune,"rune_path":rune,"rune_rev":null,"registration_fixture":null,"out":out,"original_manifest":manifest,"original_lock":lock});
		Self { dir, request }
	}
	fn path(&self, key: &str) -> PathBuf {
		PathBuf::from(self.request[key].as_str().unwrap())
	}
	fn metadata(&self) -> Value {
		let rune = self.path("rune_root");
		let mut packages = Vec::new();
		for name in PACKAGES {
			let path = rune.join("crates").join(name).join("Cargo.toml");
			fs::create_dir_all(path.parent().unwrap()).unwrap();
			fs::write(&path, name).unwrap();
			packages.push(
				json!({"name":name,"id":name,"version":"0.15.0","source":null,"manifest_path":path}),
			);
		}
		packages.push(json!({"name":"rnx-project","id":"project","manifest_path":self.path("rnx_root").join("tools/project/Cargo.toml")}));
		json!({"packages":packages,"resolve":{"nodes":[{"id":"project","deps":[{"name":"rune","pkg":"rune"}]}]}})
	}
	fn artifact(&self) -> (PathBuf, Value, String) {
		let out = self.path("out");
		fs::write(
			out.join("Cargo.toml"),
			format!("temporary manifest\n{DEV_TARGET}"),
		)
		.unwrap();
		let binary = out.join("target/debug/deps/rnx-fixture");
		fs::create_dir_all(binary.parent().unwrap()).unwrap();
		fs::write(&binary, "bound executable bytes").unwrap();
		let mut metadata = self.metadata();
		metadata["packages"].as_array_mut().unwrap().push(
			json!({"name":"rnx","id":"root-rnx","manifest_path":self.path("rnx_root").join("Cargo.toml")}),
		);
		metadata["resolve"]["root"] = json!("root-rnx");
		metadata["resolve"]["nodes"]
			.as_array_mut()
			.unwrap()
			.push(json!({"id":"root-rnx","features":["cross-verification"]}));
		self.save_metadata(&metadata);
		let record = json!({"reason":"compiler-artifact","package_id":"root-rnx","manifest_path":self.path("rnx_root").join("Cargo.toml"),"target":{"kind":["test"],"crate_types":["bin"],"name":"rnx","src_path":self.path("rnx_root").join("src/lib.rs"),"required-features":["cross-verification"],"test":true},"profile":{"opt_level":"0","debuginfo":2,"debug_assertions":true,"overflow_checks":true,"test":true},"features":["cross-verification"],"filenames":[binary],"executable":binary,"fresh":false});
		let stream = format!(
			"{}\n{}\n",
			record,
			json!({"reason":"build-finished","success":true})
		);
		fs::write(out.join("cargo.stdout"), &stream).unwrap();
		(binary, record, stream)
	}

	fn save_metadata(&self, value: &Value) {
		fs::write(
			self.path("out").join("metadata.json"),
			serde_json::to_vec(value).unwrap(),
		)
		.unwrap();
	}
}
impl Drop for Fixture {
	fn drop(&mut self) {
		fs::remove_dir_all(&self.dir).unwrap();
	}
}
#[test]
fn wrong_rune_identity_duplicate_and_project_edge_are_rejected() {
	let f = Fixture::new();
	let valid = f.metadata();
	f.save_metadata(&valid);
	assert!(dependencies(&f.request).is_ok());
	let mut duplicate = valid.clone();
	let duplicated_package = duplicate["packages"][0].clone();
	duplicate["packages"]
		.as_array_mut()
		.unwrap()
		.push(duplicated_package);
	f.save_metadata(&duplicate);
	assert!(dependencies(&f.request).is_err());
	let wrong = f.dir.join("other/Cargo.toml");
	fs::create_dir(wrong.parent().unwrap()).unwrap();
	fs::write(&wrong, "different Rune").unwrap();
	let mut changed = valid.clone();
	changed["packages"][0]["manifest_path"] = json!(wrong);
	f.save_metadata(&changed);
	assert!(dependencies(&f.request).is_err());
	let mut old_git = valid.clone();
	old_git["packages"][0]["source"] = json!("git+old-rev");
	f.save_metadata(&old_git);
	assert!(dependencies(&f.request).is_err());
	let mut edge = valid;
	edge["resolve"]["nodes"][0]["deps"][0]["pkg"] = json!("second-rune");
	f.save_metadata(&edge);
	assert!(dependencies(&f.request).is_err());
}
#[test]
fn missing_or_wrong_dwarf_root_cannot_pass() {
	let f = Fixture::new();
	let rnx = f.path("rnx_root");
	let rune = f.path("rune_root");
	let units = format!(
		"Compilation Unit @ offset 0:\nDW_AT_name : src/lib.rs/@/abc\nDW_AT_comp_dir : {}\nCompilation Unit @ offset 1:\nDW_AT_name : crates/rune/src/lib.rs/@/def\nDW_AT_comp_dir : {}\n",
		rnx.display(),
		rune.display()
	);
	assert!(dwarf_text(&units, &rnx, &rune).is_ok());
	assert!(dwarf_text("", &rnx, &rune).is_err());
	assert!(
		dwarf_text(
			units.split("Compilation Unit @ offset 1:").next().unwrap(),
			&rnx,
			&rune
		)
		.is_err()
	);
	assert!(
		dwarf_text(
			&units.replace(&rnx.display().to_string(), &rune.display().to_string()),
			&rnx,
			&rune
		)
		.is_err()
	);
	#[cfg(unix)]
	assert!(dwarf(Path::new("/bin/true"), &rnx, &rune).is_err());
}
#[test]
fn binary_bytes_and_normal_host_files_are_not_trusted_by_path() {
	let f = Fixture::new();
	let binary = f.dir.join("child");
	fs::write(&binary, "original executable bytes").unwrap();
	let digest = hash(&binary).unwrap();
	assert!(same_hash(&binary, &digest).is_ok());
	fs::write(&binary, "different executable bytes").unwrap();
	assert!(same_hash(&binary, &digest).is_err());
	assert!(host_check(&f.request).is_err());
	assert!(f.path("out").join("HOST-CHECK.json").exists());
	assert!(!f.path("out").join("COMPLETE.json").exists());
	fs::write(
		f.path("rnx_root").join("Cargo.toml"),
		"changed host manifest",
	)
	.unwrap();
	assert!(
		host_check(&f.request)
			.unwrap_err()
			.to_string()
			.contains("normal host Cargo.toml changed")
	);
}
#[test]
fn private_overlay_and_lock_drift_are_rejected() {
	let f = Fixture::new();
	let before = overlays(&f.request).unwrap();
	fs::write(
		f.path("out").join("Cargo.lock"),
		"[[package]]\nname='new'\n",
	)
	.unwrap();
	assert_ne!(overlays(&f.request).unwrap(), before);
	fs::copy(
		f.path("out").join("Cargo.lock"),
		f.path("rnx_root").join("Cargo.lock"),
	)
	.unwrap();
	assert!(overlays(&f.request).is_err());
}
#[test]
fn compile_pins_reject_stale_sources_missing_rows_and_duplicates() {
	let f = Fixture::new();
	let rnx = source(&f.path("rnx_root"), false, &f.request).unwrap();
	let rune = source(&f.path("rune_root"), false, &f.request).unwrap();
	let mut text = String::new();
	for (kind, snapshot) in [("rnx", &rnx), ("rune", &rune)] {
		for (name, digest) in snapshot["files"].as_object().unwrap() {
			text.push_str(&format!("{kind}\t{}\t{name}\n", digest.as_str().unwrap()));
		}
	}
	assert!(compiled(&f.request, &text, &rnx, &rune).is_ok());
	let duplicate = format!("{text}{}\n", text.lines().next().unwrap());
	assert!(compiled(&f.request, &duplicate, &rnx, &rune).is_err());
	let missing = text.lines().skip(1).collect::<Vec<_>>().join("\n");
	assert!(compiled(&f.request, &missing, &rnx, &rune).is_err());
	fs::write(
		f.path("rune_root").join("crates/rune/src/lib.rs"),
		"edited since compilation",
	)
	.unwrap();
	let changed = source(&f.path("rune_root"), false, &f.request).unwrap();
	assert!(compiled(&f.request, &text, &rnx, &changed).is_err());
	assert!(compiled(&f.request, &text.replace("rune\t", "other\t"), &rnx, &rune).is_err());
}
#[test]
fn explicit_control_needs_matching_root_and_identical_observers() {
	let f = Fixture::new();
	assert_eq!(registration(&f.request).unwrap()["label"], "actual");
	let mut wrong = f.request.clone();
	wrong["registration_fixture"] = json!(f.path("rune_root"));
	assert!(registration(&wrong).is_err());
	let mut explicit = f.request.clone();
	explicit["registration_fixture"] = explicit["rnx_root"].clone();
	assert_eq!(
		registration(&explicit).unwrap()["label"],
		"explicit registration-fixture control"
	);
	let other = Fixture::new();
	explicit["controller_root"] = other.request["rnx_root"].clone();
	assert!(registration(&explicit).is_ok());
	fs::write(
		other.path("rnx_root").join("src/verification/observer.rs"),
		"changed classifier",
	)
	.unwrap();
	assert!(registration(&explicit).is_err());
	fs::remove_file(other.path("rnx_root").join("src/verification/observer.rs")).unwrap();
	assert!(registration(&explicit).is_err());
	fs::write(
		other.path("rnx_root").join("src/verification/observer.rs"),
		"actual observer",
	)
	.unwrap();
	fs::write(
		other.path("rnx_root").join("src/lib.rs"),
		"fn main(){bypass_controller();}",
	)
	.unwrap();
	assert!(registration(&explicit).is_err());
	fs::copy(
		f.path("rnx_root").join("src/lib.rs"),
		other.path("rnx_root").join("src/lib.rs"),
	)
	.unwrap();
	fs::write(
		other.path("rnx_root").join("tools/verify.py"),
		"different launcher",
	)
	.unwrap();
	assert!(registration(&explicit).is_err());
}
#[test]
fn wrong_git_export_and_untracked_source_drift_are_detected() {
	let f = Fixture::new();
	let repo = f.dir.join("git-source");
	fs::create_dir(&repo).unwrap();
	run(Command::new("git")
		.arg("-C")
		.arg(&repo)
		.args(["init", "-q"]))
	.unwrap();
	fs::write(repo.join("source.rs"), "frozen").unwrap();
	run(Command::new("git")
		.arg("-C")
		.arg(&repo)
		.args(["add", "source.rs"]))
	.unwrap();
	run(Command::new("git").arg("-C").arg(&repo).args([
		"-c",
		"user.name=Verification",
		"-c",
		"user.email=verify@example.invalid",
		"-c",
		"commit.gpgsign=false",
		"-c",
		"core.hooksPath=/dev/null",
		"commit",
		"-qm",
		"fixture",
	]))
	.unwrap();
	fs::write(repo.join("untracked-policy.json"), "first").unwrap();
	let before = source(&repo, false, &f.request).unwrap();
	fs::write(repo.join("untracked-policy.json"), "changed").unwrap();
	assert_ne!(source(&repo, false, &f.request).unwrap(), before);
	let rev = String::from_utf8(
		run(Command::new("git")
			.arg("-C")
			.arg(&repo)
			.args(["rev-parse", "HEAD"]))
		.unwrap(),
	)
	.unwrap();
	let archive = f.dir.join("source.tar");
	fs::write(
		&archive,
		run(Command::new("git")
			.arg("-C")
			.arg(&repo)
			.args(["archive", "HEAD"]))
		.unwrap(),
	)
	.unwrap();
	let export = f.dir.join("export");
	fs::create_dir(&export).unwrap();
	run(Command::new("tar")
		.arg("-xf")
		.arg(&archive)
		.arg("-C")
		.arg(&export))
	.unwrap();
	let mut request = f.request.clone();
	request["rune_rev"] = json!("HEAD");
	request["rune_revision_resolved"] = json!(rev.trim());
	request["rune_repo"] = json!(repo);
	request["rune_archive"] = json!(archive);
	request["rune_root"] = json!(export);
	assert!(revision(&request).is_ok());
	let second = f.dir.join("second-out");
	fs::create_dir(&second).unwrap();
	request["out"] = json!(second);
	fs::write(export.join("source.rs"), "tampered export").unwrap();
	assert!(revision(&request).is_err());
}
#[cfg(unix)]
#[test]
fn source_symlinks_are_rejected() {
	let f = Fixture::new();
	std::os::unix::fs::symlink("/etc/passwd", f.path("rune_root").join("outside.rs")).unwrap();
	assert!(source(&f.path("rune_root"), false, &f.request).is_err());
}

#[test]
fn complete_requires_host_sources_binary_and_bound_suite_pass() {
	for mode in [
		"valid",
		"missing-result",
		"failed-result",
		"changed-rnx",
		"changed-rune",
		"changed-binary",
	] {
		let f = Fixture::new();
		let root = f.path("rnx_root");
		run(Command::new("git")
			.arg("-C")
			.arg(&root)
			.args(["init", "-q"]))
		.unwrap();
		run(Command::new("git")
			.arg("-C")
			.arg(&root)
			.args(["add", "src/lib.rs"]))
		.unwrap();
		run(Command::new("git").arg("-C").arg(&root).args([
			"-c",
			"user.name=Verification",
			"-c",
			"user.email=verify@example.invalid",
			"-c",
			"commit.gpgsign=false",
			"-c",
			"core.hooksPath=/dev/null",
			"commit",
			"-qm",
			"fixture",
		]))
		.unwrap();
		let out = f.path("out");
		let (executable, _, _) = f.artifact();
		let binary = out.join("copied-child");
		fs::copy(&executable, &binary).unwrap();
		let before = json!({"artifact":artifact(&f.request,&executable).unwrap(),"executable":executable,"overlays":overlays(&f.request).unwrap(),"dependencies":dependencies(&f.request).unwrap(),"rnx_source":source(&root,true,&f.request).unwrap(),"rune_source":source(&f.path("rune_root"),false,&f.request).unwrap(),"binary_copy":binary,"executable_sha256":hash(&binary).unwrap()});
		write(&out.join("BINDING-BEFORE.json"), &before).unwrap();
		write(&out.join("BINDING-AFTER.json"), &json!({"status":"PASS"})).unwrap();
		if mode != "missing-result" {
			write(
				&out.join("RESULT.json"),
				&json!({"status":if mode=="failed-result"{"FAIL"}else{"PASS"},"binding":before}),
			)
			.unwrap();
		}
		match mode {
			"changed-rnx" => fs::write(root.join("src/lib.rs"), "edited after controller").unwrap(),
			"changed-rune" => fs::write(
				f.path("rune_root").join("crates/rune/src/lib.rs"),
				"edited Rune",
			)
			.unwrap(),
			"changed-binary" => fs::write(binary, "replacement executable").unwrap(),
			_ => {}
		}
		let result = host_check(&f.request);
		if mode == "valid" {
			assert!(result.is_ok());
			assert_eq!(read(&out.join("COMPLETE.json")).unwrap()["status"], "PASS");
		} else {
			assert!(result.is_err(), "accepted {mode}");
			assert!(!out.join("COMPLETE.json").exists());
		}
		if mode == "missing-result" || mode == "failed-result" {
			assert_eq!(
				read(&out.join("HOST-CHECK.json")).unwrap()["status"],
				"PASS"
			);
		}
	}
}

#[test]
fn cargo_artifact_stream_rejects_missing_duplicate_malformed_wrong_and_stale() {
	let f = Fixture::new();
	let (executable, record, stream) = f.artifact();
	assert!(artifact_text(&f.request, &executable, &stream).is_ok());
	let finish = json!({"reason":"build-finished","success":true});
	for bad in [
		format!("{finish}\n"),
		format!("{record}\n{record}\n{finish}\n"),
		format!("malformed\n{record}\n{finish}\n"),
		format!(
			"{record}\n{}\n",
			json!({"reason":"build-finished","success":false})
		),
	] {
		assert!(artifact_text(&f.request, &executable, &bad).is_err());
	}
	for (field, value) in [
		("package_id", json!("wrong-package")),
		(
			"features",
			json!(["cross-verification", "unexpected-feature"]),
		),
	] {
		let mut wrong = record.clone();
		wrong[field] = value;
		assert!(artifact_text(&f.request, &executable, &format!("{wrong}\n{finish}\n")).is_err());
	}
	for (field, value) in [
		("name", json!("wrong-target")),
		(
			"src_path",
			json!(f.path("rune_root").join("crates/rune/src/lib.rs")),
		),
		("required-features", json!([])),
	] {
		let mut wrong = record.clone();
		wrong["target"][field] = value;
		assert!(artifact_text(&f.request, &executable, &format!("{wrong}\n{finish}\n")).is_err());
	}
	let mut wrong = record.clone();
	wrong["profile"]["test"] = json!(false);
	assert!(artifact_text(&f.request, &executable, &format!("{wrong}\n{finish}\n")).is_err());
	let stale = f.path("out").join("target/debug/deps/stale");
	fs::write(&stale, "old binary").unwrap();
	let mut wrong = record.clone();
	wrong["executable"] = json!(stale);
	wrong["filenames"] = json!([stale]);
	assert!(artifact_text(&f.request, &executable, &format!("{wrong}\n{finish}\n")).is_err());
	let before = artifact_text(&f.request, &executable, &stream).unwrap();
	fs::write(&executable, "replacement binary at same path").unwrap();
	assert_ne!(
		artifact_text(&f.request, &executable, &stream).unwrap(),
		before
	);
	fs::write(
		f.path("out").join("Cargo.toml"),
		DEV_TARGET.replace("harness = false", "harness = true"),
	)
	.unwrap();
	assert!(artifact_text(&f.request, &executable, &stream).is_err());
}
#[test]
fn compiled_cargo_pins_bind_private_files_while_normal_host_stays_original() {
	let f = Fixture::new();
	let _ = f.artifact();
	let rnx = source(&f.path("rnx_root"), false, &f.request).unwrap();
	let rune = source(&f.path("rune_root"), false, &f.request).unwrap();
	let mut text = String::new();
	for (kind, snapshot) in [("rnx", &rnx), ("rune", &rune)] {
		for (name, digest) in snapshot["files"].as_object().unwrap() {
			let digest = if kind == "rnx" && (name == "Cargo.toml" || name == "Cargo.lock") {
				hash(&f.path("out").join(name)).unwrap()
			} else {
				digest.as_str().unwrap().to_owned()
			};
			text.push_str(&format!("{kind}\t{digest}\t{name}\n"));
		}
	}
	assert_ne!(
		hash(&f.path("rnx_root").join("Cargo.toml")).unwrap(),
		hash(&f.path("out").join("Cargo.toml")).unwrap()
	);
	assert!(compiled(&f.request, &text, &rnx, &rune).is_ok());
	assert!(overlays(&f.request).is_ok());
	fs::write(f.path("out").join("Cargo.toml"), "different private graph").unwrap();
	assert!(compiled(&f.request, &text, &rnx, &rune).is_err());
}
