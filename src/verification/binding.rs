//! Source, dependency and executable binding for the explicit dev controller.
use serde_json::{Value, json};
use std::{
	collections::BTreeMap,
	fs::{self, OpenOptions},
	io::Write,
	path::{Path, PathBuf},
	process::{Command, Stdio},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const PACKAGES: &[&str] = &[
	"rune",
	"rune-core",
	"rune-alloc",
	"rune-alloc-macros",
	"rune-macros",
	"rune-tracing",
	"rune-tracing-macros",
];
fn need(ok: bool, message: &str) -> Result<()> {
	if !ok {
		return Err(message.into());
	}
	Ok(())
}
fn field<'a>(r: &'a Value, key: &str) -> Result<&'a str> {
	r[key]
		.as_str()
		.ok_or_else(|| format!("missing request {key}").into())
}
fn run(command: &mut Command) -> Result<Vec<u8>> {
	let output = command.output()?;
	need(
		output.status.success(),
		&format!(
			"command {:?} failed: {}",
			command,
			String::from_utf8_lossy(&output.stderr)
		),
	)?;
	Ok(output.stdout)
}
fn hash(path: &Path) -> Result<String> {
	let output = run(Command::new("sha256sum").arg("--").arg(path))?;
	let text = std::str::from_utf8(&output)?;
	let digest = text.get(..64).ok_or("malformed sha256sum output")?;
	need(
		digest.bytes().all(|c| c.is_ascii_hexdigit()),
		"malformed SHA256",
	)?;
	Ok(digest.into())
}
fn hash_bytes(bytes: &[u8]) -> Result<String> {
	let mut child = Command::new("sha256sum")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()?;
	child
		.stdin
		.take()
		.ok_or("missing hash stdin")?
		.write_all(bytes)?;
	let output = child.wait_with_output()?;
	need(output.status.success(), "hash process failed")?;
	let text = std::str::from_utf8(&output.stdout)?;
	let digest = text.get(..64).ok_or("missing byte digest")?;
	need(
		digest.bytes().all(|b| b.is_ascii_hexdigit()),
		"invalid byte digest",
	)?;
	Ok(digest.into())
}

fn write(path: &Path, value: &Value) -> Result<()> {
	let mut f = OpenOptions::new().write(true).create_new(true).open(path)?;
	serde_json::to_writer_pretty(&mut f, value)?;
	f.write_all(b"\n")?;
	Ok(())
}
fn read(path: &Path) -> Result<Value> {
	Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn no_link(path: &Path) -> Result<()> {
	need(
		!fs::symlink_metadata(path)?.file_type().is_symlink(),
		&format!("source symlink is not permitted: {}", path.display()),
	)
}
fn walk(root: &Path, dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
	for entry in fs::read_dir(dir)? {
		let p = entry?.path();
		let name = p.file_name().ok_or("missing source filename")?;
		if name == ".git" || name == "target" {
			continue;
		}
		no_link(&p)?;
		if p.is_dir() {
			walk(root, &p, files)?;
		} else if p.is_file() {
			files.push(p.strip_prefix(root)?.to_owned());
		} else {
			return Err("unsupported source file kind".into());
		}
	}
	Ok(())
}
fn source(root: &Path, rnx: bool, request: &Value) -> Result<Value> {
	let mut names = Vec::new();
	if rnx || root.join(".git").exists() {
		let raw = run(Command::new("git").arg("-C").arg(root).args([
			"ls-files",
			"--cached",
			"--others",
			"--exclude-standard",
			"-z",
		]))?;
		for name in raw.split(|b| *b == 0).filter(|b| !b.is_empty()) {
			names.push(PathBuf::from(std::str::from_utf8(name)?));
		}
		if root.join("Cargo.lock").is_file() && !names.iter().any(|n| n == Path::new("Cargo.lock"))
		{
			names.push("Cargo.lock".into());
		}
	} else {
		walk(root, root, &mut names)?;
	}
	let mut files = BTreeMap::new();
	for name in names {
		need(
			!name.is_absolute()
				&& name
					.components()
					.all(|c| matches!(c, std::path::Component::Normal(_))),
			"invalid source path",
		)?;
		let actual = if rnx && name == Path::new("Cargo.toml") {
			PathBuf::from(field(request, "original_manifest")?)
		} else if rnx && name == Path::new("Cargo.lock") {
			PathBuf::from(field(request, "original_lock")?)
		} else {
			root.join(&name)
		};
		no_link(&actual)?;
		need(actual.is_file(), "source disappeared or is not regular")?;
		files.insert(
			name.to_str().ok_or("non-UTF8 source path")?.to_owned(),
			hash(&actual)?,
		);
	}
	need(!files.is_empty(), "empty source snapshot")?;
	let head = if root.join(".git").exists() {
		Some(
			String::from_utf8(run(Command::new("git")
				.arg("-C")
				.arg(root)
				.args(["rev-parse", "HEAD"]))?)?
			.trim()
			.to_owned(),
		)
	} else {
		None
	};
	Ok(json!({"root":root,"head":head,"files":files}))
}
fn registration(request: &Value) -> Result<Value> {
	let rnx = fs::canonicalize(field(request, "rnx_root")?)?;
	let controller = fs::canonicalize(field(request, "controller_root")?)?;
	let label = if request["registration_fixture"].is_null() {
		need(rnx == controller, "unlabeled alternative registration root")?;
		"actual"
	} else {
		need(
			fs::canonicalize(field(request, "registration_fixture")?)? == rnx,
			"registration fixture differs from compiled Rnx root",
		)?;
		"explicit registration-fixture control"
	};
	let wiring = |root: &Path| -> Result<Value> {
		let dir = root.join("src/verification");
		let mut names = Vec::new();
		walk(&dir, &dir, &mut names)?;
		need(!names.is_empty(), "missing checked-in verifier sources")?;
		let mut files = BTreeMap::new();
		for name in names {
			files.insert(
				name.to_str().ok_or("non-UTF8 verifier path")?.to_owned(),
				hash(&dir.join(name))?,
			);
		}
		let lib = fs::read_to_string(root.join("src/lib.rs"))?;
		let start = lib
			.find("#[cfg(all(test, feature = \"cross-verification\"))]")
			.ok_or("missing cfgtest verifier entry")?;
		let end = lib[start..]
			.find("pub use rune;")
			.ok_or("missing verifier root boundary")?
			+ start;
		no_link(&root.join("tools/verify.py"))?;
		Ok(
			json!({"files":files,"launcher_sha256":hash(&root.join("tools/verify.py"))?,"entry":&lib[start..end]}),
		)
	};
	let actual = wiring(&rnx)?;
	need(
		actual == wiring(&controller)?,
		"control changed checked-in observer/controller wiring",
	)?;
	Ok(json!({"label":label,"controller_root":controller,"rnx_root":rnx,"wiring":actual}))
}

fn dependencies(request: &Value) -> Result<Value> {
	let out = Path::new(field(request, "out")?);
	let root = fs::canonicalize(field(request, "rune_root")?)?;
	let metadata = read(&out.join("metadata.json"))?;
	let packages = metadata["packages"]
		.as_array()
		.ok_or("missing Cargo packages")?;
	let mut observed = Vec::new();
	for name in PACKAGES {
		let selected = packages
			.iter()
			.filter(|p| p["name"] == *name)
			.collect::<Vec<_>>();
		need(
			selected.len() == 1,
			&format!("expected exactly one {name} package"),
		)?;
		let p = selected[0];
		let manifest = fs::canonicalize(
			p["manifest_path"]
				.as_str()
				.ok_or("missing package manifest")?,
		)?;
		need(
			manifest == root.join("crates").join(name).join("Cargo.toml"),
			&format!("wrong Rune source: {name}"),
		)?;
		need(
			p["source"].is_null(),
			"a Rune git/registry source survived path override",
		)?;
		observed.push(json!({"name":name,"id":p["id"],"version":p["version"],"manifest":manifest,"manifest_sha256":hash(&manifest)?}));
	}
	let project = packages
		.iter()
		.filter(|p| p["name"] == "rnx-project")
		.collect::<Vec<_>>();
	need(
		project.len() == 1,
		"compiler-only rnx-project package missing or duplicated",
	)?;
	need(
		fs::canonicalize(
			project[0]["manifest_path"]
				.as_str()
				.ok_or("missing project manifest")?,
		)? == Path::new(field(request, "rnx_root")?).join("tools/project/Cargo.toml"),
		"wrong compiler-only project source",
	)?;
	let resolve = metadata["resolve"]["nodes"]
		.as_array()
		.ok_or("missing Cargo resolve graph")?;
	let project_node = resolve
		.iter()
		.find(|n| n["id"] == project[0]["id"])
		.ok_or("project resolve node missing")?;
	let rune_id = &observed[0]["id"];
	need(
		project_node["deps"]
			.as_array()
			.ok_or("missing project dependencies")?
			.iter()
			.any(|d| d["name"] == "rune" && d["pkg"] == *rune_id),
		"project retained a second Rune identity",
	)?;
	Ok(
		json!({"packages":observed,"project":project[0]["id"],"metadata_sha256":hash(&out.join("metadata.json"))?,"resolve":metadata["resolve"]}),
	)
}
fn compiled(request: &Value, text: &str, rnx_source: &Value, rune_source: &Value) -> Result<Value> {
	let mut expected = BTreeMap::new();
	for (kind, source) in [("rnx", rnx_source), ("rune", rune_source)] {
		let files = source["files"]
			.as_object()
			.ok_or("missing runtime source files")?;
		for (path, digest) in files {
			expected.insert(
				(kind.to_owned(), path.clone()),
				digest.as_str().ok_or("invalid runtime digest")?.to_owned(),
			);
		}
	}
	for name in ["Cargo.toml", "Cargo.lock"] {
		expected.insert(
			("rnx".to_owned(), name.to_owned()),
			hash(&Path::new(field(request, "out")?).join(name))?,
		);
	}
	let mut pinned = BTreeMap::new();
	for line in text.lines() {
		let fields = line.split('\t').collect::<Vec<_>>();
		need(fields.len() == 3, "malformed compiled-source pin")?;
		let (kind, digest, name) = (fields[0], fields[1], fields[2]);
		need(
			kind == "rnx" || kind == "rune",
			"unknown compiled-source kind",
		)?;
		need(
			digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
			"invalid compiled-source SHA256",
		)?;
		let path = Path::new(name);
		need(
			!name.is_empty()
				&& !path.is_absolute()
				&& path
					.components()
					.all(|c| matches!(c, std::path::Component::Normal(_))),
			"invalid compiled-source path",
		)?;
		need(
			pinned
				.insert((kind.to_owned(), name.to_owned()), digest.to_owned())
				.is_none(),
			"duplicate compiled-source pin",
		)?;
	}
	need(
		!pinned.is_empty() && pinned == expected,
		"stale or incomplete compiled-source identity",
	)?;
	Ok(
		json!({"status":"PASS","rnx_files":pinned.keys().filter(|(k,_)|k=="rnx").count(),"rune_files":pinned.keys().filter(|(k,_)|k=="rune").count(),"stream_sha256":hash_bytes(text.as_bytes())?}),
	)
}

fn same_hash(path: &Path, expected: &str) -> Result<()> {
	need(hash(path)? == expected, "binary/source digest mismatch")
}

fn dwarf(binary: &Path, rnx: &Path, rune: &Path) -> Result<Value> {
	let raw = run(Command::new("readelf")
		.args(["--debug-dump=info", "--dwarf-depth=1"])
		.arg(binary))?;
	let text = std::str::from_utf8(&raw)?;
	dwarf_text(text, rnx, rune)
}
fn dwarf_text(text: &str, rnx: &Path, rune: &Path) -> Result<Value> {
	let mut rnx_units = Vec::new();
	let mut rune_units = Vec::new();
	for block in text.split("Compilation Unit @ offset") {
		let name = block.lines().find(|l| l.contains("DW_AT_name"));
		let dir = block.lines().find(|l| l.contains("DW_AT_comp_dir"));
		let (Some(name), Some(dir)) = (name, dir) else {
			continue;
		};
		let value =
			|line: &str| -> String { line.rsplit(": ").next().unwrap_or("").trim().to_owned() };
		let name = value(name);
		let dir = value(dir);
		let source = name.split("/@/").next().ok_or("bad DWARF source")?;
		let path = Path::new(source);
		let actual = if path.is_absolute() {
			path.to_owned()
		} else {
			Path::new(&dir).join(path)
		};
		let Ok(actual) = fs::canonicalize(actual) else {
			continue;
		};
		if actual == rnx.join("src/lib.rs") {
			rnx_units.push(json!({"source":actual,"comp_dir":dir}));
		}
		if actual == rune.join("crates/rune/src/lib.rs") {
			rune_units.push(json!({"source":actual,"comp_dir":dir}));
		}
	}
	need(
		!rnx_units.is_empty(),
		"executable lacks real Rnx root DWARF",
	)?;
	need(
		!rune_units.is_empty(),
		"executable lacks requested Rune root DWARF",
	)?;
	Ok(json!({"rnx":rnx_units,"rune":rune_units}))
}
fn overlays(request: &Value) -> Result<Value> {
	let out = Path::new(field(request, "out")?);
	let rnx = Path::new(field(request, "rnx_root")?);
	need(
		hash(&rnx.join("Cargo.toml"))? == hash(Path::new(field(request, "original_manifest")?))?,
		"normal host manifest changed",
	)?;
	need(
		hash(&rnx.join("Cargo.lock"))? == hash(Path::new(field(request, "original_lock")?))?,
		"normal host lock changed",
	)?;
	let old = fs::read_to_string(field(request, "original_lock")?)?;
	let new = fs::read_to_string(out.join("Cargo.lock"))?;
	let sections = |s: &str| {
		s.split("[[package]]")
			.skip(1)
			.map(|p| p.trim().to_owned())
			.collect::<std::collections::BTreeSet<_>>()
	};
	let old_sections = sections(&old);
	let new_sections = sections(&new);
	Ok(
		json!({"manifest_sha256":hash(&out.join("Cargo.toml"))?,"lock_sha256":hash(&out.join("Cargo.lock"))?,"original_manifest_sha256":hash(Path::new(field(request,"original_manifest")?))?,"original_lock_sha256":hash(Path::new(field(request,"original_lock")?))?,"lock_removed_package_records":old_sections.difference(&new_sections).collect::<Vec<_>>(),"lock_added_package_records":new_sections.difference(&old_sections).collect::<Vec<_>>() }),
	)
}
const DEV_TARGET: &str = "[[test]]\nname = \"rnx\"\npath = \"src/lib.rs\"\nharness = false\nrequired-features = [\"cross-verification\"]\n";
fn artifact_text(request: &Value, executable: &Path, text: &str) -> Result<Value> {
	need(cfg!(test), "controller is not cfg(test)")?;
	let out = Path::new(field(request, "out")?);
	let root = fs::canonicalize(field(request, "rnx_root")?)?;
	let manifest = fs::read_to_string(out.join("Cargo.toml"))?;
	need(
		manifest.matches("[[test]]").count() == 1 && manifest.contains(DEV_TARGET),
		"temporary dev target must be exact harness=false root target",
	)?;
	let metadata = read(&out.join("metadata.json"))?;
	let packages = metadata["packages"]
		.as_array()
		.ok_or("missing artifact package metadata")?;
	let roots = packages
		.iter()
		.filter(|p| p["name"] == "rnx")
		.collect::<Vec<_>>();
	need(roots.len() == 1, "missing or duplicate Rnx package")?;
	let package = roots[0];
	need(
		fs::canonicalize(
			package["manifest_path"]
				.as_str()
				.ok_or("missing Rnx package manifest")?,
		)? == root.join("Cargo.toml"),
		"wrong artifact package root",
	)?;
	need(
		metadata["resolve"]["root"] == package["id"],
		"wrong resolved root package",
	)?;
	let nodes = metadata["resolve"]["nodes"]
		.as_array()
		.ok_or("missing artifact feature graph")?;
	let selected = nodes
		.iter()
		.filter(|node| node["id"] == package["id"])
		.collect::<Vec<_>>();
	need(
		selected.len() == 1,
		"missing or duplicate root feature node",
	)?;
	let features = |value: &Value| -> Result<std::collections::BTreeSet<String>> {
		let a = value.as_array().ok_or("missing feature array")?;
		let set = a
			.iter()
			.map(|v| Ok(v.as_str().ok_or("invalid feature")?.to_owned()))
			.collect::<Result<std::collections::BTreeSet<_>>>()?;
		need(a.len() == set.len(), "duplicate features")?;
		Ok(set)
	};
	let expected = features(&selected[0]["features"])?;
	need(
		expected.contains("cross-verification"),
		"verification feature not resolved",
	)?;
	let records = text
		.lines()
		.filter(|l| !l.trim().is_empty())
		.map(serde_json::from_str::<Value>)
		.collect::<std::result::Result<Vec<_>, _>>()?;
	need(
		records
			.last()
			.is_some_and(|r| r["reason"] == "build-finished" && r["success"] == true),
		"Cargo build did not finish successfully",
	)?;
	need(
		records
			.iter()
			.filter(|r| r["reason"] == "build-finished")
			.count() == 1,
		"multiple Cargo build completions",
	)?;
	let matches = records
		.iter()
		.filter(|r| {
			r["reason"] == "compiler-artifact"
				&& r["package_id"] == package["id"]
				&& r["target"]["name"] == "rnx"
				&& r["target"]["kind"] == json!(["test"])
		})
		.collect::<Vec<_>>();
	need(
		matches.len() == 1,
		"expected exactly one matching dev compiler artifact",
	)?;
	let record = matches[0];
	need(
		record["profile"]["test"] == true && record["target"]["crate_types"] == json!(["bin"]),
		"artifact is not a test executable",
	)?;
	need(
		record["target"]["required-features"] == json!(["cross-verification"]),
		"wrong target feature requirement",
	)?;
	need(
		features(&record["features"])? == expected,
		"artifact features differ from resolved graph",
	)?;
	need(
		fs::canonicalize(
			record["manifest_path"]
				.as_str()
				.ok_or("missing artifact manifest")?,
		)? == root.join("Cargo.toml"),
		"artifact manifest source differs",
	)?;
	need(
		fs::canonicalize(
			record["target"]["src_path"]
				.as_str()
				.ok_or("missing artifact source")?,
		)? == root.join("src/lib.rs"),
		"artifact does not compile actual library root",
	)?;
	let path = fs::canonicalize(
		record["executable"]
			.as_str()
			.ok_or("missing artifact executable")?,
	)?;
	need(
		path == fs::canonicalize(executable)?
			&& path.starts_with(fs::canonicalize(out.join("target"))?),
		"stale or wrong launched executable",
	)?;
	let files = record["filenames"]
		.as_array()
		.ok_or("missing artifact filenames")?;
	need(
		files.iter().filter(|v| v.as_str() == path.to_str()).count() == 1,
		"executable absent or duplicated in artifact filenames",
	)?;
	Ok(
		json!({"record":record,"stream_sha256":hash_bytes(text.as_bytes())?,"profile_sha256":hash_bytes(&serde_json::to_vec(&record["profile"])?)?,"executable_sha256":hash(&path)?,"cfg_test":cfg!(test),"harness":false,"private_manifest_sha256":hash(&out.join("Cargo.toml"))?}),
	)
}
fn artifact(request: &Value, executable: &Path) -> Result<Value> {
	artifact_text(
		request,
		executable,
		&fs::read_to_string(Path::new(field(request, "out")?).join("cargo.stdout"))?,
	)
}

fn revision(request: &Value) -> Result<Value> {
	if request["rune_rev"].is_null() {
		need(
			fs::canonicalize(field(request, "rune_path")?)?
				== fs::canonicalize(field(request, "rune_root")?)?,
			"requested Rune path differs from selected source",
		)?;
		return Ok(json!({"mode":"path","requested":request["rune_path"]}));
	}
	let repo = field(request, "rune_repo")?;
	let revision = field(request, "rune_revision_resolved")?;
	let actual = run(Command::new("git").args([
		"-C",
		repo,
		"rev-parse",
		&format!("{}^{{commit}}", field(request, "rune_rev")?),
	]))?;
	need(
		std::str::from_utf8(&actual)?.trim() == revision,
		"requested revision resolution changed",
	)?;
	let out = Path::new(field(request, "out")?);
	let expected = out.join("verified-rune.tar");
	let file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&expected)?;
	let status = Command::new("git")
		.args(["-C", repo, "archive", revision])
		.stdout(Stdio::from(file))
		.status()?;
	need(status.success(), "cannot reproduce requested Git archive")?;
	need(
		hash(&expected)? == hash(Path::new(field(request, "rune_archive")?))?,
		"Rune archive differs from requested revision",
	)?;
	let extracted = out.join("verified-rune-source");
	fs::create_dir(&extracted)?;
	run(Command::new("tar")
		.arg("-xf")
		.arg(&expected)
		.arg("-C")
		.arg(&extracted))?;
	let observed = source(Path::new(field(request, "rune_root")?), false, request)?;
	let frozen = source(&extracted, false, request)?;
	need(
		observed["files"] == frozen["files"],
		"exported Rune source differs from requested Git revision",
	)?;
	Ok(
		json!({"mode":"revision","requested":request["rune_rev"],"resolved":revision,"archive_sha256":hash(&expected)?,"verified_source":extracted}),
	)
}
pub fn before(request: &Value) -> Result<Value> {
	let registration = registration(request)?;
	let rnx = fs::canonicalize(field(request, "rnx_root")?)?;
	let rune = fs::canonicalize(field(request, "rune_root")?)?;
	let out = Path::new(field(request, "out")?);
	need(
		!out.starts_with(&rnx) && !out.starts_with(&rune),
		"output must be outside both source trees",
	)?;
	let revision = revision(request)?;
	let dependencies = dependencies(request)?;
	let executable = std::env::current_exe()?;
	need(
		fs::canonicalize(&executable)?.starts_with(fs::canonicalize(out.join("target"))?),
		"executable is outside the private Cargo target",
	)?;
	let executable_sha256 = hash(&executable)?;
	let artifact = artifact(request, &executable)?;
	let binary = out.join("bound-verification-child");
	let mut file = OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(&binary)?;
	std::io::copy(&mut fs::File::open(&executable)?, &mut file)?;
	drop(file);
	fs::set_permissions(&binary, fs::metadata(&executable)?.permissions())?;
	same_hash(&binary, &executable_sha256)?;
	let rnx_source = source(&rnx, true, request)?;
	let rune_source = source(&rune, false, request)?;
	let compiled_source = compiled(request, super::COMPILED_PINS, &rnx_source, &rune_source)?;
	let result = json!({"artifact":artifact,"registration":registration["label"],"registration_binding":registration,"rnx_source":rnx_source,"rune_source":rune_source,"compiled_source":compiled_source,"dependencies":dependencies,"revision":revision,"overlays":overlays(request)?,"executable":executable,"executable_sha256":executable_sha256,"binary_copy":binary,"binary_copy_sha256":hash(&binary)?,"dwarf":dwarf(&executable,&rnx,&rune)?,"args":std::env::args_os().map(|s|s.to_string_lossy().into_owned()).collect::<Vec<_>>(),"environment":std::env::vars().filter(|(k,_)| matches!(k.as_str(),"PATH"|"HOME"|"CARGO_HOME"|"RUSTUP_HOME"|"TMPDIR"|"LANG"|"TZ"|"RUSTFLAGS"|"CARGO_TARGET_DIR"|"CARGO_BUILD_JOBS"|"CARGO_INCREMENTAL"|"CARGO_PROFILE_DEV_DEBUG"|"CARGO_PROFILE_TEST_DEBUG"|"CARGO_PROFILE_DEV_STRIP"|"CARGO_PROFILE_TEST_STRIP") || k.starts_with("RNX_VERIFY_")).collect::<BTreeMap<_,_>>(),"bwrap_version":request["bwrap_version"],"request":request});
	write(&out.join("BINDING-BEFORE.json"), &result)?;
	Ok(result)
}
pub fn after(request: &Value, before: &Value) -> Result<()> {
	let out = Path::new(field(request, "out")?);
	need(
		artifact(request, &std::env::current_exe()?)? == before["artifact"],
		"compiler artifact changed during verification",
	)?;
	need(
		registration(request)? == before["registration_binding"],
		"registration/controller wiring changed",
	)?;
	need(
		source(Path::new(field(request, "rnx_root")?), true, request)? == before["rnx_source"],
		"Rnx source changed during verification",
	)?;
	need(
		source(Path::new(field(request, "rune_root")?), false, request)? == before["rune_source"],
		"Rune source changed during verification",
	)?;
	need(
		dependencies(request)? == before["dependencies"],
		"dependency metadata changed",
	)?;
	need(
		overlays(request)? == before["overlays"],
		"manifest/lock changed during verification",
	)?;
	need(
		hash(&std::env::current_exe()?)? == before["executable_sha256"],
		"running executable changed",
	)?;
	need(
		hash(Path::new(
			before["binary_copy"]
				.as_str()
				.ok_or("missing copied binary")?,
		))? == before["executable_sha256"],
		"copied executable changed",
	)?;
	write(
		&out.join("BINDING-AFTER.json"),
		&json!({"status":"PASS","normal_host_files":"checked separately outside overlays by --host-check"}),
	)
}
pub fn host_check(request: &Value) -> Result<()> {
	let root = Path::new(field(request, "rnx_root")?);
	let out = Path::new(field(request, "out")?);
	for (name, key) in [
		("Cargo.toml", "original_manifest"),
		("Cargo.lock", "original_lock"),
	] {
		need(
			hash(&root.join(name))? == hash(Path::new(field(request, key)?))?,
			&format!("normal host {name} changed"),
		)?;
	}
	let binding_path = out.join("BINDING-BEFORE.json");
	if !binding_path.is_file() {
		write(
			&out.join("HOST-CHECK.json"),
			&json!({"status":"PASS_NORMAL_FILES_ONLY","full_source_check":"unavailable because controller did not produce BINDING-BEFORE.json"}),
		)?;
		return Err("missing controller binding; no final acceptance".into());
	}
	let before = read(&binding_path)?;
	need(
		artifact(
			request,
			Path::new(
				before["executable"]
					.as_str()
					.ok_or("missing original executable")?,
			),
		)? == before["artifact"],
		"host compiler artifact changed",
	)?;
	need(
		overlays(request)? == before["overlays"],
		"host private graph changed",
	)?;
	need(
		dependencies(request)? == before["dependencies"],
		"host dependency metadata changed",
	)?;
	need(
		source(root, true, request)? == before["rnx_source"],
		"host Rnx source changed since controller binding",
	)?;
	need(
		source(Path::new(field(request, "rune_root")?), false, request)? == before["rune_source"],
		"host Rune source changed since controller binding",
	)?;
	same_hash(
		Path::new(
			before["binary_copy"]
				.as_str()
				.ok_or("missing copied binary path")?,
		),
		before["executable_sha256"]
			.as_str()
			.ok_or("missing binary digest")?,
	)?;
	let host = out.join("HOST-CHECK.json");
	write(
		&host,
		&json!({"status":"PASS","normal_manifest_sha256":hash(&root.join("Cargo.toml"))?,"normal_lock_sha256":hash(&root.join("Cargo.lock"))?,"full_sources":"unchanged from BINDING-BEFORE","binary_copy":"digest unchanged"}),
	)?;
	let result_path = out.join("RESULT.json");
	let result = read(&result_path)?;
	need(
		result["status"] == "PASS" && result["binding"] == before,
		"suite result is not a passing bound result",
	)?;
	let after_path = out.join("BINDING-AFTER.json");
	need(
		read(&after_path)?["status"] == "PASS",
		"controller post-check did not pass",
	)?;
	write(
		&out.join("COMPLETE.json"),
		&json!({"status":"PASS","result_sha256":hash(&result_path)?,"binding_sha256":hash(&binding_path)?,"binding_after_sha256":hash(&after_path)?,"host_check_sha256":hash(&host)?,"acceptance":"suite PASS plus controller and outside-overlay host guards"}),
	)
}

#[cfg(test)]
#[path = "binding_tests.rs"]
mod tests;
