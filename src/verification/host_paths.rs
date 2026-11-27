//! Exercise the actual installed file context in process, with a new store,
//! filesystem and instruction allowance for every invocation.
mod engines;
mod inputs;

use super::{Result, write_new};
use engines::Engines;
use serde_json::{Value, json};
use std::{
	collections::{BTreeMap, BTreeSet},
	env, fs,
	path::Path,
	process::{Command, Stdio},
	time::Instant,
};

struct Probe {
	hash: rune::Hash,
	path: String,
	arguments: Vec<String>,
	awaited: bool,
}
impl Probe {
	// A reduction of an open range uses the small fuel control from the
	// original survey. Finite programs retain the normal file allowance.
	fn fuel(&self, args: &[String]) -> usize {
		let method = self.path.rsplit_once(">::").map(|(_, s)| s).unwrap_or("");
		let method = method.split("::<").next().unwrap();
		if self.path.contains("::RangeFromIter::<")
			&& matches!(
				method,
				"count"
					| "sum" | "product"
					| "collect" | "reduce"
					| "fold" | "last"
					| "for_each" | "min"
					| "max"
			) && args.first().is_some_and(|s| s.contains("..).iter()"))
		{
			2048
		} else {
			crate::runner::BUDGET
		}
	}
	fn source(&self, args: &[String], discovery: bool) -> String {
		let prefix = if self.awaited { "async " } else { "" };
		let suffix = if self.awaited { ".await" } else { "" };
		let call = format!("{}({}){suffix};", self.path, args.join(","));
		let body = if discovery {
			format!("if args.len()==999 {{{call}}}")
		} else {
			call
		};
		format!("struct Probe {{n}}\nfn probe_fn(x){{x}}\npub {prefix}fn main(args){{\n{body}\n}}")
	}
}

fn registry(engines: &Engines) -> Result<Vec<Probe>> {
	let hints = crate::routing_inventory::host_path_arguments();
	let mut rows = BTreeMap::new();
	let mut unique = BTreeSet::new();
	engines
		.context
		.visit_host_path_controls(|hash, mut path, owner, signature, mut awaited| {
			if !path.starts_with("::std::") {
				path = path.trim_start_matches("::").to_owned();
			}
			let method = path
				.strip_prefix(owner.as_deref().unwrap_or(""))
				.unwrap_or(&path)
				.trim_start_matches("::")
				.split("::<")
				.next()
				.unwrap()
				.rsplit("::")
				.next()
				.unwrap();
			let args = if let Some((args, is_async)) = hints.get(path.as_str()) {
				awaited |= is_async;
				if args.is_empty() {
					Vec::new()
				} else {
					args.split(',').map(str::to_owned).collect()
				}
			} else {
				inputs::arguments(method, owner.as_deref(), signature)
			};
			assert!(
				unique.insert(path.clone()),
				"duplicate generated path {path}"
			);
			assert!(
				rows.insert(
					hash,
					Probe {
						hash,
						path,
						arguments: args,
						awaited
					}
				)
				.is_none(),
				"duplicate installed hash {hash}"
			);
		});
	if rows.is_empty() {
		return Err("empty installed host registry".into());
	}
	Ok(rows.into_values().collect())
}

// The worker owns its cwd; none of the stage controller's threads change cwd.
fn isolated<T>(root: &Path, run: impl FnOnce() -> Result<T>) -> Result<T> {
	let directory = root.join("invocation");
	fs::create_dir(&directory)?;
	env::set_current_dir(&directory)?;
	let result = run();
	env::set_current_dir(root)?;
	fs::remove_dir_all(directory)?;
	result
}

// process::exit may terminate the whole observer. Only its cells use a
// disposable worker; a normal language failure is reported through the file.
pub(super) fn exit_child(source: &str, mode: &str, output: &Path) -> Result<()> {
	let mut engines = Engines::new()?;
	crate::host::running_a_script();
	let ok = if mode == "stock" {
		engines.stock(source, crate::runner::BUDGET)?
	} else {
		let (program, entry) = engines
			.compile(source)?
			.ok_or("admitted exit probe refused")?;
		engines.execution(&program, entry, mode == "on", crate::runner::BUDGET)?
	};
	write_new(output, &json!(ok))
}
fn exit_status(root: &Path, source: &str, mode: &str) -> Result<bool> {
	let output = root.join("exit-outcome.json");
	let status = Command::new(env::current_exe()?)
		.args(["--child", "host-path-exit", source, mode])
		.arg(&output)
		.status()?;
	if output.exists() {
		if !status.success() {
			return Err(format!("exit worker failed: {status}").into());
		}
		let ok = serde_json::from_slice(&fs::read(&output)?)?;
		fs::remove_file(output)?;
		return Ok(ok);
	}
	match status.code() {
		Some(0) => Ok(true),
		// The only nonzero integral input in this fixture is one.
		Some(1)
			if source.contains("process::exit(1);") || source.contains("process::exit(1u64);") =>
		{
			Ok(false)
		}
		_ => Err(format!("exit worker terminated without a verdict: {status}, {source}").into()),
	}
}

pub(super) fn sweep(root: &Path, receipt: &Path) -> Result<()> {
	let result = compare_all(root, receipt);
	if let Err(error) = &result
		&& !receipt.exists()
	{
		write_new(receipt, &json!({"status":"FAIL","error":error.to_string()}))?;
	}
	result
}

fn compare_all(root: &Path, receipt: &Path) -> Result<()> {
	let started = Instant::now();
	let root = fs::canonicalize(root)?;
	let mut engines = Engines::new()?;
	let kinds = inputs::kinds()?;
	let rows = registry(&engines)?;
	let mut positions = 0usize;
	let mut probed_positions = 0usize;
	let mut qualified = BTreeMap::new();
	let mut admitted = 0usize;
	let mut compared = 0usize;
	let mut refused = 0usize;
	let mut failing = BTreeSet::new();
	let mut routes = Vec::new();
	for row in &rows {
		let discovery = row.source(&row.arguments, true);
		qualified.insert(row.hash.to_string(), row.path.as_str());
		if engines.compile(&discovery)?.is_some()
			&& isolated(&root, || engines.stock(&discovery, 2048))?
		{
			admitted += 1;
			routes.push(row.hash.to_string());
			positions += row.arguments.len();
		}
		// A different argument can supply proof that the discovery source lacked.
		probed_positions += row.arguments.len();
		let substitutions = (0..row.arguments.len()).flat_map(|slot| {
			kinds
				.iter()
				.map(move |(kind, expr)| (Some(slot), kind.as_str(), *expr))
		});
		let no_arguments = row
			.arguments
			.is_empty()
			.then_some((None, "NoArguments", ""));
		for (slot, kind, expr) in substitutions.chain(no_arguments) {
			let mut args = row.arguments.clone();
			if let Some(slot) = slot {
				args[slot] = expr.to_owned();
			}
			let text = row.source(&args, false);
			let fuel = row.fuel(&args);
			let Some((program, mode)) = engines.compile(&text)? else {
				refused += 1;
				continue;
			};
			compared += 1;
			let exit = row.path == "process::exit";
			let stock = isolated(&root, || {
				if exit {
					exit_status(&root, &text, "stock")
				} else {
					engines.stock(&text, fuel)
				}
			})?;
			for native in [false, true] {
				let success = isolated(&root, || {
					if exit {
						exit_status(&root, &text, if native { "on" } else { "off" })
					} else {
						engines.execution(&program, mode, native, fuel)
					}
				})?;
				if stock && !success {
					let i = slot.map_or_else(|| "-".into(), |slot| slot.to_string());
					failing.insert(format!(
						"{}\t{}\t{i}\t{kind}\t{}",
						row.hash,
						row.path,
						if native { "on" } else { "off" }
					));
				}
			}
		}
	}
	let mut expected = BTreeSet::new();
	let mut groups = BTreeMap::new();
	for (group, table) in [
		("original", include_str!("host_paths/deferred.tsv")),
		(
			"empty_iterator",
			include_str!("host_paths/empty-iterators.tsv"),
		),
	] {
		let previous = expected.len();
		let mut pins = table.lines();
		if pins.next() != Some("handler\tpath\targument\tinput kinds") {
			return Err("invalid deferred host path header".into());
		}
		for line in pins {
			let mut fields = line.rsplitn(2, '\t');
			let kinds = fields.next().ok_or("missing deferred inputs")?;
			let path = fields.next().ok_or("missing deferred path")?;
			if kinds.is_empty() {
				return Err("empty deferred inputs".into());
			}
			for kind in kinds.split_whitespace() {
				for mode in ["off", "on"] {
					if !expected.insert(format!("{path}\t{kind}\t{mode}")) {
						return Err(format!("duplicate deferred host case: {line}").into());
					}
				}
			}
		}
		groups.insert(group, (expected.len() - previous) / 2);
	}
	let unexpected: Vec<_> = failing.difference(&expected).collect();
	let repaired: Vec<_> = expected.difference(&failing).collect();
	let pass = unexpected.is_empty() && repaired.is_empty();
	write_new(
		receipt,
		&json!({"status":if pass {"PASS"} else {"FAIL"}, "registry":rows.len(),"admitted":admitted,"positions":positions,"probed_positions":probed_positions,"identities":qualified,"kinds":kinds.keys().collect::<Vec<_>>(),"compared":compared,"refused":refused,"routes":routes,"pinned":expected.len()/2,"groups":groups,"unexpected":unexpected,"repaired":repaired,"elapsed_seconds":started.elapsed().as_secs_f64()}),
	)?;
	if !pass {
		return Err(format!("host path sweep differs: {}", receipt.display()).into());
	}
	Ok(())
}
pub(super) fn run(output: &Path) -> Result<Value> {
	let output = fs::canonicalize(output)?;
	let work = output.join("host-path-work");
	fs::create_dir(&work)?;
	let receipt = output.join("HOST-PATHS.json");
	let status = Command::new(env::current_exe()?)
		.args(["--child", "host-paths"])
		.arg(&work)
		.arg(&receipt)
		.current_dir(&work)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null())
		.status();
	fs::remove_dir_all(&work)?;
	if !status?.success() {
		return Err(format!("host path control failed: {}", receipt.display()).into());
	}
	Ok(serde_json::from_slice(&fs::read(receipt)?)?)
}
