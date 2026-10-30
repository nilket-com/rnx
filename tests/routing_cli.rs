//! Live comparisons of the actual CLI. Expected execution streams are never
//! fixtures. Debug-engine evidence comes from a separate invocation.
#![cfg(unix)]
#[path = "support/routing_adapter.rs"]
mod adapter;
#[path = "../src/verification/clock.rs"]
mod clock;
use serde_json::{Value, json};
use std::{
	fs,
	path::{Path, PathBuf},
	process::{Command, Stdio},
	sync::{
		Mutex,
		atomic::{AtomicUsize, Ordering},
	},
	time::{Instant, SystemTime, UNIX_EPOCH},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

struct Case {
	id: String,
	source: String,
	callable: bool,
	adaptation: String,
	clock: &'static str,
	budget: usize,
}
struct Ran {
	stdout: Vec<u8>,
	stderr: Vec<u8>,
	code: i32,
	before: i64,
	after: i64,
	elapsed: u128,
	files: Value,
}
fn wall() -> i64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_millis()
		.try_into()
		.unwrap()
}
fn hash(path: &Path) -> Result<String> {
	let out = Command::new("sha256sum").arg("--").arg(path).output()?;
	if !out.status.success() {
		return Err("source hashing failed".into());
	}
	Ok(std::str::from_utf8(&out.stdout)?[..64].to_owned())
}
fn child(root: &Path, path: &str) -> Result<PathBuf> {
	let path = Path::new(path);
	if path.is_absolute()
		|| !path
			.components()
			.all(|p| matches!(p, std::path::Component::Normal(_)))
	{
		return Err("unsafe corpus path".into());
	}
	Ok(root.join(path))
}
fn rune_root() -> Result<PathBuf> {
	if let Some(path) = std::env::var_os("RNX_ROUTING_RUNE_ROOT") {
		return Ok(PathBuf::from(path));
	}
	let output = Command::new("cargo")
		.args(["metadata", "--locked", "--offline", "--format-version=1"])
		.current_dir(env!("CARGO_MANIFEST_DIR"))
		.output()?;
	if !output.status.success() {
		return Err("cannot locate the locked Rune source".into());
	}
	let metadata: Value = serde_json::from_slice(&output.stdout)?;
	let package = metadata["packages"]
		.as_array()
		.ok_or("metadata packages missing")?
		.iter()
		.find(|p| p["name"] == "rune")
		.ok_or("Rune package missing")?;
	let manifest = Path::new(
		package["manifest_path"]
			.as_str()
			.ok_or("Rune manifest missing")?,
	);
	Ok(manifest
		.parent()
		.and_then(Path::parent)
		.and_then(Path::parent)
		.ok_or("Rune source layout changed")?
		.to_owned())
}
fn cases() -> Result<Vec<Case>> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let mut cases = Vec::new();
	let manifest: Value = serde_json::from_str(include_str!(
		"../tools/verify/fixtures/census_manifest.json"
	))?;
	for row in manifest["rows"].as_array().ok_or("census rows missing")? {
		let id = row["id"].as_str().ok_or("census id missing")?;
		let path = child(
			&root.join("tools/verify/fixtures"),
			row["input"].as_str().ok_or("census path missing")?,
		)?;
		if hash(&path)?
			!= row["input_sha256"]
				.as_str()
				.ok_or("census digest missing")?
		{
			return Err("census source drift".into());
		}
		let text = fs::read_to_string(path)?;
		let adapted = adapter::adapt(&text, "main", "main");
		cases.push(Case {
			id: format!("census-{id}"),
			source: adapted.source,
			callable: adapted.callable,
			adaptation: adapted.reason.into(),
			clock: match id {
				"allow-23" => "wall",
				"allow-24" => "monotonic",
				_ => "",
			},
			budget: 2_000_000,
		});
	}
	let corpus = rune_root()?.join("tools/xtask/corpus");
	let index = fs::read_to_string(corpus.join("l1-cases.tsv"))?;
	let mut lines = index.lines();
	if lines.next() != Some("id\tsource\tmode\tentry\targs\tsha256") {
		return Err("L1 schema drift".into());
	}
	for line in lines {
		let fields: Vec<_> = line.split('\t').collect();
		if fields.len() != 6 || !fields[4].is_empty() {
			return Err("L1 argument adapter review required".into());
		}
		let path = child(&corpus, fields[1])?;
		if hash(&path)? != fields[5] {
			return Err("L1 source drift".into());
		}
		let unit_clone_source = match fields[0] {
			"01903" | "01966" => {
				Some("419fcb666b3a4753fdb0a01cfd2cc1059022b80f127f457496907e9728c0c501")
			}
			"02213" => Some("0b16eec71a658ff6801cda463e3f22910441e3dfab5992f9942374885cf3e72b"),
			_ => None,
		};
		if unit_clone_source.is_some_and(|expected| fields[5] != expected) {
			return Err("unit-clone policy source changed; a new ruling is required".into());
		}
		let text = fs::read_to_string(path)?;
		let adapted = adapter::adapt(&text, fields[2], fields[3]);
		cases.push(Case {
			id: format!("l1-{}", fields[0]),
			source: adapted.source,
			callable: adapted.callable,
			adaptation: adapted.reason.into(),
			clock: "",
			budget: 2_000_000,
		});
	}
	for name in ["01_mortgage", "02_orders", "03_report"] {
		cases.push(Case {
			id: format!("demo-{name}"),
			source: fs::read_to_string(root.join("demos").join(format!("{name}.rn")))?,
			callable: true,
			adaptation: "original-cli-entry".into(),
			clock: "",
			budget: 2_000_000,
		});
	}
	// The timing controller must never label a fallback as execution. Admit
	// the exact committed kernel bodies here through the same real CLI gate.
	for name in [
		"fib",
		"loops",
		"numeric_calls",
		"shared_rows",
		"awfy_sieve",
		"awfy_permute",
	] {
		let prefix = fs::read_to_string(
			root.join("tools/routing-bench/kernels")
				.join(format!("{name}.rn")),
		)?;
		cases.push(Case {
			id: format!("benchmark-{name}"),
			source: format!("{prefix}\npub fn main(args){{let checksum=0;for i in 0..1{{checksum=(checksum+kernel(i%97+1))%1000000007;}}checksum}}\n"),
			callable: true,
			adaptation: "benchmark-one-round".into(),
			clock: "",
			budget: 2_000_000,
		});
	}
	cases.push(Case {
		id: "control-moved-nominal-std".into(),
		source: "mod namespace{pub mod std{pub struct P{x}pub fn value(){P{x:1}}}}pub fn main(args){let a=namespace::std::value();drop(a);a}".into(),
		callable: true,
		adaptation: "literal-nominal-identity".into(),
		clock: "",
		budget: 2_000_000,
	});
	for (name, body, budget) in [
		("admitted", "let a=[];a.push(a);a", 2_000_000),
		("refused", "json::stringify(#{a:1})", 2_000_000),
		// E1's live edge oracle: legacy reaches Panic at remaining=0,
		// execution reaches the same Panic with remaining work. The CLI
		// differs only by legacy's exhausted-budget annotation.
		("budget-annotation", "panic!(\"terminal\")", 4),
		(
			"terminal",
			"println!(\"once\");panic!(\"terminal\")",
			2_000_000,
		),
		(
			"budget-loop",
			"let n=0;loop{println!(\"effect {}\",n);n+=1;}",
			80,
		),
		(
			"budget-success",
			"let n=0;for i in 0..40{println!(\"effect {}\",i);n+=i;}n",
			80,
		),
		(
			"budget-error",
			"for i in 0..40{println!(\"effect {}\",i);}panic!(\"terminal\")",
			80,
		),
	] {
		cases.push(Case {
			id: format!("control-{name}"),
			source: format!("pub fn main(args){{{body}}}"),
			callable: true,
			adaptation: "control".into(),
			clock: "",
			budget,
		});
	}
	Ok(cases)
}
fn snapshot(root: &Path) -> Result<Value> {
	fn walk(root: &Path, dir: &Path, rows: &mut Vec<Value>) -> Result<()> {
		let mut paths = fs::read_dir(dir)?
			.map(|p| p.map(|p| p.path()))
			.collect::<std::io::Result<Vec<_>>>()?;
		paths.sort();
		for path in paths {
			let metadata = fs::symlink_metadata(&path)?;
			let name = path
				.strip_prefix(root)?
				.to_str()
				.ok_or("non-UTF8 effect path")?;
			if metadata.is_dir() {
				rows.push(json!({"path":name,"kind":"directory"}));
				walk(root, &path, rows)?;
			} else if metadata.is_file() {
				rows.push(json!({"path":name,"kind":"file","bytes":fs::read(&path)?}));
			} else {
				return Err("unreviewed filesystem effect kind".into());
			}
		}
		Ok(())
	}
	let mut rows = Vec::new();
	walk(root, root, &mut rows)?;
	Ok(json!(rows))
}
fn run(
	case: &Case,
	dir: &Path,
	legacy: bool,
	native: bool,
	debug: bool,
	budget: usize,
	label: &str,
) -> Result<Ran> {
	let work = dir.join("work");
	if work.exists() {
		fs::remove_dir_all(&work)?;
	}
	fs::create_dir_all(work.join("demos/data"))?;
	let orders = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("demos/data/orders.json"))?;
	for name in ["demos/data/orders.json", "orders.json", "data/orders.json"] {
		let path = work.join(name);
		fs::create_dir_all(path.parent().unwrap())?;
		fs::write(path, &orders)?;
	}
	let input = dir.join("script.rn");
	fs::write(&input, &case.source)?;
	let mut command = Command::new("/usr/bin/timeout");
	command
		.args([
			"--kill-after=2s",
			"60s",
			"/usr/bin/bwrap",
			"--die-with-parent",
			"--unshare-net",
			"--ro-bind",
			"/",
			"/",
			"--bind",
		])
		.arg(&work)
		.arg(&work)
		.args(["--chdir"])
		.arg(&work)
		.args(["--dev", "/dev", "--proc", "/proc", "--setenv", "HOME"])
		.arg(&work)
		.args(["--setenv", "TMPDIR"])
		.arg(&work)
		.arg(env!("CARGO_BIN_EXE_rnx"))
		.args(["--color=never", "run"])
		.arg(format!("--native={}", if native { "on" } else { "off" }));
	if legacy {
		command.arg("--engine=legacy");
	}
	if debug {
		command.arg("--debug-engine");
	}
	command
		.arg("--budget")
		.arg(budget.to_string())
		.arg(&input)
		.stdin(Stdio::null());
	let before = wall();
	let start = Instant::now();
	let output = command.output()?;
	let elapsed = start.elapsed().as_millis();
	let after = wall();
	fs::write(dir.join(format!("{label}.stdout")), &output.stdout)?;
	fs::write(dir.join(format!("{label}.stderr")), &output.stderr)?;
	let code = output.status.code().ok_or("CLI terminated by a signal")?;
	if code == 124 || code == 137 {
		return Err("CLI exceeded its execution deadline".into());
	}
	let files = snapshot(&work)?;
	fs::write(
		dir.join(format!("{label}.files.json")),
		serde_json::to_vec_pretty(&files)?,
	)?;
	Ok(Ran {
		stdout: output.stdout,
		stderr: output.stderr,
		code,
		before,
		after,
		elapsed,
		files,
	})
}
fn halt<'a>(run: &'a Ran, budget: usize) -> Option<(&'a [u8], bool)> {
	let halt = format!("halted: {budget} work units exceeded; --budget N raises it\n");
	let annotation = format!(
		"halted: the budget of {budget} work units was exhausted at that point; --budget N raises it\n"
	);
	if run.code != 1 {
		return None;
	}
	if let Some(prefix) = run.stderr.strip_suffix(halt.as_bytes()) {
		Some((prefix, false))
	} else {
		run.stderr
			.strip_suffix(annotation.as_bytes())
			.map(|prefix| (prefix, true))
	}
}
fn exact(a: &Ran, b: &Ran) -> bool {
	a.stdout == b.stdout && a.stderr == b.stderr && a.code == b.code && a.files == b.files
}
fn compare(case: &Case, dir: &Path, native: bool) -> Result<Value> {
	let mode = if native { "on" } else { "off" };
	let old = run(
		case,
		dir,
		true,
		native,
		false,
		case.budget,
		&format!("{mode}-legacy"),
	)?;
	let new = run(
		case,
		dir,
		false,
		native,
		false,
		case.budget,
		&format!("{mode}-auto"),
	)?;
	let evidence = run(
		case,
		dir,
		false,
		native,
		true,
		case.budget,
		&format!("{mode}-route"),
	)?;
	let selected = String::from_utf8_lossy(&evidence.stderr)
		.lines()
		.filter(|line| line.starts_with("rnx: engine="))
		.map(str::to_owned)
		.collect::<Vec<_>>();
	if selected.len() != 1 {
		return Err(format!("missing unambiguous route evidence: {}", case.id).into());
	}
	if case.id.starts_with("demo-")
		|| case.id.starts_with("benchmark-")
		|| matches!(
			case.id.as_str(),
			"control-admitted"
				| "control-moved-nominal-std"
				| "control-terminal"
				| "control-budget-loop"
				| "control-budget-success"
				| "control-budget-error"
				| "control-budget-annotation"
		) {
		if !selected[0].starts_with("rnx: engine=execution ") {
			return Err(format!("admitted path did not select execution: {}", case.id).into());
		}
	}
	if case.id == "control-refused" && !selected[0].starts_with("rnx: engine=legacy refusal=") {
		return Err("refused witness did not select legacy before execution".into());
	}

	let mut class = "exact";
	if !case.clock.is_empty() {
		if old.files != new.files {
			return Err(format!("clock witness changed filesystem effects: {}", case.id).into());
		}
		for observed in [&old, &new] {
			let value = std::str::from_utf8(&observed.stdout)?
				.trim()
				.parse::<i64>()
				.ok();
			if observed.code != 0
				|| !observed.stderr.is_empty()
				|| !clock::clock_check(
					case.clock,
					value,
					observed.before,
					observed.after,
					observed.elapsed,
				) {
				return Err(format!("live clock witness failed: {}", case.id).into());
			}
		}
		class = "nondeterministic-clock";
	} else if matches!(case.id.as_str(), "l1-01903" | "l1-01966" | "l1-02213") {
		// Named intentional difference, ruled in rune-routing seq24062. These
		// SHA-pinned L1 sources return unit.clone(); legacy manufactures an
		// owned empty Tuple. Never apply this to a census or demo program.
		if old.code != 0
			|| new.code != 0
			|| old.stdout != b"()\n"
			|| !new.stdout.is_empty()
			|| evidence.code != 0
			|| !evidence.stdout.is_empty()
			|| !old.stderr.is_empty()
			|| !new.stderr.is_empty()
			|| old.files != new.files
			|| !selected[0].starts_with("rnx: engine=execution ")
			|| evidence.stderr
				!= format!("rnx: engine=execution native={mode}\nrnx: result=Unit\n").as_bytes()
		{
			return Err(format!(
				"unit-clone row no longer meets its exact typed policy: {}",
				case.id
			)
			.into());
		}
		class = "unit-clone";
	} else if !exact(&old, &new) {
		let Some((old_prefix, annotation)) = halt(&old, case.budget) else {
			return Err(format!(
				"CLI mismatch where legacy reached a non-budget outcome: {} native={mode}",
				case.id
			)
			.into());
		};
		let larger_budget = case
			.budget
			.checked_mul(64)
			.filter(|n| *n < usize::MAX)
			.ok_or("larger budget overflow")?;
		let larger = run(
			case,
			dir,
			true,
			native,
			false,
			larger_budget,
			&format!("{mode}-legacy-larger"),
		)?;
		let new_halt = halt(&new, case.budget);
		let new_prefix = new_halt.map_or(new.stderr.as_slice(), |(prefix, _)| prefix);
		let larger_prefix =
			halt(&larger, larger_budget).map_or(larger.stderr.as_slice(), |(prefix, _)| prefix);
		if !new.stdout.starts_with(&old.stdout)
			|| !larger.stdout.starts_with(&new.stdout)
			|| !new_prefix.starts_with(old_prefix)
			|| !larger_prefix.starts_with(new_prefix)
		{
			return Err(format!("two-way budget stream prefix failed: {}", case.id).into());
		}
		// Final filesystem equality cannot prove the order of overwritten
		// effects. Fail closed on any external-state module in a budget case;
		// the file-boundary probe tests supply ordered mutable-host witnesses.
		let mut parser = rune::parse::Parser::new(&case.source, rune::SourceId::EMPTY, true);
		while let Ok(token) = parser.parse::<rune::ast::Token>() {
			if matches!(token.kind, rune::ast::Kind::Ident(..))
				&& matches!(&case.source[token.span.range()], "fs" | "process" | "http")
			{
				return Err(format!(
					"budget difference requires an ordered external-state witness: {}",
					case.id
				)
				.into());
			}
		}
		if old.files != new.files || new.files != larger.files {
			return Err(format!(
				"budget difference has filesystem effects requiring an ordered witness: {}",
				case.id
			)
			.into());
		}
		class = if annotation {
			if new_halt.is_some()
				|| old.stdout != new.stdout
				|| old_prefix != new.stderr
				|| !exact(&new, &larger)
			{
				return Err("budget annotation differs beyond the approved suffix".into());
			}
			"budget-annotation"
		} else if new_halt.is_some() {
			"both-halt/further"
		} else if new.code == 0 {
			if !exact(&new, &larger) {
				return Err("larger legacy did not prove the new successful outcome".into());
			}
			"reaches-further/success"
		} else {
			if !exact(&new, &larger) {
				return Err("larger legacy did not prove the new terminal error".into());
			}
			"reaches-further/error"
		};
	}
	let required_budget_class = match case.id.as_str() {
		"control-budget-loop" => Some("both-halt/further"),
		"control-budget-success" => Some("reaches-further/success"),
		"control-budget-error" => Some("reaches-further/error"),
		"control-budget-annotation" => Some("budget-annotation"),
		_ => None,
	};
	if required_budget_class.is_some_and(|required| class != required) {
		return Err(format!(
			"named budget control lost its witness class: {} ({class})",
			case.id
		)
		.into());
	}
	let stderr = String::from_utf8_lossy(&old.stderr);
	let executed = case.callable
		&& !stderr.starts_with("error at ")
		&& !stderr.starts_with("error (no source position")
		&& !stderr.contains("Missing entry")
		&& !stderr.contains("Expected ")
		&& !stderr.contains("Bad argument count");
	Ok(
		json!({"id":case.id,"native":mode,"class":class,"execution_coverage":executed,"adaptation":case.adaptation,"route":selected[0],"legacy_exit":old.code,"auto_exit":new.code}),
	)
}

#[test]
fn real_cli_live_census_demos_and_l1_match_legacy_with_native_off_and_on() -> Result<()> {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
	let out = std::env::var_os("RNX_ROUTING_OUT")
		.map(PathBuf::from)
		.unwrap_or_else(|| root.join(format!("target/routing-cli/{}-{nonce}", std::process::id())));
	fs::create_dir_all(out.parent().ok_or("output parent missing")?)?;
	fs::create_dir(&out)?;
	let cases = cases()?;
	let next = AtomicUsize::new(0);
	let results = Mutex::new(Vec::new());
	let workers = std::thread::available_parallelism()?.get();
	std::thread::scope(|scope| {
		for _ in 0..workers {
			scope.spawn(|| {
				loop {
					let index = next.fetch_add(1, Ordering::Relaxed);
					let Some(case) = cases.get(index) else { break };
					let dir = out.join(&case.id);
					let result = (|| -> Result<Vec<Value>> {
						fs::create_dir(&dir)?;
						let mut rows = Vec::new();
						for native in [false, true] {
							rows.push(compare(case, &dir, native)?);
						}
						Ok(rows)
					})();
					results.lock().unwrap().push(match result {
						Ok(rows) => json!({"id":case.id,"status":"PASS","rows":rows}),
						Err(error) => {
							json!({"id":case.id,"status":"FAIL","error":error.to_string()})
						}
					});
				}
			});
		}
	});
	let mut results = results.into_inner().unwrap();
	results.sort_by_key(|row| row["id"].as_str().unwrap().to_owned());
	let failed = results.iter().any(|row| row["status"] != "PASS");
	let receipt = out.join("RESULT.json");
	let unit_clone_rows: Vec<_> = results
		.iter()
		.filter_map(|row| {
			row["rows"]
				.as_array()
				.filter(|rows| rows.iter().any(|row| row["class"] == "unit-clone"))
				.map(|_| row["id"].clone())
		})
		.collect();
	let unit_clone_observations = results
		.iter()
		.filter_map(|row| row["rows"].as_array())
		.flatten()
		.filter(|row| row["class"] == "unit-clone")
		.count();
	let mut execution_by_class = std::collections::BTreeMap::new();
	let mut controls_by_class = std::collections::BTreeMap::new();
	for row in results
		.iter()
		.filter_map(|row| row["rows"].as_array())
		.flatten()
	{
		let counts = if row["execution_coverage"] == true {
			&mut execution_by_class
		} else {
			&mut controls_by_class
		};
		*counts
			.entry(row["class"].as_str().unwrap().to_owned())
			.or_insert(0usize) += 1;
	}
	let unit_clone_row_count = unit_clone_rows.len();
	fs::write(
		&receipt,
		serde_json::to_vec_pretty(
			&json!({"status":if failed{"FAIL"}else{"PASS"},"workers":workers,"execution_by_class":execution_by_class,"controls_by_class":controls_by_class,"named_differences":{"unit-clone":{"rows":unit_clone_rows,"row_count":unit_clone_row_count,"observations":unit_clone_observations}},"results":results}),
		)?,
	)?;
	println!(
		"{} routing CLI; receipt {}",
		if failed { "FAIL" } else { "PASS" },
		receipt.display()
	);
	if failed {
		Err(format!(
			"routing CLI comparison failed; receipt {}",
			receipt.display()
		)
		.into())
	} else {
		Ok(())
	}
}

#[test]
fn entry_reexports_fall_back_before_effects_and_missing_entry_is_terminal() -> Result<()> {
	let nonce = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
	let out = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
		"target/routing-alias/{}-{nonce}",
		std::process::id()
	));
	fs::create_dir_all(&out)?;
	for (id, source) in [
		(
			"alias",
			"mod inner{pub fn work(args){println!(\"once\");7}}pub use inner::work as main;",
		),
		(
			"final",
			"mod inner{pub fn main(args){println!(\"once\");7}}pub use inner::main;",
		),
		(
			"group",
			"mod inner{pub fn work(args){println!(\"once\");7}}pub use inner::{work as main};",
		),
		(
			"wildcard",
			"mod inner{pub fn main(args){println!(\"once\");7}}pub use inner::*;",
		),
		(
			"nested-group",
			"mod inner{pub mod nested{pub fn work(args){println!(\"once\");7}}}pub use inner::{nested::{work as main}};",
		),
		("direct", "pub fn main(args){println!(\"once\");7}"),
		("missing", "pub fn work(args){println!(\"never\");7}"),
	] {
		let case = Case {
			id: id.into(),
			source: source.into(),
			callable: id != "missing",
			adaptation: "entry-boundary".into(),
			clock: "",
			budget: 2_000_000,
		};
		let dir = out.join(id);
		fs::create_dir(&dir)?;
		for native in [false, true] {
			let mode = if native { "on" } else { "off" };
			let old = run(
				&case,
				&dir,
				true,
				native,
				false,
				case.budget,
				&format!("{mode}-legacy"),
			)?;
			let new = run(
				&case,
				&dir,
				false,
				native,
				false,
				case.budget,
				&format!("{mode}-auto"),
			)?;
			assert!(exact(&old, &new), "{id} native={mode}");
			let evidence = run(
				&case,
				&dir,
				false,
				native,
				true,
				case.budget,
				&format!("{mode}-route"),
			)?;
			let route = if matches!(id, "missing" | "direct") {
				format!("rnx: engine=execution native={mode}\n")
			} else {
				"rnx: engine=legacy refusal=Import\n".to_owned()
			};
			assert!(
				evidence.stderr.starts_with(route.as_bytes()),
				"{id}: {}",
				String::from_utf8_lossy(&evidence.stderr)
			);
			if id == "missing" {
				assert_eq!(new.code, 1);
				assert!(new.stdout.is_empty());
				assert!(String::from_utf8_lossy(&new.stderr).contains("Missing entry `main`"));
				assert_eq!(&evidence.stderr[route.len()..], new.stderr);
			} else {
				assert_eq!(new.code, 0);
				assert_eq!(new.stdout, b"once\n7\n");
				assert!(new.stderr.is_empty());
				assert_eq!(evidence.stdout, new.stdout);
			}
		}
	}
	Ok(())
}
