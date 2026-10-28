//! Live stock/fresh execution for census entries with declared safe inputs.
use rune::experimental_core::{
	error::{Detail, ErrorKind, ErrorOrigin, ErrorProjection, d1_missing_method_message},
	execute::Executor,
	frontend,
	heap::{Limits, Store},
	value::Scalar,
};
use rune::{Diagnostics, Hash, Source, Sources};
use serde_json::{Value, json};
use std::{
	collections::{BTreeMap, BTreeSet},
	fs,
	path::Path,
	process::{Command, Stdio},
	time::{Instant, SystemTime, UNIX_EPOCH},
};

fn opaque_error(error: impl std::fmt::Debug) -> std::io::Error {
	std::io::Error::other(format!("{error:?}"))
}

fn reviewed_class(projection: ErrorProjection) -> Result<String, Box<dyn std::error::Error>> {
	match projection {
		ErrorProjection::Class(class) => Ok(format!("{class:?}")),
		other => Err(format!("unreviewed census error projection: {other:?}").into()),
	}
}

fn wall_ms() -> i64 {
	SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_millis()
		.try_into()
		.unwrap()
}
fn sources(text: &str) -> Sources {
	let mut source = Sources::new();
	source
		.insert(Source::new("census.rn", text).unwrap())
		.unwrap();
	source
}
fn clock_check(clock: &str, value: Option<i64>, before: i64, after: i64, elapsed: u128) -> bool {
	match (clock, value) {
		("", _) => true,
		("wall", Some(v)) => before <= v && v <= after.saturating_add(1),
		("monotonic", Some(v)) => v >= 0 && v as u128 <= elapsed.saturating_add(1),
		_ => false,
	}
}

pub(super) fn child(
	input: &Path,
	engine: &str,
	clock: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
	let original = std::fs::read_to_string(input)?;
	let source = super::enrollment::augment_source(&original)?;
	let lifecycle = crate::lifecycle::Lifecycle::new(true)?;
	let (context, http) = super::enrollment::context(&lifecycle);
	let mut diagnostics = Diagnostics::new();
	let started = Instant::now();
	let before = wall_ms();
	let (outcome, clock_value) = match engine {
		"stock" => {
			let mut src = sources(&source);
			let mut vm = rune::prepare(&mut src)
				.with_context(&context)
				.with_diagnostics(&mut diagnostics)
				.build_vm()?;
			let result = rune::runtime::budget::with(2_000_000, || vm.call(["main"], ())).call();
			match result {
				Ok(value) => {
					let clock_value = value.as_integer::<i64>().ok();
					(
						json!({"status":"value","typed":super::enrollment::typed_stock(&value, 0)}),
						clock_value,
					)
				}
				Err(e) => {
					let class = reviewed_class(e.error_class())?;
					let args: Vec<_> = e
						.invocation_context()
						.map(|c| match c {
							rune::runtime::InvocationContext::BadArgument { arg } => arg,
						})
						.collect();
					let origin = origin_name(e.origin());
					let site = e.first_location().and_then(|at| {
						let inst = at.unit.debug_info()?.instruction_at(at.ip)?;
						let range = inst.span.range();
						Some(
							json!({"source":inst.source_id.into_index(),"start":range.start,"end":range.end}),
						)
					});
					(
						json!({"status":"error","class":class,"origin":origin,"arguments":args,"site":site,"message":e.error().to_string()}),
						None,
					)
				}
			}
		}
		"fresh" => {
			let mut src = sources(&source);
			let program =
				frontend::compile(&mut src, &context, &mut diagnostics, 1).map_err(opaque_error)?;
			let main = program
				.function(Hash::type_hash(["main"]))
				.ok_or("fresh main missing")?;
			let mut store = Store::new(Limits::default()).map_err(opaque_error)?;
			let mut executor = Executor::new().map_err(opaque_error)?;
			executor
				.set_host_runtime(Some(rune::sync::Arc::try_new(context.runtime()?)?))
				.map_err(opaque_error)?;
			match executor.invoke(&program, &mut store, main, &[], 2_000_000) {
				Ok(root) => {
					let clock_value = match store.root_scalar(&root).map_err(opaque_error)? {
						Some(Scalar::I64(v)) => Some(v),
						_ => None,
					};
					let outcome = json!({"status":"value","typed":super::enrollment::typed_fresh(&mut store, &mut executor, &program, &root, 0)});
					store.root_release(root).map_err(opaque_error)?;
					(outcome, clock_value)
				}
				Err(e) => {
					let class = reviewed_class(e.error.error_class())?;
					let host = e.error.kind == ErrorKind::Host
						|| matches!(
							e.error.detail,
							Detail::HostMessage { .. }
								| Detail::HostExpectedType { .. }
								| Detail::HostExpectedNumber { .. }
						);
					let origin = if host { "host" } else { "script" };
					let arguments: Vec<usize> = if host {
						e.error
							.detail
							.numeric_argument()
							.map(|(_, arg)| usize::from(arg))
							.into_iter()
							.collect()
					} else {
						Vec::new()
					};
					let site = e
						.error
						.location
						.map(|at| json!({"source":at.source,"start":at.start,"end":at.end}));
					(
						json!({"status":"error","class":class,"origin":origin,"arguments":arguments,"site":site,"message":e.message(&program).to_string()}),
						None,
					)
				}
			}
		}
		_ => return Err("invalid census engine".into()),
	};
	let after = wall_ms();
	let elapsed = started.elapsed().as_millis();
	let clock_ok = clock_check(clock, clock_value, before, after, elapsed);
	drop(context);
	drop(http);
	lifecycle.close()?;
	Ok(json!({"outcome":outcome,"clock":clock,"clock_ok":clock_ok,"lifecycle_closed":true}))
}

const PLAN: &str = include_str!("../../tools/verify/fixtures/census/execution.tsv");
#[derive(Clone)]
struct Plan<'a> {
	id: &'a str,
	mode: &'a str,
	reason: &'a str,
	fixture: &'a str,
	clock: &'a str,
}
fn plan() -> Result<Vec<Plan<'static>>, Box<dyn std::error::Error>> {
	let mut lines = PLAN.lines();
	if lines.next() != Some("id\tmode\treason\tfixture\tclock") {
		return Err("invalid census execution table header".into());
	}
	let mut ids = BTreeSet::new();
	let mut rows = Vec::new();
	for line in lines {
		let fields: Vec<_> = line.split('\t').collect();
		let [id, mode, reason, fixture, clock]: [&str; 5] = fields
			.try_into()
			.map_err(|_| "invalid census execution row")?;
		if id.is_empty()
			|| !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
			|| !ids.insert(id)
			|| !matches!(mode, "execute" | "admission-only")
			|| !matches!(fixture, "" | "orders")
			|| !matches!(clock, "" | "wall" | "monotonic")
			|| (mode == "execute" && !reason.is_empty())
			|| (mode == "admission-only" && reason.is_empty())
		{
			return Err("invalid census execution policy".into());
		}
		rows.push(Plan {
			id,
			mode,
			reason,
			fixture,
			clock,
		});
	}
	if rows.len() != 244 {
		return Err("wrong census execution policy count".into());
	}
	Ok(rows)
}

fn snapshot(root: &Path) -> Result<Value, Box<dyn std::error::Error>> {
	fn walk(
		root: &Path,
		dir: &Path,
		items: &mut Vec<Value>,
	) -> Result<(), Box<dyn std::error::Error>> {
		let mut children = fs::read_dir(dir)?
			.map(|e| e.map(|x| x.path()))
			.collect::<Result<Vec<_>, _>>()?;
		children.sort();
		for path in children {
			let meta = fs::symlink_metadata(&path)?;
			let relative = path
				.strip_prefix(root)?
				.to_str()
				.ok_or("non-UTF8 scratch path")?;
			use std::os::unix::fs::PermissionsExt;
			if meta.is_dir() {
				items.push(
					json!({"path":relative,"kind":"dir","mode":meta.permissions().mode() & 0o7777}),
				);
				walk(root, &path, items)?;
			} else if meta.is_file() {
				items.push(json!({"path":relative,"kind":"file","mode":meta.permissions().mode() & 0o7777,"sha256":super::census::hash_file(&path)?}));
			} else if meta.file_type().is_symlink() {
				items
					.push(json!({"path":relative,"kind":"symlink","target":fs::read_link(&path)?}));
			} else {
				return Err("unsupported census scratch entry".into());
			}
			if items.len() > 10_000 {
				return Err("census scratch entry limit exceeded".into());
			}
		}
		Ok(())
	}
	let mut items = Vec::new();
	walk(root, root, &mut items)?;
	Ok(json!(items))
}

fn invoke(
	binary: &Path,
	input: &Path,
	repo: &Path,
	output: &Path,
	item: &Plan<'_>,
	engine: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
	let run_dir = output.join(item.id).join(engine);
	fs::create_dir_all(&run_dir)?;
	let work = run_dir.join("work");
	fs::create_dir(&work)?;
	let report = run_dir.join("report");
	fs::create_dir(&report)?;
	fs::create_dir(work.join("run"))?;
	if item.fixture == "orders" {
		fs::create_dir(work.join("data"))?;
		fs::copy(
			repo.join("demos/data/orders.json"),
			work.join("data/orders.json"),
		)?;
	}
	let mut command = Command::new("/usr/bin/timeout");
	command
		.args([
			"--signal=TERM",
			"--kill-after=2s",
			"30s",
			"/usr/bin/prlimit",
			"--cpu=25",
			"--as=2147483648",
			"--fsize=67108864",
			"--core=0",
			"--",
			"/usr/bin/bwrap",
			"--die-with-parent",
			"--unshare-net",
			"--clearenv",
			"--ro-bind",
			"/usr",
			"/usr",
			"--ro-bind",
			"/lib",
			"/lib",
			"--ro-bind",
			"/lib64",
			"/lib64",
			"--proc",
			"/proc",
			"--dev",
			"/dev",
			"--tmpfs",
			"/tmp",
			"--ro-bind",
		])
		.arg(input)
		.arg("/input.rn")
		.arg("--ro-bind")
		.arg(binary)
		.arg("/observer")
		.arg("--bind")
		.arg(&work)
		.arg("/work")
		.arg("--bind")
		.arg(&report)
		.arg("/report")
		.args([
			"--chdir",
			"/work/run",
			"--setenv",
			"HOME",
			"/work/run",
			"--setenv",
			"PATH",
			"/usr/bin:/bin",
			"--setenv",
			"LANG",
			"C.UTF-8",
			"--setenv",
			"TZ",
			"UTC",
			"--setenv",
			"TMPDIR",
			"/tmp",
			"--",
			"/observer",
			"--child",
			"census-exec",
			"/input.rn",
			engine,
			item.clock,
			"/report/result.json",
		])
		.env_clear()
		.stdin(Stdio::null())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped());
	let result = command.output()?;
	fs::write(run_dir.join("stdout"), &result.stdout)?;
	fs::write(run_dir.join("stderr"), &result.stderr)?;
	if !result.status.success() {
		return Err(format!(
			"census execution {} {engine} failed: {}",
			item.id, result.status
		)
		.into());
	}
	if result.stdout.len() > 2_097_152 || result.stderr.len() > 2_097_152 {
		return Err("census output cap exceeded".into());
	}
	let observation: Value = serde_json::from_slice(&fs::read(report.join("result.json"))?)?;
	if observation["clock_ok"] != true || observation["lifecycle_closed"] != true {
		return Err("census clock or lifecycle check failed".into());
	}
	Ok(
		json!({"observation":observation,"stdout":result.stdout,"stderr":result.stderr,"files":snapshot(&work)?}),
	)
}

pub(super) fn run(
	binary: &Path,
	fixtures: &Path,
	repo: &Path,
	out: &Path,
	admissions: &BTreeMap<&str, &str>,
) -> Result<Value, Box<dyn std::error::Error>> {
	let policies = plan()?;
	let output = out.join("live-execution");
	fs::create_dir(&output)?;
	let mut executed = 0;
	let mut compile_only = Vec::new();
	let mut differences = Vec::new();
	for item in policies {
		let class = admissions
			.get(item.id)
			.ok_or("missing census admission policy")?;
		if item.mode == "admission-only" {
			if *class == "both-admit"
				&& !matches!(item.reason, "no-main" | "main-requires-arguments")
			{
				return Err("unexpected compile-only reason".into());
			}
			if *class != "both-admit" && item.reason != *class {
				return Err("census refusal policy mismatch".into());
			}
			compile_only.push(json!({"id":item.id,"reason":item.reason}));
			continue;
		}
		if *class != "both-admit" {
			return Err("execution requires both-admit policy".into());
		}
		let input = fixtures
			.join("census/inputs")
			.join(format!("{}.rn", item.id));
		let comparison = (|| -> Result<bool, Box<dyn std::error::Error>> {
			let mut old = invoke(binary, &input, repo, &output, &item, "stock")?;
			let mut new = invoke(binary, &input, repo, &output, &item, "fresh")?;
			if !item.clock.is_empty() {
				for observed in [&mut old, &mut new] {
					if observed["observation"]["outcome"]["status"] != "value" {
						return Err("clock case did not return a value".into());
					}
					observed["observation"]["outcome"]["typed"] =
						json!({"clock-envelope":"passed"});
				}
			}
			Ok(equivalent(&old, &new))
		})();
		executed += 1;
		match comparison {
			Ok(true) => {}
			Ok(false) => {
				differences.push(json!({"id":item.id,"reason":"live observations differ"}))
			}
			Err(e) => differences.push(json!({"id":item.id,"reason":e.to_string()})),
		}
	}
	let summary =
		json!({"executed":executed,"admission_only":compile_only,"differences":differences});
	let mut file = std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(output.join("RESULT.json"))?;
	use std::io::Write;
	serde_json::to_writer_pretty(&mut file, &summary)?;
	file.write_all(b"\n")?;
	if !summary["differences"].as_array().is_some_and(Vec::is_empty) {
		return Err("unreviewed census execution differences".into());
	}
	Ok(summary)
}

fn origin_name(origin: ErrorOrigin) -> &'static str {
	match origin {
		ErrorOrigin::Script => "script",
		ErrorOrigin::Host => "host",
		ErrorOrigin::Unknown => "unknown",
	}
}

fn field<'a>(outcome: &'a serde_json::Map<String, Value>, name: &str) -> Option<&'a str> {
	outcome.get(name).and_then(Value::as_str)
}

/// Observable equivalence: every field must match, except that origin is compared only
/// when both engines know it, and a TypeMismatch message may differ by the reviewed D1
/// missing-method rewrite. The class itself was already required to be reviewed.
fn equivalent(old: &Value, new: &Value) -> bool {
	let (Some(old_outcome), Some(new_outcome)) = (
		old["observation"]["outcome"].as_object(),
		new["observation"]["outcome"].as_object(),
	) else {
		return old == new;
	};
	if old_outcome.get("status") != Some(&json!("error"))
		|| new_outcome.get("status") != Some(&json!("error"))
	{
		return old == new;
	}
	let mut rest_old = old.clone();
	let mut rest_new = new.clone();
	rest_old["observation"]["outcome"] = Value::Null;
	rest_new["observation"]["outcome"] = Value::Null;
	if rest_old != rest_new {
		return false;
	}
	let mut remaining_old = old_outcome.clone();
	let mut remaining_new = new_outcome.clone();
	for key in ["status", "class", "arguments", "site", "origin", "message"] {
		remaining_old.remove(key);
		remaining_new.remove(key);
	}
	if remaining_old != remaining_new {
		return false;
	}
	for field in ["class", "arguments", "site"] {
		if old_outcome.get(field) != new_outcome.get(field) {
			return false;
		}
	}
	match (field(old_outcome, "origin"), field(new_outcome, "origin")) {
		(Some("unknown"), Some(_)) | (Some(_), Some("unknown")) => {}
		(a, b) if a == b => {}
		_ => return false,
	}
	match (field(old_outcome, "message"), field(new_outcome, "message")) {
		(Some(a), Some(b)) if a == b => true,
		(Some(a), Some(b)) => {
			old_outcome.get("class") == Some(&json!("TypeMismatch"))
				&& d1_missing_method_message(a, b)
		}
		_ => false,
	}
}

fn sample_error(origin: &str, message: &str) -> Value {
	json!({"observation":{"outcome":{"status":"error","class":"TypeMismatch","origin":origin,
		"arguments":[],"site":{"source":0,"start":4,"end":9},"message":message},"clock_ok":true}})
}

/// Runs in every census self-check: the test target has no libtest harness.
pub(super) fn equivalence_self_check() -> Result<usize, Box<dyn std::error::Error>> {
	let old = "Type `std::i64` has no matching method";
	let new = "Type `std::i64` has no method named `len`";
	let mut moved = sample_error("script", old);
	moved["observation"]["outcome"]["site"]["start"] = json!(5);
	let mut class = sample_error("script", old);
	class["observation"]["outcome"]["class"] = json!("IndexOutOfBounds");
	let mut streams = sample_error("script", old);
	streams["observation"]["clock_ok"] = json!(false);
	let mut extra = sample_error("script", old);
	extra["observation"]["outcome"]["extra"] = json!(1);
	let cases = [
		(
			sample_error("script", old),
			sample_error("script", old),
			true,
		),
		(
			sample_error("unknown", old),
			sample_error("script", new),
			true,
		),
		(
			sample_error("host", old),
			sample_error("script", old),
			false,
		),
		(
			sample_error("script", old),
			sample_error("script", "different"),
			false,
		),
		(sample_error("script", old), moved, false),
		(sample_error("script", old), class, false),
		(sample_error("script", old), streams, false),
		(sample_error("script", old), extra, false),
	];
	for (i, (a, b, expected)) in cases.iter().enumerate() {
		if equivalent(a, b) != *expected {
			return Err(format!("census equivalence self-check {i} failed").into());
		}
	}
	Ok(cases.len())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn equivalence_compares_every_observable_field() {
		assert_eq!(equivalence_self_check().unwrap(), 8);
	}

	#[test]
	fn execution_plan_accounts_for_every_case() {
		let rows = plan().unwrap();
		assert_eq!(rows.len(), 244);
		assert_eq!(rows.iter().filter(|row| row.mode == "execute").count(), 67);
		assert_eq!(
			rows.iter().filter(|row| row.reason == "no-main").count(),
			16
		);
		assert_eq!(
			rows.iter()
				.filter(|row| row.reason == "main-requires-arguments")
				.count(),
			4
		);
	}
	#[test]
	fn clock_envelopes_do_not_accept_missing_or_outside_values() {
		assert!(clock_check("wall", Some(11), 10, 12, 4));
		assert!(!clock_check("wall", Some(9), 10, 12, 4));
		assert!(!clock_check("wall", None, 10, 12, 4));
		assert!(clock_check("monotonic", Some(4), 0, 0, 5));
		assert!(!clock_check("monotonic", Some(-1), 0, 0, 5));
	}
}
