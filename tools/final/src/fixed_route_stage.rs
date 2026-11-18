//! `rnx-final stage fixed-route` (ADR 0298): the fixed-route gate,
//! as `fixed_route_gate.py` ran it, over the measurement policy, bands and
//! anchor cache of `fixed_route`. Its three measurement children stay
//! Python and run exactly as the gate ran them: `paired_build.py` and
//! `fixed_route_vectors.py` through the stage wrapper, `executor_identity.py`
//! when the revisions differ. The recipe and capture key are the published
//! gate's, so the live anchor cache's entries stay hits.
use crate::child::{Raised, raise, value_error};
use crate::failure::Environment;
use crate::fixed_route::{self as fr, Cache};
use crate::pyjson::identity;
use crate::receipt::{atomic_json, digest};
use anyhow::{Context as _, Result, bail};
use serde_json::{Map, Value, json};
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The measurement tooling the recipe digests; the children's own files.
pub const TOOLING: [&str; 5] = [
	"tools/column/paired_build.py",
	"tools/column/fixed_route_vectors.py",
	"tools/column/old_path_child.py",
	"tools/column/ir_accounting.py",
	"tools/column/old_path_programs.py",
];
/// `paired_build.BUILD_KEYS`.
pub const BUILD_KEYS: [&str; 25] = [
	"AR",
	"CARGO_HOME",
	"CC",
	"CFLAGS",
	"CXX",
	"CXXFLAGS",
	"HOME",
	"LANG",
	"LC_ALL",
	"LDFLAGS",
	"PATH",
	"PKG_CONFIG_LIBDIR",
	"PKG_CONFIG_PATH",
	"PKG_CONFIG_SYSROOT_DIR",
	"RUSTC",
	"RUSTC_WORKSPACE_WRAPPER",
	"RUSTC_WRAPPER",
	"RUSTDOC",
	"RUSTDOCFLAGS",
	"RUSTFLAGS",
	"RUSTUP_HOME",
	"RUSTUP_TOOLCHAIN",
	"SOURCE_DATE_EPOCH",
	"TMPDIR",
	"TZ",
];
/// `paired_build.BUILD_PREFIXES`.
pub const BUILD_PREFIXES: [&str; 2] = ["CARGO_", "LC_"];
/// `ir_pool.ALLOCATOR`.
pub const ALLOCATOR: &str = "glibc.malloc.arena_max=1:glibc.malloc.mmap_threshold=131072:glibc.malloc.trim_threshold=131072:glibc.malloc.top_pad=0:glibc.malloc.perturb=0";
/// The anchor cache and the legacy anchors, under the checkout.
pub const CACHE: &str = "target/fixed-route-anchor-cache-v2";
pub const LEGACY: &str = "target/fixed-route-anchors";
/// What a stage child may take.
const CHILD_LIMIT: Duration = Duration::from_secs(28800);

/// `ir_pool.capture_environment()`: the measured programs' environment.
pub fn capture_environment() -> Environment {
	[
		("PATH", "/usr/local/bin:/usr/bin:/bin"),
		("HOME", "/tmp"),
		("LC_ALL", "C"),
		("POLARS_MAX_THREADS", "1"),
		("PYTHONDONTWRITEBYTECODE", "1"),
		("GLIBC_TUNABLES", ALLOCATOR),
	]
	.into_iter()
	.map(|(k, v)| (k.to_string(), v.to_string()))
	.collect()
}

/// Everything the gate asks of its host, so a control can answer.
pub trait Host {
	/// The record's baseline and candidate.
	fn record(&mut self, root: &Path) -> Result<Value>;
	/// `git status --porcelain` in the checkout.
	fn status(&mut self, root: &Path) -> Result<Vec<u8>>;
	fn source(&mut self, root: &Path) -> Result<String>;
	fn head(&mut self, root: &Path) -> Option<String>;
	fn approval(
		&mut self,
		root: &Path,
		baseline: &str,
		candidate: &str,
		policy: &Value,
	) -> Result<()>;
	fn condition(
		&mut self,
		root: &Path,
		baseline: &str,
		candidate: &str,
		policy: &Value,
		guard: &Path,
	) -> Result<Value>;
	/// A command with its output in `log`: its exit code. Its whole group
	/// is drained before this returns.
	fn run(&mut self, command: &[String], cwd: Option<&Path>, log: File) -> Result<i32>;
	/// A stage child, in the checkout, with its output in `log`, within
	/// `limit`: its exit code.
	fn child(&mut self, command: &[String], cwd: &Path, log: File, limit: Duration) -> Result<i32>;
	/// A query's standard output (`rustup which`, `cargo metadata`).
	fn output(&mut self, command: &[String], cwd: Option<&Path>) -> Result<String>;
	fn cpuinfo(&mut self) -> Result<String>;
	/// Python's `shutil.which` over the environment's PATH.
	fn which(&self, name: &str, env: &Environment) -> Option<PathBuf>;
}

/// `paired_build.build_environment`: the build's environment, every
/// unknown compiler override refused, the path, locales and Cargo
/// settings stabilized, the target dropped.
pub fn build_environment(env: &Environment, host: &mut dyn Host) -> Result<Environment> {
	for key in env.keys() {
		let overriding = ["RUST", "CC_", "CXX_", "CFLAGS_", "LDFLAGS_", "PKG_CONFIG_"]
			.iter()
			.any(|p| key.starts_with(p));
		if overriding && !BUILD_KEYS.contains(&key.as_str()) {
			bail!(value_error(&format!("undeclared build variable: {key}")));
		}
	}
	let mut out: Environment = env
		.iter()
		.filter(|(k, _)| {
			BUILD_KEYS.contains(&k.as_str()) || BUILD_PREFIXES.iter().any(|p| k.starts_with(p))
		})
		.map(|(k, v)| (k.clone(), v.clone()))
		.collect();
	let Some(rustup) = host.which("rustup", env) else {
		bail!(value_error("rustup unavailable"));
	};
	let compiler = host
		.output(
			&[
				rustup.to_string_lossy().into_owned(),
				"which".into(),
				"--toolchain".into(),
				"1.98.1".into(),
				"rustc".into(),
			],
			None,
		)?
		.trim()
		.to_owned();
	let absolute = if rustup.is_absolute() {
		rustup.clone()
	} else {
		std::env::current_dir()?.join(&rustup)
	};
	let mut paths: Vec<String> = Vec::new();
	for p in [
		absolute.parent().map(|p| p.to_string_lossy().into_owned()),
		Path::new(&compiler)
			.parent()
			.map(|p| p.to_string_lossy().into_owned()),
		Some("/usr/local/bin".into()),
		Some("/usr/bin".into()),
		Some("/bin".into()),
	]
	.into_iter()
	.flatten()
	{
		if !paths.contains(&p) {
			paths.push(p);
		}
	}
	out.retain(|k, _| !k.starts_with("LC_"));
	for (k, v) in [
		("PATH", paths.join(":")),
		("LANG", "C".into()),
		("LC_ALL", "C".into()),
		("TZ", "UTC".into()),
		("CARGO_BUILD_JOBS", "4".into()),
		("CARGO_INCREMENTAL", "0".into()),
		("CARGO_TERM_COLOR", "never".into()),
	] {
		out.insert(k.into(), v);
	}
	out.remove("CARGO_TARGET_DIR");
	Ok(out)
}

/// The measurement recipe: what was built, by what toolchain, on what CPU,
/// in what environments, under what policy, with what tooling and fixtures.
pub fn recipe(
	root: &Path,
	policy: &Value,
	env: &Environment,
	host: &mut dyn Host,
) -> Result<Value> {
	let cpuinfo = host.cpuinfo()?;
	let first = cpuinfo.split("\n\n").next().unwrap_or_default();
	let cpu: Vec<&str> = first
		.lines()
		.filter(|l| {
			["model name", "vendor_id", "flags"]
				.iter()
				.any(|p| l.starts_with(p))
		})
		.collect();
	let mut toolchain = Map::new();
	for name in ["rustc", "cargo"] {
		let path = host.output(
			&["rustup", "which", "--toolchain", "1.98.1", name].map(String::from),
			None,
		)?;
		toolchain.insert(name.into(), json!(digest(Path::new(path.trim()))?));
	}
	let run_environment = capture_environment();
	let build = build_environment(env, host)?;
	let mut subset = Map::new();
	for k in [
		"schema",
		"anchor",
		"kernels",
		"modes",
		"sizes",
		"per_record_per_mille",
		"cumulative_per_mille",
	] {
		subset.insert(
			k.into(),
			policy
				.get(k)
				.cloned()
				.ok_or_else(|| raise("KeyError", &format!("'{k}'")))?,
		);
	}
	let mut tooling = Map::new();
	for p in TOOLING {
		tooling.insert(p.into(), json!(digest(&root.join(p))?));
	}
	let mut kernels: Vec<PathBuf> = std::fs::read_dir(root.join("tools/routing-bench/kernels"))?
		.filter_map(|e| e.ok().map(|e| e.path()))
		.filter(|p| p.extension().is_some_and(|x| x == "rn"))
		.collect();
	kernels.sort();
	let mut fixtures = Map::new();
	for p in kernels {
		fixtures.insert(
			p.file_name()
				.unwrap_or_default()
				.to_string_lossy()
				.into_owned(),
			json!(digest(&p)?),
		);
	}
	let mut keys: Vec<&String> = build.keys().collect();
	keys.sort();
	Ok(json!({
		"build": ["cargo", "+1.98.1", "build", "--locked", "--release", "--bin", "rnx"],
		"toolchain": toolchain,
		"cpu": cpu,
		"environment": crate::child::environment(&run_environment)?,
		"run_environment": run_environment,
		"build_environment": {"sha256": identity(&build)?, "keys": keys,
			"allowlist": BUILD_KEYS, "prefixes": BUILD_PREFIXES},
		"policy": subset,
		"tooling": tooling,
		"fixtures": fixtures,
	}))
}

/// The cache's key: the binary and only what determines its counts.
pub fn capture_key(binary_sha256: &str, current: &Value) -> Result<String> {
	identity(
		&json!({"binary_sha256": binary_sha256, "measurement": fr::measurement_recipe(current)?}),
	)
}

/// A capture receipt whose every artifact still has its digest.
pub fn verified(receipt: Value) -> Result<Value> {
	if receipt.get("status") != Some(&json!("PASS"))
		|| !receipt
			.get("artifacts")
			.is_some_and(|a| a.as_object().is_some_and(|o| !o.is_empty()))
	{
		bail!(value_error("invalid capture receipt"));
	}
	for (path, sha) in receipt["artifacts"].as_object().into_iter().flatten() {
		if json!(digest(Path::new(path))?) != *sha {
			bail!(value_error(&format!("capture artifact changed: {path}")));
		}
	}
	Ok(receipt)
}

fn read(path: &Path) -> Result<Value> {
	Ok(serde_json::from_slice(&std::fs::read(path)?)?)
}

fn at<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
	value
		.get(key)
		.ok_or_else(|| raise("KeyError", &format!("'{key}'")))
}

fn text(path: &Path) -> String {
	path.to_string_lossy().into_owned()
}

/// A legacy capture of the anchor, imported under the binary identity rule.
pub fn reuse_anchor(path: &Path, binary: &Value, current: &Value, policy: &Value) -> Result<Value> {
	let original = verified(read(path)?)?;
	if at(&original, "revision")? != at(policy, "anchor")? {
		bail!(value_error("wrong imported anchor revision"));
	}
	// Legacy receipts locate their binary in the preserved capture manifest.
	let first = &at(&original, "records")?[0];
	let profile = PathBuf::from(at(first, "profile")?.as_str().unwrap_or_default());
	let manifest_path = profile
		.parent()
		.and_then(Path::parent)
		.context("profile without capture directory")?
		.join("MANIFEST.json");
	let manifest = read(&manifest_path)?;
	let role = at(first, "role")?.as_str().unwrap_or_default();
	let measured = at(at(&manifest, "binaries")?, role)?;
	let measured_sha = at(measured, "sha256")?;
	if json!(digest(Path::new(
		at(measured, "binary")?.as_str().unwrap_or_default()
	))?) != *measured_sha
		|| measured_sha != at(binary, "sha256")?
	{
		bail!(value_error("anchor binary differs; new capture required"));
	}
	let environment = at(&manifest, "environment")?;
	let mut measured_recipe = at(&original, "recipe")?.clone();
	if measured_recipe
		.get("run_environment")
		.unwrap_or(environment)
		!= environment
	{
		bail!(value_error("capture environment provenance mismatch"));
	}
	let Some(recipe) = measured_recipe.as_object_mut() else {
		bail!(raise(
			"TypeError",
			"the legacy capture's recipe is not a mapping"
		));
	};
	recipe.insert("run_environment".into(), environment.clone());
	let env: Environment = serde_json::from_value(environment.clone())?;
	if crate::child::environment(&env)? != *at(at(&original, "recipe")?, "environment")? {
		bail!(value_error("legacy environment identity mismatch"));
	}
	if fr::measurement_recipe(&measured_recipe)? != fr::measurement_recipe(current)? {
		bail!(value_error(
			"anchor measurement inputs differ; new capture required"
		));
	}
	if env != capture_environment() {
		bail!(value_error("anchor run environment differs"));
	}
	let binary_path = PathBuf::from(at(binary, "binary")?.as_str().unwrap_or_default());
	if json!(digest(&binary_path)?) != *at(binary, "sha256")? {
		bail!(value_error("current build binary changed"));
	}
	let resolved = crate::cross::resolve(path)?;
	let mut artifacts = at(&original, "artifacts")?
		.as_object()
		.cloned()
		.unwrap_or_default();
	artifacts.insert(text(&resolved), json!(digest(path)?));
	artifacts.insert(text(&manifest_path), json!(digest(&manifest_path)?));
	artifacts.insert(text(&binary_path), at(binary, "sha256")?.clone());
	let sha = at(binary, "sha256")?
		.as_str()
		.unwrap_or_default()
		.to_owned();
	let mut out = original.clone();
	let o = out.as_object_mut().context("receipt is not an object")?;
	o.insert("recipe".into(), measured_recipe);
	o.insert("artifacts".into(), Value::Object(artifacts));
	o.insert("binary_sha256".into(), json!(sha));
	o.insert("capture_key".into(), json!(capture_key(&sha, current)?));
	o.insert(
		"reuse_proof".into(),
		json!({"original_receipt": text(&resolved), "original_sha256": digest(path)?,
			"manifest_sha256": digest(&manifest_path)?, "binary_sha256": sha,
			"current_build": binary, "current_recipe": current}),
	);
	Ok(out)
}

/// A stage child that failed, with its retained evidence.
#[derive(Debug)]
pub struct StageFailure {
	pub stage: String,
	pub failure: Value,
}

impl std::fmt::Display for StageFailure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(
			f,
			"{} failed: STOP; {}",
			self.stage,
			self.failure["error"].as_str().unwrap_or_default()
		)
	}
}

impl std::error::Error for StageFailure {}

/// Python's class name of an error, as a receipt reports it.
fn class(error: &anyhow::Error) -> String {
	if let Some(r) = error.downcast_ref::<Raised>() {
		return r.class.to_string();
	}
	if error.downcast_ref::<StageFailure>().is_some() {
		return "failure_detail.StageFailure".into();
	}
	if let Some(e) = error.downcast_ref::<std::io::Error>() {
		return match e.kind() {
			std::io::ErrorKind::NotFound => "FileNotFoundError",
			std::io::ErrorKind::AlreadyExists => "FileExistsError",
			std::io::ErrorKind::PermissionDenied => "PermissionError",
			_ => "OSError",
		}
		.into();
	}
	if error.downcast_ref::<serde_json::Error>().is_some() {
		return "json.decoder.JSONDecodeError".into();
	}
	"RuntimeError".into()
}

/// Python's `str(error)`.
fn message(error: &anyhow::Error) -> String {
	match error.downcast_ref::<Raised>() {
		Some(r) => r.message.clone(),
		None => format!("{error:#}"),
	}
}

fn create(path: &Path) -> Result<File> {
	Ok(std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)?)
}

/// `cross_program_gate.stage`: a child in the checkout with its output in
/// `<name>.log`, its receipt read, a stage row written; a failure keeps its
/// receipt or the row, the log and the evidence they name.
#[allow(clippy::too_many_arguments)]
fn stage(
	out: &Path,
	root: &Path,
	name: &str,
	command: &[String],
	receipt: &Path,
	env: &Environment,
	host: &mut dyn Host,
) -> Result<Value> {
	let log = out.join(format!("{name}.log"));
	let row = out.join(format!("{name}-stage.json"));
	let mut code = None;
	let mut value = None;
	let attempt = (|| -> Result<Value> {
		let c = host.child(command, root, create(&log)?, CHILD_LIMIT)?;
		code = Some(c);
		let v = read(receipt)?;
		value = Some(v.clone());
		atomic_json(
			&row,
			&json!({"exit": c, "command": command, "receipt": text(receipt), "sha256": digest(receipt)?,
				"log_sha256": digest(&log)?, "status": v.get("status")}),
		)?;
		if c != 0 || v.get("status") != Some(&json!("PASS")) {
			let shown = v
				.get("error")
				.or_else(|| v.get("status"))
				.map(|s| match s {
					Value::String(s) => s.clone(),
					Value::Null => "None".into(),
					other => other.to_string(),
				})
				.unwrap_or_else(|| "None".into());
			bail!(raise("RuntimeError", &format!("child failed: {shown}")));
		}
		Ok(v)
	})();
	match attempt {
		Ok(v) => Ok(v),
		Err(error) => {
			let shown = message(&error);
			let short = class(&error);
			atomic_json(
				&row,
				&json!({"status": "FAIL", "exit": code, "error": shown,
					"error_type": short.rsplit('.').next().unwrap_or(&short)}),
			)?;
			let evidence = if value.as_ref().is_some_and(Value::is_object) {
				receipt.to_path_buf()
			} else {
				row.clone()
			};
			let failure = crate::failure::retain(
				out,
				name,
				Some(&evidence),
				&[log],
				&[out.to_path_buf()],
				env,
				&shown,
			)?;
			Err(anyhow::Error::new(StageFailure {
				stage: name.into(),
				failure,
			}))
		}
	}
}

/// Absolute against the launch directory, links not resolved, as Python's
/// `sys.executable` and `Path.absolute` spell a program: a relative PATH
/// entry survives the children's working directory.
fn absolute(path: PathBuf, launch: &Path) -> String {
	text(&if path.is_absolute() {
		path
	} else {
		launch.join(path)
	})
}

/// The interpreter the Python children run under, fixed once per run.
fn python(env: &Environment, host: &dyn Host, launch: &Path) -> Result<String> {
	Ok(absolute(
		host.which("python3", env)
			.context("python3 is required for the measurement children")?,
		launch,
	))
}

/// A paired build's receipt: exactly two builds, each naming its source
/// commit, binary and digest, before any of them is used.
fn checked_pair(pair: Value) -> Result<Value> {
	let builds = pair.get("builds").and_then(Value::as_array);
	let Some(builds) = builds.filter(|b| b.len() == 2) else {
		bail!(value_error("build receipt must name exactly two builds"));
	};
	for b in builds {
		let field = |k: &str| {
			b.get(k)
				.and_then(Value::as_str)
				.is_some_and(|v| !v.is_empty())
		};
		let sha = b.get("sha256").and_then(Value::as_str).unwrap_or_default();
		if !field("source_commit")
			|| !field("binary")
			|| sha.len() != 64
			|| !sha.bytes().all(|c| c.is_ascii_hexdigit())
		{
			bail!(value_error(
				"build receipt lacks a source commit, binary or digest"
			));
		}
	}
	Ok(pair)
}

#[allow(clippy::too_many_arguments)]
fn build(
	out: &Path,
	root: &Path,
	baseline: &str,
	candidate: &str,
	cargo: &str,
	py: &str,
	env: &Environment,
	host: &mut dyn Host,
) -> Result<Value> {
	let command = [
		py.to_string(),
		"-B".into(),
		"tools/column/paired_build.py".into(),
		"--out".into(),
		text(&out.join("pair")),
		"--baseline".into(),
		baseline.into(),
		"--candidate".into(),
		candidate.into(),
		"--cargo".into(),
		cargo.into(),
	];
	checked_pair(stage(
		out,
		root,
		"build",
		&command,
		&out.join("pair/RESULT.json"),
		env,
		host,
	)?)
}

/// The capture receipts of a built pair, baseline then candidate; the
/// instruction-identity observation first when the revisions differ, its
/// tool error fatal.
#[allow(clippy::too_many_arguments)]
fn captures(
	out: &Path,
	root: &Path,
	pair: &Value,
	valgrind: &Path,
	policy: &Value,
	current: &Value,
	py: &str,
	env: &Environment,
	host: &mut dyn Host,
) -> Result<Vec<Value>> {
	let builds = at(pair, "builds")?.as_array().cloned().unwrap_or_default();
	let revisions: Vec<String> = builds
		.iter()
		.map(|b| b["source_commit"].as_str().unwrap_or_default().to_owned())
		.collect();
	if revisions.first() != revisions.get(1) {
		let command = [
			py.to_string(),
			"-B".into(),
			"tools/column/executor_identity.py".into(),
			"--baseline".into(),
			builds[0]["binary"].as_str().unwrap_or_default().into(),
			"--candidate".into(),
			builds[1]["binary"].as_str().unwrap_or_default().into(),
			"--out".into(),
			text(&out.join("IDENTITY.json")),
		];
		let status = host.run(&command, Some(root), create(&out.join("identity.log"))?)?;
		let ident = read(&out.join("IDENTITY.json"))?;
		let verdict = ident
			.as_object()
			.and_then(|o| o.get("status"))
			.and_then(Value::as_str)
			.is_some_and(|v| matches!(v, "PASS" | "FAIL"));
		let error = ident.get("error").is_some_and(|e| !e.is_null() && e != "");
		if !matches!(status, 0 | 1) || !verdict || error {
			bail!(value_error("identity tool failed, not a measured mismatch"));
		}
	}
	let command = [
		py.to_string(),
		"-B".into(),
		"tools/column/fixed_route_vectors.py".into(),
		"--capture-only".into(),
		"--out".into(),
		text(&out.join("vectors")),
		"--build-receipt".into(),
		text(&out.join("pair/RESULT.json")),
		"--valgrind-root".into(),
		text(valgrind),
	];
	let vectors = stage(
		out,
		root,
		"capture",
		&command,
		&out.join("vectors/RESULT.json"),
		env,
		host,
	)?;
	let mut rows = Vec::new();
	for p in at(&vectors, "records")?.as_array().into_iter().flatten() {
		rows.push(read(Path::new(p.as_str().unwrap_or_default()))?);
	}
	let policy_sha = identity(policy)?;
	let mut result = Vec::new();
	for ((role, revision), binary) in ["baseline", "candidate"]
		.iter()
		.zip(&revisions)
		.zip(&builds)
	{
		let selected: Vec<Value> = rows
			.iter()
			.filter(|r| r["role"] == *role)
			.cloned()
			.collect();
		let mut artifacts = Map::new();
		artifacts.insert(
			binary["binary"].as_str().unwrap_or_default().into(),
			binary["sha256"].clone(),
		);
		for r in &selected {
			let profile = PathBuf::from(r["profile"].as_str().unwrap_or_default());
			let dir = profile.parent().unwrap_or(Path::new(""));
			artifacts.insert(text(&profile), r["profile_sha256"].clone());
			artifacts.insert(text(&dir.join("program.rn")), r["fixture_sha256"].clone());
			artifacts.insert(
				text(&dir.join("program.stdout")),
				r["stdout_sha256"].clone(),
			);
		}
		let sha = binary["sha256"].as_str().unwrap_or_default();
		result.push(json!({"status": "PASS", "revision": revision, "recipe": current, "policy_sha256": policy_sha,
			"binary_sha256": sha, "capture_key": capture_key(sha, current)?, "records": selected, "artifacts": artifacts}));
	}
	Ok(result)
}

/// The gate's arguments.
pub struct Options {
	/// The launch directory, against which relative programs are spelled.
	pub cwd: PathBuf,
	pub out: PathBuf,
	pub cargo: Option<String>,
	pub valgrind_root: Option<PathBuf>,
	pub anchor_only: bool,
	pub import_anchor: Option<PathBuf>,
}

pub fn parse(args: &[String], cwd: &Path) -> Result<Options> {
	let mut o = Options {
		cwd: cwd.to_path_buf(),
		out: PathBuf::new(),
		cargo: None,
		valgrind_root: None,
		anchor_only: false,
		import_anchor: None,
	};
	let mut out = None;
	let path = |v: &str| {
		let p = PathBuf::from(v);
		if p.is_absolute() { p } else { cwd.join(p) }
	};
	let mut i = 0;
	while i < args.len() {
		let value = || args.get(i + 1).cloned().context("an option needs a value");
		match args[i].as_str() {
			"--out" => out = Some(path(&value()?)),
			"--cargo" => o.cargo = Some(value()?),
			"--valgrind-root" => o.valgrind_root = Some(path(&value()?)),
			"--import-anchor" => o.import_anchor = Some(path(&value()?)),
			"--anchor-only" => {
				o.anchor_only = true;
				i += 1;
				continue;
			}
			other => bail!(value_error(&format!("unrecognized argument: {other}"))),
		}
		i += 2;
	}
	o.out = out.context("the fixed-route stage needs --out")?;
	Ok(o)
}

/// The semantic logs a final keeps from its assigned payload, never the
/// source exports or the raw vector payload.
const KEPT: [&str; 12] = [
	"guard-build.log",
	"mirror.log",
	"anchor/build-stage.json",
	"anchor/capture-stage.json",
	"anchor/build.log",
	"anchor/capture.log",
	"record/build-stage.json",
	"record/capture-stage.json",
	"record/build.log",
	"record/capture.log",
	"record/IDENTITY.json",
	"record/identity.log",
];

/// The gate: the stage's receipt, written whatever happens; the exit is 0
/// only for a PASS.
pub fn gate(o: &Options, root: &Path, env: &Environment, host: &mut dyn Host) -> Result<i32> {
	let retained = crate::cross::resolve(&o.out)?;
	std::fs::create_dir_all(retained.parent().context("--out has no parent")?)?;
	std::fs::create_dir(&retained)?;
	let out = crate::producer::assigned_root("RNX_FINAL_FIXED_ROOT", &retained, env)?;
	let mut r = Map::new();
	r.insert("status".into(), json!("FAIL"));
	let outcome = run_gate(o, root, env, host, &retained, &out, &mut r);
	if let Err(error) = outcome {
		r.insert("status".into(), json!("FAIL"));
		r.insert(
			"error".into(),
			json!(format!("{}: {}", class(&error), message(&error))),
		);
		if let Some(failed) = error.downcast_ref::<StageFailure>() {
			let failure = if out != retained {
				crate::failure::copy_bundle(
					&retained,
					&failed.failure,
					std::slice::from_ref(&out),
					env,
				)?
			} else {
				failed.failure.clone()
			};
			r.insert("failure".into(), failure);
		}
	}
	if out != retained {
		for relative in KEPT {
			let source = out.join(relative);
			if source.is_file() {
				let dest = retained.join(relative);
				std::fs::create_dir_all(dest.parent().context("kept log without directory")?)?;
				std::fs::copy(&source, &dest)?;
			}
		}
		r.insert(
			"payload_lifetime".into(),
			json!("artifact paths valid through live final validation; after release see PAYLOAD-RELEASE.json and final MANIFEST.json"),
		);
	}
	let result = Value::Object(r);
	crate::producer::publish(&retained.join("RESULT.json"), &result, env)?;
	let status = result["status"].as_str().unwrap_or("FAIL");
	println!("{status} {}", retained.join("RESULT.json").display());
	Ok(if status == "PASS" { 0 } else { 1 })
}

#[allow(clippy::too_many_arguments)]
fn run_gate(
	o: &Options,
	root: &Path,
	env: &Environment,
	host: &mut dyn Host,
	retained: &Path,
	out: &Path,
	r: &mut Map<String, Value>,
) -> Result<()> {
	let valgrind = crate::stage::valgrind_root_from(o.valgrind_root.as_deref(), env)?;
	let cargo = match &o.cargo {
		Some(c) => c.clone(),
		None => host
			.which("cargo", env)
			.map_or_else(|| "cargo".to_string(), |p| absolute(p, &o.cwd)),
	};
	let py = python(env, host, &o.cwd)?;
	let info = host.record(root)?;
	let candidate = at(&info, "candidate")?
		.as_str()
		.unwrap_or_default()
		.to_owned();
	let baseline = at(&info, "baseline")?
		.as_str()
		.unwrap_or_default()
		.to_owned();
	let source = host.source(root)?;
	if !host.status(root)?.is_empty() {
		bail!(value_error("clean candidate required"));
	}
	let policy = read(&root.join(fr::POLICY))?;
	fr::validate_policy(&policy)?;
	host.approval(root, &baseline, &candidate, &policy)?;
	let manifest = "ci/execute_guard/Cargo.toml";
	let guard_build = [
		&cargo,
		"+1.98.1",
		"build",
		"--locked",
		"--manifest-path",
		manifest,
	]
	.map(String::from);
	let code = host.run(
		&guard_build,
		Some(root),
		create(&out.join("guard-build.log"))?,
	)?;
	if code != 0 {
		bail!(crate::stage::called_process_error(&guard_build, code));
	}
	let mut guard = root.join("ci/execute_guard/target/debug/execute-guard");
	if !guard.exists() {
		let meta: Value = serde_json::from_str(
			&host.output(
				&[
					&cargo,
					"+1.98.1",
					"metadata",
					"--no-deps",
					"--format-version=1",
					"--manifest-path",
					manifest,
				]
				.map(String::from),
				Some(root),
			)?,
		)?;
		guard = PathBuf::from(at(&meta, "target_directory")?.as_str().unwrap_or_default())
			.join("debug")
			.join(
				meta["packages"][0]["targets"][0]["name"]
					.as_str()
					.unwrap_or_default(),
			);
	}
	r.insert(
		"source_condition".into(),
		host.condition(root, &baseline, &candidate, &policy, &guard)?,
	);
	let mirror = [text(&guard), text(root)];
	let code = host.run(&mirror, None, create(&out.join("mirror.log"))?)?;
	if code != 0 {
		bail!(crate::stage::called_process_error(&mirror, code));
	}
	let mut current = recipe(root, &policy, env, host)?;
	// Valgrind's executable is an explicit recipe input, never merely a path.
	current["valgrind"] = json!(digest(&valgrind.join("bin/valgrind"))?);
	let anchor_started = Instant::now();
	let attempt = out.join("anchor");
	std::fs::create_dir(&attempt)?;
	let anchor_revision = at(&policy, "anchor")?
		.as_str()
		.unwrap_or_default()
		.to_owned();
	let anchor_pair = build(
		&attempt,
		root,
		&anchor_revision,
		&anchor_revision,
		&cargo,
		&py,
		env,
		host,
	)?;
	let anchor_build_seconds = anchor_started.elapsed().as_secs_f64();
	let anchor_binary = anchor_pair["builds"][0].clone();
	let key = capture_key(
		anchor_binary["sha256"].as_str().unwrap_or_default(),
		&current,
	)?;
	let mut cache = Cache::open(&root.join(CACHE))?;
	let (found, reason) = cache.lookup(&key)?;
	let mut state = if found.is_some() { "hit" } else { "miss" };
	let mut legacy_outcome = json!({"status": "none"});
	let anchor = match found {
		Some(a) => a,
		None => {
			let legacy = root.join(LEGACY).join(&key).join("ANCHOR.json");
			let imported = o
				.import_anchor
				.clone()
				.or_else(|| legacy.is_file().then_some(legacy));
			let mut provenance = json!({});
			let mut anchor = None;
			if let Some(imported) = imported {
				match reuse_anchor(&imported, &anchor_binary, &current, &policy) {
					Ok(a) => {
						provenance = json!({"receipt_sha256": digest(&imported)?,
							"manifest_sha256": a["reuse_proof"]["manifest_sha256"]});
						state = "migrated";
						legacy_outcome = json!({"status": "migrated"});
						anchor = Some(a);
					}
					Err(error) => {
						let name = class(&error);
						let short = name.rsplit('.').next().unwrap_or(&name).to_owned();
						if !matches!(
							short.as_str(),
							"ValueError"
								| "KeyError" | "JSONDecodeError"
								| "OSError" | "FileNotFoundError"
								| "FileExistsError" | "PermissionError"
						) {
							return Err(error);
						}
						legacy_outcome = json!({"status": "failed", "error_class": short});
					}
				}
			}
			let anchor = match anchor {
				Some(a) => a,
				None => {
					let a = captures(
						&attempt,
						root,
						&anchor_pair,
						&valgrind,
						&policy,
						&current,
						&py,
						env,
						host,
					)?
					.remove(0);
					provenance = json!({"capture_sha256": identity(&a)?});
					a
				}
			};
			cache.publish(
				&anchor,
				Path::new(anchor_binary["binary"].as_str().unwrap_or_default()),
				&provenance,
			)?
		}
	};
	// The cache owns the anchor bytes; the current-build proof is ephemeral.
	if anchor["revision"] != anchor_revision
		|| anchor["binary_sha256"] != anchor_binary["sha256"]
		|| fr::measurement_recipe(&anchor["recipe"])? != fr::measurement_recipe(&current)?
	{
		bail!(value_error("anchor cache recipe differs"));
	}
	let mut anchor = anchor;
	anchor["reuse_proof"] = json!({"binary_sha256": anchor_binary["sha256"],
		"current_build": anchor_binary, "current_recipe": current});
	r.insert(
		"anchor_cache".into(),
		json!({"state": state, "lookup_reason": reason, "legacy_import": legacy_outcome, "key": key,
			"anchor_build_seconds": anchor_build_seconds, "anchor_total_seconds": anchor_started.elapsed().as_secs_f64()}),
	);
	let evidence = retained.join("ANCHOR-EVIDENCE.json");
	atomic_json(&evidence, &anchor)?;
	if o.anchor_only {
		r.insert("status".into(), json!("PASS"));
		r.insert("anchor".into(), json!(text(&evidence)));
		r.insert("recipe".into(), current);
	} else {
		let pair = out.join("record");
		std::fs::create_dir(&pair)?;
		let built = build(&pair, root, &baseline, &candidate, &cargo, &py, env, host)?;
		let mut both = captures(
			&pair, root, &built, &valgrind, &policy, &current, &py, env, host,
		)?;
		let changed = both.pop().context("no candidate capture")?;
		let base = both.pop().context("no baseline capture")?;
		let base = verified(base)?;
		let changed = verified(changed)?;
		let scored = fr::score(&base, &changed, Some(&anchor), &policy)?;
		for (k, v) in scored.as_object().into_iter().flatten() {
			r.insert(k.clone(), v.clone());
		}
		r.insert("baseline".into(), base);
		r.insert("candidate".into(), changed);
		r.insert("anchor".into(), json!(text(&evidence)));
	}
	drop(cache);
	if host.source(root)? != source || host.head(root).as_deref() != Some(candidate.as_str()) {
		bail!(value_error("source changed during gate"));
	}
	Ok(())
}

/// The live host: git and records through the controller, the policy
/// checks of `fixed_route`, children under the guardian.
pub struct Live {
	pub guardian: PathBuf,
	pub groups: PathBuf,
	pub env: Environment,
	/// Raised by SIGINT or SIGTERM in a standalone run: the running child is
	/// stopped and drained, no further child starts, and the gate publishes
	/// its FAIL receipt. Inside a final the controller owns interruption.
	pub stop: Option<&'static crate::guardian::StopSignal>,
}

impl Live {
	fn spawn(
		&self,
		command: &[String],
		cwd: &Path,
		log: File,
		limit: Option<Duration>,
	) -> Result<i32> {
		if self.stop.is_some() {
			crate::cross::interrupted()?;
		}
		let environment: Vec<(std::ffi::OsString, std::ffi::OsString)> =
			self.env.iter().map(|(k, v)| (k.into(), v.into())).collect();
		let guarded = crate::guardian::spawn(
			&self.guardian,
			&self.groups,
			command,
			cwd,
			&environment,
			log.try_clone()?,
			log,
		)?;
		let outcome = guarded.wait_until(limit, self.stop)?;
		if self.stop.is_some() {
			crate::cross::interrupted()?;
		}
		if let Some(stopped) = outcome.stopped {
			bail!(raise("RuntimeError", &stopped));
		}
		Ok(outcome.status)
	}
}

impl Host for Live {
	fn record(&mut self, root: &Path) -> Result<Value> {
		crate::selection::record(root, "HEAD")
	}
	fn status(&mut self, root: &Path) -> Result<Vec<u8>> {
		let output = crate::child::git_command()
			.args(["status", "--porcelain"])
			.current_dir(root)
			.output()?;
		if !output.status.success() {
			bail!(crate::stage::called_process_error(
				&["git", "status", "--porcelain"].map(String::from),
				output.status.code().unwrap_or(-1)
			));
		}
		Ok(output.stdout)
	}
	fn source(&mut self, root: &Path) -> Result<String> {
		crate::child::source(root)
	}
	fn head(&mut self, root: &Path) -> Option<String> {
		crate::child::head(root)
	}
	fn approval(
		&mut self,
		root: &Path,
		baseline: &str,
		candidate: &str,
		policy: &Value,
	) -> Result<()> {
		fr::anchor_approval(root, baseline, candidate, policy)
	}
	fn condition(
		&mut self,
		root: &Path,
		baseline: &str,
		candidate: &str,
		policy: &Value,
		guard: &Path,
	) -> Result<Value> {
		fr::source_condition(root, baseline, candidate, policy, guard)
	}
	fn run(&mut self, command: &[String], cwd: Option<&Path>, log: File) -> Result<i32> {
		let cwd = match cwd {
			Some(c) => c.to_path_buf(),
			None => std::env::current_dir()?,
		};
		self.spawn(command, &cwd, log, None)
	}
	fn child(&mut self, command: &[String], cwd: &Path, log: File, limit: Duration) -> Result<i32> {
		self.spawn(command, cwd, log, Some(limit))
	}
	fn output(&mut self, command: &[String], cwd: Option<&Path>) -> Result<String> {
		let mut c = std::process::Command::new(&command[0]);
		c.args(&command[1..]).env_clear().envs(&self.env);
		if let Some(cwd) = cwd {
			c.current_dir(cwd);
		}
		let out = c
			.output()
			.map_err(|e| crate::stage::spawn_error(&command[0], &e))?;
		if !out.status.success() {
			bail!(crate::stage::called_process_error(
				command,
				out.status.code().unwrap_or(-1)
			));
		}
		Ok(String::from_utf8(out.stdout)?)
	}
	fn cpuinfo(&mut self) -> Result<String> {
		Ok(std::fs::read_to_string("/proc/cpuinfo")?)
	}
	fn which(&self, name: &str, env: &Environment) -> Option<PathBuf> {
		let path = env.get("PATH").map_or("/bin:/usr/bin", String::as_str);
		path.split(':').find_map(|dir| {
			let candidate = Path::new(if dir.is_empty() { "." } else { dir }).join(name);
			let meta = std::fs::metadata(&candidate).ok()?;
			let c = std::ffi::CString::new(candidate.as_os_str().as_encoded_bytes()).ok()?;
			// SAFETY: access(2) on a NUL-terminated path.
			let runnable = unsafe { libc::access(c.as_ptr(), libc::X_OK) } == 0;
			(runnable && !meta.is_dir()).then_some(candidate)
		})
	}
}

/// `rnx-final stage fixed-route ...`: inside a final, the stage is first
/// bound to its context, before any child starts.
pub fn main(args: &[String], root: &Path, env: &Environment) -> Result<i32> {
	let cwd = std::env::current_dir()?;
	let o = parse(args, &cwd)?;
	let mut full = vec!["fixed-route".to_string()];
	full.extend(args.iter().cloned());
	crate::stage::bound(&full, &o.out, env)?;
	let groups = match env.get("RNX_FINAL_GROUPS").filter(|g| !g.is_empty()) {
		Some(g) => PathBuf::from(g),
		None => {
			let g = std::env::temp_dir().join(format!(
				"rnx-final-fixed-route-groups-{}",
				crate::receipt::invocation_id()
			));
			std::fs::create_dir(&g)?;
			g
		}
	};
	let standalone = env.get(crate::child::CONTEXT).is_none_or(|c| c.is_empty());
	let mut live = Live {
		guardian: std::fs::canonicalize("/proc/self/exe")?,
		groups: groups.clone(),
		env: env.clone(),
		stop: if standalone {
			Some(crate::cross::interruptible()?)
		} else {
			None
		},
	};
	let result = gate(&o, root, env, &mut live);
	if env.get("RNX_FINAL_GROUPS").is_none_or(|g| g.is_empty()) {
		let _ = std::fs::remove_dir_all(&groups);
	}
	result
}
