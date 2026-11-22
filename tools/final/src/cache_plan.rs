//! The live catalogue, not the subset selected for today's run, owns the lanes.
use super::*;
use crate::{catalogue, failure::Environment, pyjson, scheduler, stage};
use serde_json::json;
use std::ffi::OsString;
use std::io::{BufReader, Write};
use std::process::{Command, Stdio};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
	pub target: PathBuf,
	#[serde(default)]
	group: PathBuf,
	recipe: String,
	#[serde(default)]
	disable_incremental: bool,
	mark: PathBuf,
}
type Routes = BTreeMap<String, Route>;

// The matched adapter and upstream edits in ADR 0323 justified keeping
// Cargo's declared incremental settings in these two test configurations.
fn keep_incremental(stage: &str) -> bool {
	matches!(stage, "adapter-polars" | "suite-adapter-candle")
}

fn recipe_key(argv: &[String], baseline: bool) -> Result<String> {
	let compile: Vec<_> = argv
		.iter()
		.take_while(|a| a.as_str() != "--")
		.filter(|a| !a.starts_with("--message-format="))
		.collect();
	pyjson::identity(&(compile, baseline))
}

fn writer(args: &[String]) -> bool {
	args.first()
		.is_some_and(|p| Path::new(p).file_name() == Some("cargo".as_ref()))
		&& !matches!(scheduler::cargo_verb(args), "fetch" | "fmt" | "metadata")
}

/// Use the target directory the writer shared before cache ownership. A
/// differential lint's archived source has a separate published target.
fn sharing_group(
	root: &Path,
	targets: &scheduler::Targets,
	stage: &str,
	cmd: &[String],
	published: bool,
) -> Result<PathBuf> {
	let directory = match stage {
		"commit-discipline" => PathBuf::from("target/execute-guard"),
		"clippy" | "adapter-clippy" => {
			let adapter = if stage == "adapter-clippy" {
				"adapter-"
			} else {
				""
			};
			let role = if published { "published" } else { "candidate" };
			PathBuf::from(format!("target/column-clippy-{adapter}{role}"))
		}
		_ => {
			let target = targets.resolve(scheduler::manifest(cmd))?;
			let relative = target
				.strip_prefix(root)
				.context("external Cargo target directory")?;
			if published {
				PathBuf::from("baseline").join(relative)
			} else {
				relative.to_owned()
			}
		}
	};
	let mut group = directory;
	for args in cmd.windows(2).take_while(|a| a[0] != "--") {
		if args[0] == "--target" {
			group.push(&args[1]);
		}
	}
	Ok(group.join(scheduler::profile(cmd)))
}

pub struct CachePlan {
	root: PathBuf,
	out: PathBuf,
	routes: BTreeMap<String, Routes>,
	lanes: BTreeMap<PathBuf, Lane>,
}

impl CachePlan {
	pub fn prepare(root: &Path, out: &Path, complete: bool) -> Result<Self> {
		let base = claim(&root.join("target/final-caches"))?;
		replay(&base)?;
		let registry = base.join("lanes.json");
		let prior: BTreeSet<PathBuf> = if registry.exists() {
			serde_json::from_slice(&fs::read(&registry)?)?
		} else {
			BTreeSet::new()
		};
		let mut lanes = BTreeMap::new();
		let mut routes = BTreeMap::new();
		let targets = scheduler::Targets::new(root);
		let stage_dir = out.join("cache-recipes");
		fs::create_dir(&stage_dir)?;
		for (name, args) in catalogue::commands(out, root)? {
			let mut declarations = Vec::new();
			if writer(&args) {
				declarations.push((args.clone(), false));
			} else if name == "fixed-route" {
				declarations.push((
					[
						"cargo",
						"+1.98.1",
						"build",
						"--locked",
						"--manifest-path",
						"ci/execute_guard/Cargo.toml",
					]
					.map(String::from)
					.to_vec(),
					false,
				));
			} else if name == "commit-discipline" {
				declarations.push((
					[
						"cargo",
						"run",
						"--locked",
						"--quiet",
						"--manifest-path",
						"ci/execute_guard/Cargo.toml",
					]
					.map(String::from)
					.to_vec(),
					false,
				));
			} else {
				match args.get(2).map(String::as_str) {
					Some("suite") => {
						for cmd in catalogue::recipes(&name, root)?
							.into_iter()
							.filter(|c| writer(c))
						{
							if scheduler::cargo_verb(&cmd) == "clippy"
								&& catalogue::bulk_roots(&name).contains(&catalogue::Bulk::Baseline)
							{
								declarations.push((cmd.clone(), true));
							}
							declarations.push((cmd, false));
						}
					}
					Some("clippy") => {
						let cmd = stage::clippy_command(name == "adapter-clippy");
						declarations.extend([(cmd.clone(), true), (cmd, false)]);
					}
					Some("jupyter") => declarations.extend(
						stage::jupyter_children("", root)
							.into_iter()
							.filter(|c| writer(c))
							.map(|c| (c, false)),
					),
					_ => {}
				}
			}
			let mut stage_routes = Routes::new();
			for (cmd, published) in declarations {
				let key = recipe_key(&cmd, published)?;
				let disable_incremental = !keep_incremental(&name);
				let group = sharing_group(root, &targets, &name, &cmd, published)?;
				let target = base.join(pyjson::identity(&group)?);
				if !lanes.contains_key(&target) {
					claim(&target)?;
					lanes.insert(target.clone(), Lane::open(&target)?);
				}
				let recipe = format!("{name}-{key}");
				stage_routes.insert(
					key,
					Route {
						group,
						target,
						disable_incremental,
						mark: stage_dir.join(format!("{recipe}.marks")),
						recipe,
					},
				);
			}
			if !stage_routes.is_empty() {
				atomic_json(&stage_dir.join(format!("{name}.json")), &stage_routes)?;
				routes.insert(name, stage_routes);
			}
		}
		reconcile(&base, &registry, &prior, &lanes, complete)?;
		Ok(Self {
			root: root.into(),
			out: out.into(),
			routes,
			lanes,
		})
	}

	pub fn bootstrap(&mut self) -> Result<()> {
		let lane = Lane::bootstrap(&self.root)?;
		let target = lane.path.clone();
		let source = File::open(target.join("bootstrap-build.jsonl"))?;
		let mark = self.out.join("cache-recipes/bootstrap.marks");
		if let Some(marks) = stream_marks(&target, BufReader::new(source))? {
			atomic_json(&mark, &marks)?;
		}
		self.routes.insert(
			"bootstrap".into(),
			BTreeMap::from([(
				"bootstrap".into(),
				Route {
					group: PathBuf::from("target/final-controller/release"),
					target: target.clone(),
					recipe: "bootstrap".into(),
					mark,
					disable_incremental: true,
				},
			)]),
		);
		self.lanes.insert(target, lane);
		Ok(())
	}

	pub fn targets(&self, stage: &str) -> BTreeSet<String> {
		self.routes
			.get(stage)
			.into_iter()
			.flat_map(|r| r.values())
			.map(|r| r.target.display().to_string())
			.collect()
	}

	/// Admission estimates in GiB. The measured adapter treatments share
	/// their targets. Other published targets use their observed footprint
	/// rounded up; a group without an observation keeps the cold default.
	pub fn cold_growth(&self, target: &str) -> u64 {
		let group = self
			.routes
			.values()
			.flat_map(|routes| routes.values())
			.find(|r| r.target == Path::new(target))
			.and_then(|r| r.group.to_str());
		let gib = match group {
			Some("adapters/polars/target/debug") => 49,
			Some("adapters/candle/target/debug") => 15,
			Some("adapters/file-provider/target/debug") => 25,
			Some("target/engine-dev") => 22,
			Some("target/debug") => 12,
			Some(
				"target/column-clippy-adapter-candidate/debug"
				| "target/column-clippy-adapter-published/debug"
				| "rune/hegel_tests/target/debug",
			) => 5,
			Some(
				"adapters/postgres/target/debug"
				| "adapters/polars/target/release"
				| "demos/candle/target/debug"
				| "demos/polars/target/debug",
			) => 3,
			Some("rune/tools/xtask/measure/harness/target/debug") => 2,
			Some(
				"ci/execute_guard/target/debug"
				| "target/execute-guard/debug"
				| "rune/tools/xtask/column/probe/target/debug"
				| "target/release"
				| "tools/polars-gen/target/debug"
				| "tools/project/target/debug"
				| "jupyter/target/debug"
				| "servers/http-postgres/target/debug"
				| "target/column-clippy-candidate/debug"
				| "target/column-clippy-published/debug",
			) => 1,
			_ => return scheduler::COLD_BYTES,
		};
		gib * scheduler::GIB
	}

	/// Leftover bytes after a refused or interrupted first build do not
	/// establish warmth, but already occupy part of the estimated cold footprint.
	pub fn growth(&self, resource: &str, stages: &[(String, Vec<String>)]) -> Result<u64> {
		let Some(lane) = self.lanes.get(Path::new(resource)) else {
			return Ok(scheduler::growth(resource, &self.root));
		};
		let cold = self.cold_growth(resource);
		let ledger = lane.ledger()?;
		for (stage, _) in stages {
			for route in self
				.routes
				.get(stage)
				.into_iter()
				.flat_map(|rs| rs.values())
				.filter(|r| r.target == lane.path)
			{
				if !ledger
					.recipes
					.get(&route.recipe)
					.is_some_and(|marks| !marks.is_empty())
				{
					let allocated = lane.allocated_bytes(|_, _| {})?;
					return Ok(cold
						.saturating_sub(allocated)
						.max(scheduler::WARM_GROWTH_BYTES));
				}
			}
		}
		Ok(scheduler::growth_with_cold(resource, &self.root, cold))
	}

	pub fn environment(&self, stage: &str, env: &mut Environment) -> Result<()> {
		let file = |stage: &str| self.out.join("cache-recipes").join(format!("{stage}.json"));
		if self.routes.contains_key(stage) && !matches!(stage, "fixed-route" | "commit-discipline")
		{
			env.insert(
				"RNX_FINAL_CACHE_ROUTES".into(),
				file(stage).display().to_string(),
			);
			env.insert(
				"RNX_FINAL_CACHE_CHECKOUT".into(),
				self.root.display().to_string(),
			);
		}
		if stage == "production-cli-controls" {
			let producer = self.routes["production-cli-build"]
				.values()
				.next()
				.context("missing production binary")?;
			env.insert(
				"RNX_FINAL_PRODUCTION_BINARY".into(),
				producer.binary("rnx-polars")?.display().to_string(),
			);
		}
		if matches!(stage, "commit-discipline" | "fixed-route") {
			let producer = self.routes[stage]
				.values()
				.next()
				.context("missing execute-guard recipe")?;
			env.insert(
				"RNX_FINAL_GUARD_ROUTE".into(),
				serde_json::to_string(producer)?,
			);
			env.insert(
				"RNX_FINAL_GUARD_CONTROLLER".into(),
				std::env::current_exe()?.display().to_string(),
			);
		}
		Ok(())
	}

	pub fn transaction(&self, complete: bool) -> Result<String> {
		let mut rows = BTreeMap::new();
		for (path, lane) in &self.lanes {
			let mut changes = Changes::default();
			for route in self
				.routes
				.values()
				.flat_map(|rs| rs.values())
				.filter(|r| &r.target == path)
			{
				changes.live.insert(route.recipe.clone());
				if route.mark.try_exists()? {
					changes.updates.insert(
						route.recipe.clone(),
						serde_json::from_slice(&fs::read(&route.mark)?)?,
					);
				}
			}
			if complete {
				changes.verify(lane)?;
			} else if changes.updates.is_empty() {
				continue;
			}
			rows.insert(path.clone(), changes);
		}
		let transaction = self.out.join("CACHE.json");
		atomic_json(&transaction, &Transaction { complete, rows })?;
		let sha = crate::receipt::digest(&transaction)?;
		atomic_json(
			&self.root.join("target/final-caches/pending.json"),
			&(&self.out, &sha),
		)?;
		Ok(sha)
	}

	pub fn finish(self) -> Result<()> {
		let root = self.root.join("target/final-caches");
		drop(self); // Release this invocation's leases before journal replay.
		replay(&root)
	}
}

fn reconcile(
	base: &Path,
	registry: &Path,
	prior: &BTreeSet<PathBuf>,
	lanes: &BTreeMap<PathBuf, Lane>,
	complete: bool,
) -> Result<()> {
	if complete {
		for lane in lanes.values() {
			lane.reclaim_committed()?;
		}
	}
	let mut registered: BTreeSet<_> = lanes.keys().cloned().collect();
	if !complete {
		registered.extend(prior.iter().cloned());
		return atomic_json(registry, &registered);
	}
	for path in prior.iter().filter(|p| !lanes.contains_key(*p)) {
		ensure!(path.parent() == Some(base), "lane outside registry");
		if path.try_exists()? {
			let _lease = Lane::open(path)?;
			fs::remove_dir_all(path)?;
			sync_directory(base)?;
		}
	}
	atomic_json(registry, &registered)?;
	Ok(())
}

#[derive(Default, Serialize, Deserialize)]
struct Changes {
	live: BTreeSet<String>,
	updates: BTreeMap<String, Marks>,
}
impl Changes {
	/// Before authorizing a complete sweep, every path the live union
	/// retains must still be the output at least one producer observed.
	fn verify(&self, lane: &Lane) -> Result<()> {
		let ledger = lane.ledger()?;
		let mut expected = BTreeMap::<&Path, Vec<&Artifact>>::new();
		for recipe in &self.live {
			if let Some(marks) = self
				.updates
				.get(recipe)
				.filter(|m| !m.is_empty())
				.or_else(|| ledger.recipes.get(recipe))
			{
				for (path, stamp) in marks {
					expected.entry(path).or_default().push(stamp);
				}
			}
		}
		for (rel, stamps) in expected {
			let path = lane.path.join(rel);
			ensure!(
				path.starts_with(&lane.path)
					&& path.canonicalize()? == path
					&& stamps
						.iter()
						.any(|stamp| matches_file(stamp, &path).unwrap_or(false)),
				"retained Cargo output lost its producer binding: {path:?}"
			);
		}
		Ok(())
	}
}

#[derive(Serialize, Deserialize)]
struct Transaction {
	complete: bool,
	rows: BTreeMap<PathBuf, Changes>,
}

/// Only a terminal PASS bound to these exact marks authorizes the sweep.
/// The driver lock serializes replay with the next final's admission.
fn replay(base: &Path) -> Result<()> {
	let journal = base.join("pending.json");
	if !journal.try_exists()? {
		return Ok(());
	}
	let (out, hash): (PathBuf, String) = serde_json::from_slice(&fs::read(&journal)?)?;
	let receipt = out.join("RESULT.json");
	if receipt.try_exists()? {
		let result: Value = serde_json::from_slice(&fs::read(&receipt)?)?;
		if result["status"] == "PASS" {
			let path = out.join("CACHE.json");
			ensure!(
				result["records"]["CACHE"] == hash && crate::receipt::digest(&path)? == hash,
				"cache transaction differs from successful receipt"
			);
			let txn: Transaction = serde_json::from_slice(&fs::read(path)?)?;
			let started = std::time::Instant::now();
			let mut report = BTreeMap::new();
			for (path, changes) in txn.rows {
				let root = base
					.parent()
					.and_then(Path::parent)
					.context("invalid cache root")?;
				let lane = if path == root.join("target/final-controller") {
					Lane::bootstrap(root)?
				} else {
					ensure!(path.parent() == Some(base), "lane outside cache root");
					Lane::open(&path)?
				};
				let reclaimed = lane.commit(changes.updates, &changes.live, txn.complete)?;
				let footprint = lane.footprint()?;
				report.insert(
					path,
					json!({"footprint":footprint,"reclaimed_bytes":reclaimed,"recipes":changes.live}),
				);
			}
			let footprint = out.join("CACHE-FOOTPRINT.json");
			atomic_json(&footprint, &report)?;
			atomic_json(
				&out.join("CACHE-RELEASE.json"),
				&json!({"transaction":hash,"result":crate::receipt::digest(&receipt)?,"footprint":crate::receipt::digest(&footprint)?, "seconds":started.elapsed().as_secs_f64()}),
			)?;
		}
	}
	fs::remove_file(journal)?;
	sync_directory(base)
}

pub fn lookup(env: &Environment, cwd: &Path, args: &[String]) -> Result<Option<Route>> {
	if !writer(args) {
		return Ok(None);
	}
	let Some(file) = env.get("RNX_FINAL_CACHE_ROUTES") else {
		return Ok(None);
	};
	let routes: Routes = serde_json::from_slice(&fs::read(file)?)?;
	let key = recipe_key(
		args,
		env.get("RNX_FINAL_CACHE_CHECKOUT").map(Path::new) != Some(cwd),
	)?;
	Ok(Some(
		routes
			.get(&key)
			.with_context(|| format!("undeclared Cargo writer {args:?} in {}", cwd.display()))?
			.clone(),
	))
}

impl Route {
	pub fn binary(&self, name: &str) -> Result<PathBuf> {
		self.product(Some(name))
	}

	pub fn sole_binary(&self) -> Result<PathBuf> {
		self.product(None)
	}

	fn product(&self, name: Option<&str>) -> Result<PathBuf> {
		let products: BTreeMap<String, (PathBuf, Artifact)> =
			serde_json::from_slice(&fs::read(self.mark.with_extension("products"))?)?;
		let (path, stamp) = match name {
			Some(name) => products
				.get(name)
				.context("Cargo did not report the requested binary")?,
			None => {
				ensure!(products.len() == 1, "expected one Cargo executable");
				products.first_key_value().unwrap().1
			}
		};
		ensure!(
			path.starts_with(&self.target)
				&& path.canonicalize()? == *path
				&& Artifact::read(path)? == *stamp,
			"Cargo product changed"
		);
		Ok(path.clone())
	}

	pub fn wrap(&self, exe: &Path, argv: &[String]) -> Result<Vec<String>> {
		let mut args = vec![
			exe.display().to_string(),
			"cached-cargo".into(),
			serde_json::to_string(self)?,
		];
		args.extend_from_slice(argv);
		Ok(args)
	}
}

/// Only Cargo's stdout up to build-finished is a message stream. Preserve
/// the original JSON consumer, or render diagnostics and pass ordinary
/// test/program stdout through to the original parser.
pub fn cargo_main(args: &[OsString]) -> Result<i32> {
	let (raw, argv) = args.split_first().context("missing Cargo route")?;
	let route: Route = serde_json::from_str(raw.to_str().context("invalid Cargo route")?)?;
	check_owner(&route.target)?;
	ensure!(
		Path::new(&route.recipe)
			.file_name()
			.and_then(|s| s.to_str())
			== Some(&route.recipe)
			&& !route.recipe.starts_with('.'),
		"invalid recipe name"
	);
	let directory = route.mark.parent().context("missing recipe directory")?;
	ensure!(
		directory.canonicalize()? == directory
			&& directory.file_name() == Some("cache-recipes".as_ref())
			&& route.mark == directory.join(format!("{}.marks", route.recipe)),
		"invalid Cargo mark path"
	);
	let (binary, tail) = argv.split_first().context("empty Cargo command")?;
	ensure!(
		Path::new(binary).file_name() == Some("cargo".as_ref()),
		"expected cargo writer"
	);
	let mut cmd = Command::new(binary);
	let split = tail.iter().position(|a| a == "--").unwrap_or(tail.len());
	let raw_json = tail[..split].iter().any(|a| a == "--message-format=json");
	cmd.args(&tail[..split]);
	if !raw_json {
		cmd.arg("--message-format=json");
	}

	cmd.arg("--config")
		.arg(format!(
			"build.build-dir={}",
			serde_json::to_string(&route.target)?
		))
		.arg("--target-dir")
		.arg(&route.target)
		.args(&tail[split..])
		.env_remove("CARGO_TARGET_DIR")
		.env_remove("CARGO_BUILD_BUILD_DIR")
		.env_remove("RNX_FINAL_CACHE_ROUTES")
		.env_remove("RNX_FINAL_CACHE_CHECKOUT")
		.env_remove("RNX_FINAL_GUARD_ROUTE")
		.env_remove("RNX_FINAL_GUARD_CONTROLLER")
		.env_remove("CARGO_INCREMENTAL");
	if route.disable_incremental {
		cmd.env("CARGO_INCREMENTAL", "0");
	}
	let mut marks = Marks::new();
	let mut products = BTreeMap::new();
	let mut buf = Vec::new();
	let mut build = None;
	let mut complete_outputs = true;
	// Discovery and execution may share a recipe. Only the last complete
	// invocation may replace its prior committed marks.
	for path in [&route.mark, &route.mark.with_extension("products")] {
		match fs::remove_file(path) {
			Ok(()) => {}
			Err(e) if e.kind() == ErrorKind::NotFound => {}
			Err(e) => return Err(e.into()),
		}
	}

	// Reserve the lane against sweeping until a complete recipe report
	// supersedes this invocation. This survives SIGKILL before Cargo flushes.
	let incomplete = route.target.join(".rnx-incomplete");
	fs::create_dir_all(&incomplete)?;
	atomic_json(&incomplete.join(&route.recipe), &true)?;
	let observations = route.target.join(".rnx-observed");
	fs::create_dir_all(&observations)?;
	let journal = observations.join(&route.recipe);
	let mut observed: Marks = if journal.try_exists()? {
		serde_json::from_slice(&fs::read(&journal)?)?
	} else {
		Marks::new()
	};
	let mut proc = cmd.stdout(Stdio::piped()).spawn()?;
	let mut reader = BufReader::new(proc.stdout.take().context("missing Cargo stdout")?);
	let mut stdout = std::io::stdout().lock();
	let stream = (|| -> Result<()> {
		loop {
			buf.clear();
			if reader.read_until(b'\n', &mut buf)? == 0 {
				break;
			}
			if build.is_none() && buf.starts_with(b"{") {
				let msg: Value = serde_json::from_slice(&buf)?;
				if msg["reason"] == "compiler-artifact" {
					if msg["target"]["kind"] == json!(["bin"])
						&& msg["profile"]["test"] == false
						&& let Some(exe) = msg["executable"].as_str()
					{
						let path = Path::new(exe);
						ensure!(
							path.starts_with(&route.target) && path.canonicalize()? == path,
							"binary outside the Cargo lane"
						);
						products.insert(
							msg["target"]["name"]
								.as_str()
								.context("binary without a name")?
								.to_owned(),
							(path.to_owned(), Artifact::read(path)?),
						);
					}
					complete_outputs &= record_outputs(&route.target, &msg, &mut marks)?;
				} else if msg["reason"] == "build-finished" {
					build = Some(msg["success"] == true);
				} else if !raw_json && let Some(rendered) = msg["message"]["rendered"].as_str() {
					std::io::stderr().write_all(rendered.as_bytes())?;
				}
				if !raw_json && msg["reason"].is_string() {
					continue;
				}
			}
			stdout.write_all(&buf)?;
		}
		Ok(())
	})();
	if stream.is_err() {
		let _ = proc.kill();
	}
	let status = proc.wait()?;
	// Batch the positive evidence once per producer. An interruption before
	// this rename can only leave an unproven file in the cache.
	if !marks.is_empty() {
		observed.extend(marks.clone());
		atomic_json(&journal, &observed)?;
	}
	stream?;
	if status.success() && build == Some(true) {
		if !products.is_empty() {
			atomic_json(&route.mark.with_extension("products"), &products)?;
		}
		if complete_outputs && !marks.is_empty() {
			atomic_json(&route.mark, &marks)?;
		}
	}
	Ok(status.code().unwrap_or(1))
}

#[cfg(test)]
mod controls {
	use super::*;

	struct Fixture(PathBuf);
	impl Fixture {
		fn new() -> Self {
			let p = std::env::temp_dir().join(format!("rnx-final-cache-{}", invocation_id()));
			fs::create_dir(&p).unwrap();
			Self(p)
		}
	}
	impl Drop for Fixture {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}

	#[test]
	fn admission_retires_only_registered_unleased_configurations_and_keeps_unselected_ones() {
		let f = Fixture::new();
		let root = &f.0;
		let registry = root.join("lanes.json");
		let old = claim(&root.join("old")).unwrap();
		fs::write(old.join("old-build"), vec![0xff; 32768]).unwrap();
		let live = claim(&root.join("live")).unwrap();
		let prior = BTreeSet::from([old.clone(), live.clone()]);
		let lanes = BTreeMap::from([(live.clone(), Lane::open(&live).unwrap())]);
		let foreign = root.join("developer");
		fs::create_dir(&foreign).unwrap();

		let free = |_: &Path| -> Result<u64> {
			Ok(crate::controller::DISK_FLOOR_BYTES - u64::from(old.exists()))
		};
		let admission = |name: &str| {
			let out = root.join(name);
			fs::create_dir(&out).unwrap();
			crate::controller::disk_check_with(root, &out, &Environment::new(), &free)
		};
		assert!(admission("before").is_err());
		let lease = Lane::open(&old).unwrap();
		assert!(reconcile(root, &registry, &prior, &lanes, true).is_err());
		assert!(old.join("old-build").is_file());
		let inherited = lease._lease.try_clone().unwrap();
		drop(lease);
		// Only one stage is selected. The caller still supplies every
		// catalogue lane, so the unselected live configuration survives.
		reconcile(root, &registry, &prior, &lanes, false).unwrap();
		assert!(old.is_dir());
		reconcile(root, &registry, &prior, &lanes, true).unwrap();
		assert!(!old.exists());
		admission("after").unwrap();
		drop(inherited);
		assert!(live.is_dir() && foreign.is_dir());
		assert_eq!(
			serde_json::from_slice::<BTreeSet<PathBuf>>(&fs::read(&registry).unwrap()).unwrap(),
			BTreeSet::from([live])
		);
	}

	#[test]
	fn admission_charges_uncommitted_lanes_only_for_remaining_growth() {
		let f = Fixture::new();
		let target = claim(&f.0.join("lane")).unwrap();
		let lane = Lane::open(&target).unwrap();
		let deps = target.join("debug/deps");
		fs::create_dir_all(&deps).unwrap();
		let output = deps.join("libfixture.rlib");
		fs::write(&output, vec![0x55; 65536]).unwrap();
		fs::hard_link(&output, deps.join("alias")).unwrap();
		let sparse = deps.join("sparse");
		File::create(&sparse)
			.unwrap()
			.set_len(2 * scheduler::GIB)
			.unwrap();

		let route = Route {
			group: "unknown/debug".into(),
			target: target.clone(),
			recipe: "producer".into(),
			disable_incremental: true,
			mark: f.0.join("producer.marks"),
		};
		let mut plan = CachePlan {
			root: f.0.clone(),
			out: f.0.clone(),
			routes: BTreeMap::from([("stage".into(), BTreeMap::from([("key".into(), route)]))]),
			lanes: BTreeMap::from([(target.clone(), lane)]),
		};
		let stages = [("stage".into(), Vec::new())];
		let resource = target.to_str().unwrap();
		let allocated: u64 = [
			target.as_path(),
			&target.join(".rnx-cache-owner.json"),
			&target.join("debug"),
			&deps,
			&output,
			&sparse,
		]
		.iter()
		.map(|p| p.symlink_metadata().unwrap().blocks() * 512)
		.sum();
		let cold = plan.cold_growth(resource);
		assert!(allocated > 0 && allocated < cold - scheduler::WARM_GROWTH_BYTES);
		assert_eq!(plan.growth(resource, &stages).unwrap(), cold - allocated);

		// A one-GiB group's remainder is already below the warm floor.
		plan.routes
			.get_mut("stage")
			.unwrap()
			.get_mut("key")
			.unwrap()
			.group = "ci/execute_guard/target/debug".into();
		assert_eq!(
			plan.growth(resource, &stages).unwrap(),
			scheduler::WARM_GROWTH_BYTES
		);
		plan.routes
			.get_mut("stage")
			.unwrap()
			.get_mut("key")
			.unwrap()
			.group = "unknown/debug".into();

		let marks = Marks::from([(
			"debug/deps/libfixture.rlib".into(),
			Artifact::read(&output).unwrap(),
		)]);
		plan.lanes[&target]
			.commit(
				BTreeMap::from([("producer".into(), marks)]),
				&BTreeSet::from(["producer".into()]),
				false,
			)
			.unwrap();
		assert_eq!(
			plan.growth(resource, &stages).unwrap(),
			scheduler::growth_with_cold(resource, &plan.root, cold)
		);
	}

	#[test]
	fn failed_run_cannot_commit_marks_and_a_bound_pass_can_resume() {
		let f = Fixture::new();
		let root = &f.0;
		let base = claim(&root.join("caches")).unwrap();
		let lane = claim(&base.join("config")).unwrap();
		let deps = lane.join("debug/deps");
		fs::create_dir_all(&deps).unwrap();
		let exe = deps.join("first");
		fs::write(&exe, b"old").unwrap();
		let first = Marks::from([(
			PathBuf::from("debug/deps/first"),
			Artifact::read(&exe).unwrap(),
		)]);
		Lane::open(&lane)
			.unwrap()
			.commit(
				BTreeMap::from([("a".into(), first)]),
				&BTreeSet::from(["a".into()]),
				true,
			)
			.unwrap();
		let txn = Transaction {
			complete: true,
			rows: BTreeMap::from([(lane.clone(), Changes::default())]),
		};
		let record = root.join("CACHE.json");
		atomic_json(&record, &txn).unwrap();
		let hash = crate::receipt::digest(&record).unwrap();
		let pending = base.join("pending.json");
		let receipt = root.join("RESULT.json");
		atomic_json(&pending, &(root, &hash)).unwrap();
		atomic_json(&receipt, &json!({"status":"FAIL"})).unwrap();
		replay(&base).unwrap();
		assert!(exe.is_file());
		atomic_json(&pending, &(root, &hash)).unwrap();
		atomic_json(&receipt, &json!({"status":"PASS","records":{"CACHE":hash}})).unwrap();
		replay(&base).unwrap();
		assert!(!exe.exists() && !pending.exists());
		assert!(root.join("CACHE-FOOTPRINT.json").exists());
		assert!(root.join("CACHE-RELEASE.json").exists());
	}

	#[test]
	fn physical_target_and_profile_decide_the_lane() {
		let root = Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("../..")
			.canonicalize()
			.unwrap();
		let targets = scheduler::Targets::new(&root);
		let mut cmd = vec![
			"cargo".into(),
			"test".into(),
			"--manifest-path".into(),
			"adapters/polars/Cargo.toml".into(),
		];
		let group = |stage, cmd: &[String], baseline| {
			sharing_group(&root, &targets, stage, cmd, baseline).unwrap()
		};
		let default = group("adapter-polars", &cmd, false);
		cmd.extend(["--features".into(), "test-support".into()]);
		assert_eq!(default, group("polars-suite", &cmd, false));
		assert_ne!(default, group("clippy-adapter-polars", &cmd, true));
		cmd.push("--release".into());
		assert_ne!(default, group("production-cli-build", &cmd, false));
	}
}
