//! Cargo targets shared by stages within one disposable run.
use crate::receipt::{atomic_json, digest};
use crate::{catalogue, failure::Environment, pyjson, scheduler, stage};
use anyhow::{Context, Result, ensure};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io::{BufRead, ErrorKind};
use std::path::{Path, PathBuf};

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
	pub disable_incremental: bool,
	products: PathBuf,
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

fn declarations(name: &str, args: &[String], root: &Path) -> Result<Vec<(Vec<String>, bool)>> {
	let mut declarations = Vec::new();
	if writer(args) {
		declarations.push((args.to_vec(), false));
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
				for cmd in catalogue::recipes(name, root)?
					.into_iter()
					.filter(|c| writer(c))
				{
					if scheduler::cargo_verb(&cmd) == "clippy"
						&& catalogue::bulk_roots(name).contains(&catalogue::Bulk::Baseline)
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
	Ok(declarations)
}

pub struct BuildTargets {
	root: PathBuf,
	recipes: PathBuf,
	routes: BTreeMap<String, Routes>,
}

impl BuildTargets {
	pub fn prepare(root: &Path, out: &Path) -> Result<Self> {
		let base = root.join("target/final-builds");
		fs::create_dir(&base)?;
		let mut routes = BTreeMap::new();
		let targets = scheduler::Targets::new(root);
		let stage_dir = root.join("target/build-recipes");
		fs::create_dir(&stage_dir)?;
		for (name, args) in catalogue::commands(out, root)? {
			let declarations = declarations(&name, &args, root)?;
			let mut stage_routes = Routes::new();
			for (cmd, published) in declarations {
				let key = recipe_key(&cmd, published)?;
				let disable_incremental = !keep_incremental(&name);
				let group = sharing_group(root, &targets, &name, &cmd, published)?;
				ensure!(
					group.is_relative()
						&& group
							.components()
							.all(|part| matches!(part, std::path::Component::Normal(_))),
					"invalid Cargo sharing group {group:?}"
				);
				let target = base.join(&group);
				fs::create_dir_all(target.parent().context("target group parent")?)?;
				let recipe = format!("{name}-{key}");
				stage_routes.insert(
					key,
					Route {
						group,
						target,
						disable_incremental,
						products: stage_dir.join(format!("{recipe}.products")),
						recipe,
					},
				);
			}
			if !stage_routes.is_empty() {
				atomic_json(&stage_dir.join(format!("{name}.json")), &stage_routes)?;
				routes.insert(name, stage_routes);
			}
		}
		Ok(Self {
			root: root.into(),
			recipes: stage_dir,
			routes,
		})
	}

	pub fn suite_debug_target(&self) -> Option<&Path> {
		self.routes
			.get("rune-suite")?
			.values()
			.find(|route| route.is_root_debug())
			.map(|route| route.target.as_path())
	}

	pub fn targets(&self, stage: &str) -> BTreeSet<String> {
		self.routes
			.get(stage)
			.into_iter()
			.flat_map(|r| r.values())
			.map(|r| r.target.display().to_string())
			.collect()
	}

	pub fn environment(&self, stage: &str, env: &mut Environment) -> Result<()> {
		let file = |stage: &str| self.recipes.join(format!("{stage}.json"));
		if self.routes.contains_key(stage) && !matches!(stage, "fixed-route" | "commit-discipline")
		{
			env.insert(
				"RNX_FINAL_BUILD_ROUTES".into(),
				file(stage).display().to_string(),
			);
			env.insert(
				"RNX_FINAL_BUILD_CHECKOUT".into(),
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
}

pub fn lookup(env: &Environment, cwd: &Path, args: &[String]) -> Result<Option<Route>> {
	if !writer(args) {
		return Ok(None);
	}
	let Some(file) = env.get("RNX_FINAL_BUILD_ROUTES") else {
		return Ok(None);
	};
	let routes: Routes = serde_json::from_slice(&fs::read(file)?)?;
	let key = recipe_key(
		args,
		env.get("RNX_FINAL_BUILD_CHECKOUT").map(Path::new) != Some(cwd),
	)?;
	Ok(Some(
		routes
			.get(&key)
			.with_context(|| format!("undeclared Cargo writer {args:?} in {}", cwd.display()))?
			.clone(),
	))
}

impl Route {
	pub fn is_root_debug(&self) -> bool {
		self.group == Path::new("target/debug")
	}

	pub fn binary(&self, name: &str) -> Result<PathBuf> {
		self.product(Some(name))
	}

	pub fn sole_binary(&self) -> Result<PathBuf> {
		self.product(None)
	}

	fn product(&self, name: Option<&str>) -> Result<PathBuf> {
		let products: BTreeMap<String, (PathBuf, String)> =
			serde_json::from_slice(&fs::read(&self.products)?)?;
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
				&& digest(path)? == *stamp,
			"Cargo product changed"
		);
		Ok(path.clone())
	}

	pub fn wrap(&self, exe: &Path, argv: &[String]) -> Result<Vec<String>> {
		let mut args = vec![
			exe.display().to_string(),
			"build-cargo".into(),
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
	let parent = route.target.parent().context("target parent")?;
	ensure!(
		parent.canonicalize()? == parent,
		"invalid Cargo target parent"
	);
	match fs::symlink_metadata(&route.target) {
		Ok(meta) => ensure!(
			meta.is_dir() && route.target.canonicalize()? == route.target,
			"invalid Cargo build directory"
		),
		Err(error) if error.kind() == ErrorKind::NotFound => {}
		Err(error) => return Err(error.into()),
	}
	ensure!(
		Path::new(&route.recipe)
			.file_name()
			.and_then(|s| s.to_str())
			== Some(&route.recipe)
			&& !route.recipe.starts_with('.'),
		"invalid recipe name"
	);
	let directory = route
		.products
		.parent()
		.context("missing recipe directory")?;
	ensure!(
		directory.canonicalize()? == directory
			&& directory.file_name() == Some("build-recipes".as_ref())
			&& route.products == directory.join(format!("{}.products", route.recipe)),
		"invalid Cargo product path"
	);
	let (binary, tail) = argv.split_first().context("empty Cargo command")?;
	ensure!(
		Path::new(binary).file_name() == Some("cargo".as_ref()),
		"expected cargo writer"
	);
	let mut cmd = Command::new(binary);
	let split = tail.iter().position(|a| a == "--").unwrap_or(tail.len());
	let raw_json = tail[..split]
		.iter()
		.any(|a| a.as_encoded_bytes().starts_with(b"--message-format=json"));
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
		.env_remove("RNX_FINAL_BUILD_ROUTES")
		.env_remove("RNX_FINAL_BUILD_CHECKOUT")
		.env_remove("RNX_FINAL_GUARD_ROUTE")
		.env_remove("RNX_FINAL_GUARD_CONTROLLER")
		.env_remove("CARGO_INCREMENTAL");
	if route.disable_incremental {
		cmd.env("CARGO_INCREMENTAL", "0");
	}
	let mut products = BTreeMap::new();
	let mut buf = Vec::new();
	let mut build = None;
	match fs::remove_file(&route.products) {
		Ok(()) => {}
		Err(e) if e.kind() == ErrorKind::NotFound => {}
		Err(e) => return Err(e.into()),
	}
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
							(path.to_owned(), digest(path)?),
						);
					}
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
	stream?;
	if status.success() {
		ensure!(
			build == Some(true),
			"successful Cargo process lacks build-finished"
		);
		if !products.is_empty() {
			atomic_json(&route.products, &products)?;
		}
	}
	Ok(status.code().unwrap_or(1))
}
