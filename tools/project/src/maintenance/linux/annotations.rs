//! Only explicitly named project documents. Never Project::open, graph resolution,
//! source verification, receipt refresh or a project writer lock.
use super::*;

struct Observed {
	path: PathBuf,
	file: File,
	stamp: (u64, u64, u64, i64, i64, u32),
}
fn stamp(m: &std::fs::Metadata) -> (u64, u64, u64, i64, i64, u32) {
	(
		m.dev(),
		m.ino(),
		m.len(),
		m.mtime(),
		m.mtime_nsec(),
		m.mode(),
	)
}
fn read(
	path: &Path,
	limit: usize,
	budget: &mut Budget,
	seen: &mut Vec<Observed>,
) -> Result<Vec<u8>> {
	let file = OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC)
		.open(path)
		.map_err(|e| format!("{path:?}: {e}"))?;
	let m = file.metadata().map_err(error)?;
	if !m.is_file() {
		return Err(format!("{path:?}: not a regular project document"));
	}
	let bytes = read_bytes(&file, limit, budget)?;
	seen.push(Observed {
		path: path.to_owned(),
		file,
		stamp: stamp(&m),
	});
	Ok(bytes)
}
fn string<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
	v[key]
		.as_str()
		.ok_or_else(|| format!("missing or invalid {key}"))
}
fn path(v: &Value, key: &str) -> Result<PathBuf> {
	let p = PathBuf::from(string(v, key)?);
	if !p.is_absolute()
		|| p.components().any(|c| {
			matches!(
				c,
				std::path::Component::ParentDir | std::path::Component::CurDir
			)
		}) {
		return Err(format!("{key} must be a recorded absolute path"));
	}
	Ok(p)
}
fn digest(v: &Value, key: &str) -> Result<()> {
	if !valid_id(string(v, key)?) {
		Err(format!("invalid {key}"))
	} else {
		Ok(())
	}
}
fn one(manifest: &Path, kind: &str, budget: &mut Budget) -> Result<Value> {
	let mut seen = vec![];
	crate::manifest::Manifest::parse(&read(
		manifest,
		crate::input::MANIFEST_LIMIT,
		budget,
		&mut seen,
	)?)?;
	let base = manifest.parent().ok_or("manifest has no parent")?;
	let lock = documents::decode(
		&read(
			&base.join("rnx.lock"),
			crate::input::DOCUMENT_LIMIT,
			budget,
			&mut seen,
		)?,
		budget,
	)?;
	let receipt = documents::decode(
		&read(
			&base.join(".rnx/receipt.json"),
			crate::input::DOCUMENT_LIMIT,
			budget,
			&mut seen,
		)?,
		budget,
	)?;
	if lock.as_object().is_none_or(|o| {
		o.len() != 5
			|| o.keys().any(|k| {
				!matches!(
					k.as_str(),
					"format" | "declarations" | "sources" | "inputs" | "assembly"
				)
			})
	}) {
		return Err("malformed lock envelope".into());
	}
	if receipt.as_object().is_none_or(|o| {
		o.keys().any(|k| {
			!matches!(
				k.as_str(),
				"format"
					| "assembly_key"
					| "lock_sha256" | "executable_sha256"
					| "lock_blake3" | "executable_blake3"
					| "stamp"
			)
		})
	}) {
		return Err("unrecognized receipt fields".into());
	}
	let lf = lock["format"]
		.as_u64()
		.filter(|v| matches!(v, 1..=3))
		.ok_or("unsupported project lock format")?;
	let rf = receipt["format"]
		.as_u64()
		.filter(|v| matches!(v, 1..=4))
		.ok_or("unsupported receipt format")?;
	if (lf == 3) != (rf == 4) {
		return Err("lock and receipt formats disagree".into());
	}
	let suffix = if rf == 4 { "blake3" } else { "sha256" };
	digest(&receipt, &format!("lock_{suffix}"))?;
	digest(&receipt, &format!("executable_{suffix}"))?;
	// Validate declaration syntax only. No dependency manifest or source is read.
	let declarations: crate::manifest::Manifest =
		serde_json::from_value(lock["declarations"].clone()).map_err(error)?;
	declarations.validate()?;
	if lock["sources"].as_object().is_none() || lock["inputs"].as_object().is_none() {
		return Err("malformed project lock envelope".into());
	}
	if rf != 1 {
		let s = &receipt["stamp"];
		if s["bytes"]
			.as_u64()
			.is_none_or(|n| n > crate::fingerprint::BYTES)
			|| s["mtime_seconds"].as_i64().is_none()
			|| s["mtime_nanoseconds"]
				.as_u64()
				.is_none_or(|n| n >= 1_000_000_000)
			|| s["executable"].as_bool().is_none()
			|| s["device"].as_u64().is_none()
			|| s["inode"].as_u64().is_none()
		{
			return Err("invalid receipt stamp".into());
		}
	}
	let handoff: crate::wire::Handoff =
		serde_json::from_value(lock["sources"].clone()).map_err(error)?;
	handoff.validate()?;
	if !["packages", "trees", "outside_manifests"]
		.iter()
		.all(|k| lock["inputs"]["source"][k].is_array())
	{
		return Err("malformed source inventory envelope".into());
	}

	let assembly = &lock["assembly"];
	let shape = string(assembly, "kind")?;
	let shared = shape == "shared";
	if shared != (receipt.get("assembly_key").is_some_and(|v| !v.is_null())) || (rf == 3 && !shared)
	{
		return Err("receipt assembly binding disagrees with lock".into());
	}
	let mut identity = None;
	if shared {
		if lf == 1 {
			return Err("format-1 lock cannot name a shared entry".into());
		}
		let i = documents::decode(string(assembly, "identity")?.as_bytes(), budget)?;
		if i["format"].as_u64() != Some(if lf == 3 { 2 } else { 1 })
			|| i["generator"].as_u64() != Some(if lf == 3 { 2 } else { 1 })
		{
			return Err("unsupported assembly identity format or generator".into());
		}
		path(&i["context"], "cache_root")?;
		digest(&receipt, "assembly_key")?;
		identity = Some(i);
	} else if !matches!(shape, "generated" | "executable") {
		return Err("unknown assembly kind".into());
	}
	if shape == "generated" {
		for name in ["manifest", "main", "cargo_lock"] {
			digest(assembly, &format!("{name}_{suffix}"))?;
		}
		for name in ["target", "profile", "rustc", "cargo"] {
			string(assembly, name)?;
		}
		if !assembly["features"]
			.as_array()
			.is_some_and(|v| v.iter().all(Value::is_string))
		{
			return Err("invalid generated features".into());
		}
	} else if shape == "executable" {
		path(assembly, "path")?;
		digest(assembly, suffix)?;
	}

	let reference = if kind == "cache" {
		if let Some(i) = &identity {
			let key = string(&receipt, "assembly_key")?;
			json!({"kind":"shared assembly","path":path(&i["context"],"cache_root")?.join("entries").join(key),"id":key})
		} else if shape == "executable" {
			json!({"kind":"executable override","path":path(assembly,"path")?})
		} else {
			json!({"kind":"project-local artifact","path":base.join(".rnx/artifacts").join(string(&receipt,&format!("executable_{suffix}"))?)})
		}
	} else if declarations.runtime.is_none() {
		json!({"kind":"executable override","path":path(assembly,"path")?})
	} else {
		let packages = lock["inputs"]["native"]["packages"]
			.as_array()
			.ok_or("missing recorded native packages")?;
		let roots = packages
			.iter()
			.filter(|p| p["name"].as_str() == Some("rnx"))
			.map(|p| path(p, "root"))
			.collect::<Result<Vec<_>>>()?;
		if roots.len() != 1 {
			return Err("recorded runtime source is missing or ambiguous".into());
		}
		let source = &roots[0];
		let entry = source.parent().ok_or("runtime source has no parent")?;
		if source.file_name() == Some(OsStr::new("source"))
			&& entry
				.file_name()
				.and_then(OsStr::to_str)
				.is_some_and(valid_id)
			&& entry.parent().and_then(Path::file_name) == Some(OsStr::new("entries"))
		{
			json!({"kind":"installed runtime","path":entry,"source":source,"id":entry.file_name().unwrap().to_str().unwrap()})
		} else {
			json!({"kind":"runtime outside an installation","path":source})
		}
	};
	hooks::point("annotation-before-recheck")?;
	for observed in seen {
		if stamp(&observed.file.metadata().map_err(error)?) != observed.stamp
			|| stamp(&std::fs::symlink_metadata(&observed.path).map_err(error)?) != observed.stamp
		{
			return Err(format!("project document changed: {:?}", observed.path));
		}
	}
	Ok(reference)
}
pub(super) fn inspect(args: &Args, budget: &mut Budget) -> Result<Vec<Value>> {
	let mut out = vec![];
	let mut seen = HashSet::new();
	for supplied in &args.manifests {
		commands::check()?;
		let resolved = supplied.canonicalize().map_err(error);
		let key = resolved.as_ref().unwrap_or(supplied).clone();
		if !seen.insert(key.clone()) {
			continue;
		}
		budget.reserve(key.as_os_str().len() as u64 + 256)?;
		let result = resolved.and_then(|p| {
			let value = one(&p, &args.kind, budget)?;
			if supplied.canonicalize().map_err(error)? != p {
				return Err("named manifest alias changed".into());
			}
			Ok(value)
		});
		match result {
			Ok(mut v) => {
				v["manifest"] = json!(key);
				v["status"] = json!("recorded reference; contents not authenticated");
				out.push(v)
			}
			Err(e) => {
				commands::check()?;
				out.push(json!({"manifest":format!("{key:?}"),"status":"indeterminate","reason":e}))
			}
		}
	}
	Ok(out)
}
