//! Real-registration, compile-only census. Never constructs a VM or invokes Rune.
//! Frozen fixture bytes are inputs; aggregate observations come only from children.
use rune::ast::Spanned;
use rune::compile::{CompileVisitor, Located, MetaError, meta::MetaRef};
use serde_json::{Value, json};

#[derive(Default)]
struct Inventory(Vec<Value>);
impl CompileVisitor for Inventory {
	fn visit_meta(&mut self, at: &dyn Located, meta: MetaRef<'_>) -> Result<(), MetaError> {
		let at = at.location();
		self.0.push(json!({
			"path": meta.item.to_string(), "hash": format!("{}", meta.hash),
			"context": meta.context, "kind": format!("{:?}", meta.kind),
			"source": at.source_id.into_index(),
			"start": at.span.start.0, "end": at.span.end.0,
			"evidence": "compiler-resolved item mention, not runtime invocation"
		}));
		Ok(())
	}
}

fn diagnostics(d: &rune::Diagnostics) -> Vec<Value> {
	d.diagnostics()
		.iter()
		.map(|d| {
			let mut row = json!({"raw": format!("{d:?}")});
			if let rune::diagnostics::Diagnostic::Fatal(f) = d {
				row["source"] = json!(f.source_id().into_index());
				if let Some(e) = f.as_compile_error() {
					let span = e.span();
					row["start"] = json!(span.start.0);
					row["end"] = json!(span.end.0);
					row["message"] = json!(e.to_string());
				}
			}
			row
		})
		.collect()
}

use std::{
	fs,
	io::Write,
	path::{Component, Path, PathBuf},
	process::{Command, Stdio},
	time::Instant,
};
type CensusResult<T> = Result<T, Box<dyn std::error::Error>>;
const MANIFEST: &str = include_str!("../../tools/verify/fixtures/census_manifest.json");

pub(super) fn child(args: &[String]) -> CensusResult<Value> {
	if args.len() != 1 {
		return Err("census child requires exactly one input path".into());
	}
	let input = Path::new(&args[0]);
	if fs::metadata(input)?.len() > 2 * 1024 * 1024 {
		return Err("census input exceeds 2 MiB".into());
	}
	let source = fs::read_to_string(input)?;
	if source.len() > 2 * 1024 * 1024 {
		return Err("census input grew beyond 2 MiB".into());
	}
	let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
	let mut context = rune::Context::with_default_modules().unwrap();
	let mut functions = crate::install_core(&mut context).unwrap();
	functions.extend(crate::fs::install(&mut context).unwrap());
	functions.extend(crate::path::install(&mut context).unwrap());
	functions.extend(crate::time::install(&mut context).unwrap());
	functions.extend(crate::text::install(&mut context).unwrap());
	let http = crate::http::State::default();
	functions.extend(crate::http::install(&mut context, &http, lifecycle.scope("http")).unwrap());
	functions.extend(crate::env::install(&mut context, std::sync::Arc::from([])).unwrap());
	let runtime = context.runtime().unwrap();
	let registry: Vec<_> = functions
		.iter()
		.map(|f| {
			let mut parts = f.path.split("::");
			let item = rune::ItemBuf::with_crate_item(parts.next().unwrap(), parts).unwrap();
			let path_hash = rune::Hash::type_hash(&item);
			let (hash, kind) = if f.path == "interchange::Dense" {
				(path_hash, "type")
			} else if let Some(name) = f.path.strip_prefix("interchange::Dense::") {
				let container = rune::ItemBuf::with_crate_item("interchange", ["Dense"]).unwrap();
				(
					rune::Hash::associated_function(
						rune::Hash::type_hash(&container),
						rune::Hash::ident(name),
					),
					"associated-function",
				)
			} else {
				(path_hash, "function")
			};
			if kind != "type" {
				assert!(
					runtime.function(&hash).is_some(),
					"registry lacks {}",
					f.path
				);
			}
			json!({"path": f.path, "item": item.to_string(), "hash": format!("{hash}"),
			"path_hash": format!("{path_hash}"), "kind": kind, "doc": f.doc})
		})
		.collect();
	let mut sources = rune::Sources::new();
	sources
		.insert(rune::Source::new("h0.rn", &source).unwrap())
		.unwrap();
	let mut stock_diagnostics = rune::Diagnostics::new();
	let mut visitor = Inventory::default();
	let mut loader = rune::compile::NoopSourceLoader::default();
	let stock = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut stock_diagnostics)
		.with_source_loader(&mut loader)
		.with_visitor(&mut visitor)
		.unwrap()
		.build();
	let mut result = json!({"stock": {"compiled": stock.is_ok(),
		"error": stock.as_ref().err().map(|e| format!("{e:?}")),
		"diagnostics": diagnostics(&stock_diagnostics)}, "resolved_items": visitor.0,
		"registry": registry, "execution": "not run; compile-only"});
	if stock.is_ok() {
		let mut fresh_sources = rune::Sources::new();
		fresh_sources
			.insert(rune::Source::new("h0.rn", &source).unwrap())
			.unwrap();
		let mut fresh_diagnostics = rune::Diagnostics::new();
		let fresh = rune::experimental_core::frontend::compile(
			&mut fresh_sources,
			&context,
			&mut fresh_diagnostics,
			1,
		);
		result["fresh"] = match fresh {
			Ok(_) => json!({"compiled": true, "diagnostics": diagnostics(&fresh_diagnostics)}),
			Err(e) => json!({"compiled": false, "kind": format!("{:?}", e.kind),
				"refusal": e.refusal.map(|r| format!("{r:?}")), "operation": e.operation,
				"error": format!("{e:?}"), "location": e.location.map(|l| json!({
					"source": l.source, "start": l.start, "end": l.end})),
				"diagnostics": diagnostics(&fresh_diagnostics)}),
		};
	} else {
		result["fresh"] = json!({"compiled": null, "not_run": "stock compile failed"});
	}
	drop(stock);
	drop(runtime);
	drop(context);
	drop(http);
	lifecycle.close().unwrap();
	result["lifecycle_closed"] = json!(true);
	Ok(result)
}

fn fail(message: impl Into<String>) -> Box<dyn std::error::Error> {
	message.into().into()
}
fn string<'a>(value: &'a Value, key: &str) -> CensusResult<&'a str> {
	value[key]
		.as_str()
		.ok_or_else(|| fail(format!("missing string {key}")))
}
fn number(value: &Value, key: &str) -> CensusResult<u64> {
	value[key]
		.as_u64()
		.ok_or_else(|| fail(format!("missing unsigned integer {key}")))
}
fn read_json(path: &Path) -> CensusResult<Value> {
	Ok(serde_json::from_slice(&fs::read(path)?)?)
}
fn hash_file(path: &Path) -> CensusResult<String> {
	let output = Command::new("sha256sum").arg("--").arg(path).output()?;
	if !output.status.success() {
		return Err(fail(format!("hash failed: {}", path.display())));
	}
	let digest = String::from_utf8(output.stdout)?
		.split_whitespace()
		.next()
		.ok_or("missing hash")?
		.to_owned();
	if digest.len() != 64 || !digest.bytes().all(|b| b.is_ascii_hexdigit()) {
		return Err("invalid hash output".into());
	}
	Ok(digest)
}
fn hash_bytes(bytes: &[u8]) -> CensusResult<String> {
	let mut child = Command::new("sha256sum")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()?;
	child
		.stdin
		.take()
		.ok_or("missing hash stdin")?
		.write_all(bytes)?;
	let output = child.wait_with_output()?;
	if !output.status.success() {
		return Err("byte hash failed".into());
	}
	Ok(String::from_utf8(output.stdout)?
		.split_whitespace()
		.next()
		.ok_or("missing byte hash")?
		.to_owned())
}
fn check_registration(manifest: &Value, repo: &Path) -> CensusResult<()> {
	let policy = &manifest["registration_policy"];
	let files = policy["whole_files_sha256"]
		.as_object()
		.ok_or("registration whole-file policy required")?;
	for (path, hash) in files {
		pinned(
			repo,
			path,
			hash.as_str().ok_or("invalid registration hash")?,
		)?;
	}
	let function = &policy["install_core"];
	let source = fs::read_to_string(repo.join(string(function, "path")?))?;
	let start = string(function, "start")?;
	if source.matches(start).count() != 1 {
		return Err("ambiguous install_core function start".into());
	}
	let a = source.find(start).ok_or("install_core missing")?;
	let b = a + source[a..]
		.find(string(function, "end")?)
		.ok_or("install_core boundary missing")?;
	if hash_bytes(&source.as_bytes()[a..b])? != string(function, "sha256")? {
		return Err("actual install_core function differs from reviewed real registration".into());
	}
	Ok(())
}

fn pinned(root: &Path, relative: &str, expected: &str) -> CensusResult<PathBuf> {
	let p = Path::new(relative);
	if p.is_absolute() || !p.components().all(|c| matches!(c, Component::Normal(_))) {
		return Err("unsafe census fixture path".into());
	}
	let path = root.join(p);
	if hash_file(&path)? != expected {
		return Err(fail(format!(
			"census fixture identity mismatch: {relative}"
		)));
	}
	Ok(path)
}
fn write_new(path: &Path, data: &[u8]) -> CensusResult<()> {
	let mut file = fs::OpenOptions::new()
		.create_new(true)
		.write(true)
		.open(path)?;
	file.write_all(data)?;
	Ok(())
}

#[derive(Clone, Copy)]
enum Order {
	Ordinary,
	Root,
	Rows,
	Row,
	Case,
	Origins,
	Origin,
	CellMaps,
	CellMap,
}
fn keys(map: &serde_json::Map<String, Value>, order: Order) -> CensusResult<Vec<&str>> {
	const CASE: &[&str] = &["id", "path", "compiled_sha256", "source_bytes", "origins"];
	const ORIGIN: &[&str] = &[
		"cohort",
		"origin",
		"mode",
		"original_sha256",
		"prefix_bytes",
		"suffix_bytes",
		"s0_id",
		"s0_mode",
		"notebook_sha256",
		"cell_index",
		"cell_sha256",
		"execution_count",
		"synthetic",
		"cell_map",
	];
	const CELL: &[&str] = &["cell", "start", "end", "sha256"];
	let prescribed = match order {
		Order::Case => Some(CASE),
		Order::Origin => Some(ORIGIN),
		Order::CellMap => Some(CELL),
		_ => None,
	};
	let required = matches!(order, Order::Case | Order::CellMap);
	if let Some(order) = prescribed {
		if map.keys().any(|k| !order.contains(&k.as_str())) {
			return Err("unknown ordered census metadata field".into());
		}
		if required && map.len() != order.len() {
			return Err("incomplete census metadata".into());
		}
		return Ok(order
			.iter()
			.copied()
			.filter(|k| map.contains_key(*k))
			.collect());
	}
	let mut result: Vec<_> = map.keys().map(String::as_str).collect();
	result.sort_unstable();
	if matches!(order, Order::Row) {
		if !map.contains_key("case") {
			return Err("aggregate row lacks case".into());
		}
		result.retain(|k| *k != "case");
		result.insert(0, "case");
	}
	Ok(result)
}
fn field_order(order: Order, key: &str) -> Order {
	match (order, key) {
		(Order::Root, "rnx") => Order::Rows,
		(Order::Row, "case") => Order::Case,
		(Order::Case, "origins") => Order::Origins,
		(Order::Origin, "cell_map") => Order::CellMaps,
		_ => Order::Ordinary,
	}
}
fn element_order(order: Order) -> Order {
	match order {
		Order::Rows => Order::Row,
		Order::Origins => Order::Origin,
		Order::CellMaps => Order::CellMap,
		_ => Order::Ordinary,
	}
}
fn emit(value: &Value, order: Order, depth: usize, out: &mut String) -> CensusResult<()> {
	let indent = |out: &mut String, n: usize| out.push_str(&"  ".repeat(n));
	match value {
		Value::Object(map) => {
			out.push('{');
			let fields = keys(map, order)?;
			if !fields.is_empty() {
				out.push('\n');
			}
			for (i, key) in fields.iter().enumerate() {
				indent(out, depth + 1);
				out.push_str(&serde_json::to_string(key)?);
				out.push_str(": ");
				emit(&map[*key], field_order(order, key), depth + 1, out)?;
				if i + 1 != fields.len() {
					out.push(',');
				}
				out.push('\n');
			}
			if !fields.is_empty() {
				indent(out, depth);
			}
			out.push('}');
		}
		Value::Array(values) => {
			out.push('[');
			if !values.is_empty() {
				out.push('\n');
			}
			for (i, value) in values.iter().enumerate() {
				indent(out, depth + 1);
				emit(value, element_order(order), depth + 1, out)?;
				if i + 1 != values.len() {
					out.push(',');
				}
				out.push('\n');
			}
			if !values.is_empty() {
				indent(out, depth);
			}
			out.push(']');
		}
		Value::Number(n) if !n.is_i64() && !n.is_u64() => {
			return Err("floating census metadata has no reviewed canonical encoding".into());
		}
		other => out.push_str(&serde_json::to_string(other)?),
	}
	Ok(())
}
fn canonical(value: &Value) -> CensusResult<Vec<u8>> {
	let mut pretty = String::new();
	emit(value, Order::Root, 0, &mut pretty)?;
	let mut ascii = String::with_capacity(pretty.len());
	for c in pretty.chars() {
		if (c as u32) < 127 {
			ascii.push(c);
		} else {
			for unit in c.encode_utf16(&mut [0; 2]) {
				use std::fmt::Write;
				write!(ascii, "\\u{unit:04x}")?;
			}
		}
	}
	ascii.push('\n');
	Ok(ascii.into_bytes())
}

/// Controller in the same versioned executable as the private-registration child.
/// `out` is the launcher's new output root; this function owns a fresh `out/census`.
pub(super) fn run(args: &[String]) -> CensusResult<Value> {
	let mut out = None;
	let mut fixtures = None;
	let mut iter = args.iter();
	while let Some(key) = iter.next() {
		let value = iter.next().ok_or("missing census option value")?;
		match key.as_str() {
			"--out" if out.is_none() => out = Some(PathBuf::from(value)),
			"--fixtures-root" if fixtures.is_none() => fixtures = Some(PathBuf::from(value)),
			_ => return Err(fail(format!("unknown or duplicate census option {key}"))),
		}
	}
	let root = fixtures.ok_or("census requires --fixtures-root")?;
	let parent = out.ok_or("census requires --out")?;
	if !parent.is_dir() {
		return Err("census output parent must already be created by launcher".into());
	}
	let out = parent.join("census");
	fs::create_dir(&out)?;
	let records = out.join("records");
	fs::create_dir(&records)?;
	let start = Instant::now();
	let manifest: Value = serde_json::from_str(MANIFEST)?;
	if number(&manifest, "schema")? != 1 || number(&manifest, "count")? != 244 {
		return Err("unsupported census manifest".into());
	}
	let cases_path = pinned(
		&root,
		string(&manifest, "cases_path")?,
		string(&manifest, "cases_sha256")?,
	)?;
	let cases = read_json(&cases_path)?;
	let cases = cases.as_array().ok_or("case array required")?;
	let rows = manifest["rows"]
		.as_array()
		.ok_or("manifest rows required")?;
	if cases.len() != 244 || rows.len() != 244 {
		return Err("census requires exactly 244 cases".into());
	}
	let repo = Path::new(env!("CARGO_MANIFEST_DIR"));
	check_registration(&manifest, repo)?;
	let executable = std::env::current_exe()?;
	let executable_hash = hash_file(&executable)?;
	let mut ids = std::collections::BTreeSet::new();
	let mut actual_rows = Vec::new();
	let mut runs = Vec::new();
	for (case, row) in cases.iter().zip(rows) {
		let id = string(case, "id")?;
		if string(row, "id")? != id
			|| !ids.insert(id)
			|| !id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
		{
			return Err("duplicate or malformed census id".into());
		}
		if string(case, "compiled_sha256")? != string(row, "input_sha256")?
			|| number(case, "source_bytes")? != number(row, "source_bytes")?
		{
			return Err("case and manifest input identity differ".into());
		}
		let input = pinned(&root, string(row, "input")?, string(row, "input_sha256")?)?;
		if fs::metadata(&input)?.len() != number(row, "source_bytes")? {
			return Err("census input size differs".into());
		}
		let expected_path = pinned(
			&root,
			string(row, "expected")?,
			string(row, "expected_sha256")?,
		)?;
		let stdout = records.join(format!("{id}.stdout"));
		let stderr = records.join(format!("{id}.stderr"));
		let mut command = Command::new("/usr/bin/timeout");
		command
			.args(["--signal=TERM", "--kill-after=5s", "20s"])
			.arg(&executable)
			.args(["--child", "census"])
			.arg(&input)
			.env_clear()
			.env("HOME", "/home/me")
			.env("PATH", "/usr/bin:/bin")
			.env("LANG", "C.UTF-8")
			.env("TZ", "UTC")
			.stdin(Stdio::null())
			.stdout(
				fs::OpenOptions::new()
					.create_new(true)
					.write(true)
					.open(&stdout)?,
			)
			.stderr(
				fs::OpenOptions::new()
					.create_new(true)
					.write(true)
					.open(&stderr)?,
			)
			.current_dir(&out);
		let began = Instant::now();
		let status = command.status()?;
		let receipt = json!({"id":id,"input":input,"input_sha256":string(row,"input_sha256")?,"program":executable,"args":["--child","census",input],"timeout_seconds":20,"seconds":began.elapsed().as_secs_f64(),"code":status.code(),"stdout_sha256":hash_file(&stdout)?,"stderr_sha256":hash_file(&stderr)?});
		write_new(
			&records.join(format!("{id}-run.json")),
			&serde_json::to_vec_pretty(&receipt)?,
		)?;
		if !status.success() || fs::metadata(&stderr)?.len() != 0 {
			return Err(fail(format!(
				"census child {id} failed; raw files retained"
			)));
		}
		let actual = read_json(&stdout)?;
		if actual["lifecycle_closed"] != true
			|| actual["execution"] != "not run; compile-only"
			|| actual["registry"].as_array().map(Vec::len)
				!= Some(number(&manifest, "registry_rows")? as usize)
		{
			return Err(fail(format!(
				"invalid census lifecycle/compile-only/registry witness: {id}"
			)));
		}
		let record = records.join(format!("{id}.json"));
		write_new(&record, &serde_json::to_vec_pretty(&actual)?)?;
		if actual != read_json(&expected_path)?
			|| hash_file(&record)? != string(row, "expected_sha256")?
		{
			return Err(fail(format!("census observation changed: {id}")));
		}
		let mut aggregate = actual
			.as_object()
			.ok_or("observation object required")?
			.clone();
		aggregate.insert("case".into(), case.clone());
		actual_rows.push(Value::Object(aggregate));
		runs.push(receipt);
		pinned(&root, string(row, "input")?, string(row, "input_sha256")?)?;
		pinned(
			&root,
			string(row, "expected")?,
			string(row, "expected_sha256")?,
		)?;
	}
	let observations = out.join("OBSERVATIONS.json");
	write_new(&observations, &canonical(&json!({"rnx":actual_rows}))?)?;
	if hash_file(&observations)? != string(&manifest, "aggregate_sha256")?
		|| fs::metadata(&observations)?.len() != number(&manifest, "aggregate_bytes")?
	{
		return Err("canonical census aggregate differs from frozen artifact".into());
	}
	pinned(
		&root,
		string(&manifest, "cases_path")?,
		string(&manifest, "cases_sha256")?,
	)?;
	check_registration(&manifest, repo)?;
	if hash_file(&executable)? != executable_hash {
		return Err("census executable changed".into());
	}
	let result = json!({"status":"PASS","cases":244,"changed":0,"unchanged":244,"execution":"not run; compile-only","observation_sha256":hash_file(&observations)?,"observations":observations,"binary":executable,"binary_sha256":executable_hash,"seconds":start.elapsed().as_secs_f64(),"runs":runs});
	write_new(
		&out.join("RESULT.json"),
		&serde_json::to_vec_pretty(&result)?,
	)?;
	Ok(result)
}

/// Plain checks are called by the harness-free controller, not only libtest.
pub(super) fn self_check() -> CensusResult<Value> {
	let v = json!({"rnx":[{"case":{"origins":[{"origin":"z","cohort":"x"}],"source_bytes":0,"compiled_sha256":"a","path":"logical","id":"i"},"doc":"±😀"}]});
	let encoded = String::from_utf8(canonical(&v)?)?;
	if !encoded.contains("\\u00b1\\ud83d\\ude00")
		|| !encoded.ends_with('\n')
		|| encoded.find("\"id\"").ok_or("missing canonical id")?
			>= encoded.find("\"path\"").ok_or("missing canonical path")?
		|| encoded
			.find("\"cohort\"")
			.ok_or("missing canonical cohort")?
			>= encoded
				.find("\"origin\"")
				.ok_or("missing canonical origin")?
	{
		return Err("canonical ordered ASCII serialization check failed".into());
	}
	if canonical(&json!({"rnx":[{"case":{"surprise":1}}]})).is_ok()
		|| canonical(&json!({"float":1.5})).is_ok()
	{
		return Err("canonical malformed metadata was accepted".into());
	}
	if child(&[]).is_ok() || child(&["one".into(), "two".into()]).is_ok() {
		return Err("census child accepted invalid argument selection".into());
	}
	if run(&[]).is_ok()
		|| run(&["--unknown".into(), "x".into()]).is_ok()
		|| run(&["--out".into(), "one".into(), "--out".into(), "two".into()]).is_ok()
	{
		return Err("census controller accepted malformed options".into());
	}
	if pinned(Path::new("."), "../outside", "").is_ok()
		|| pinned(Path::new("."), "/outside", "").is_ok()
	{
		return Err("unsafe fixture selector was accepted".into());
	}
	Ok(
		json!({"status":"PASS","canonical_order_ascii":true,"unknown_metadata_refused":true,
        "float_metadata_refused":true,"invalid_child_selections_refused":2,
        "invalid_controller_selections_refused":3,"unsafe_fixture_paths_refused":2}),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn canonical_metadata_order_and_ascii() {
		let v = json!({"rnx":[{"case":{"origins":[{"origin":"z","cohort":"x"}],"source_bytes":0,"compiled_sha256":"a","path":"logical","id":"i"},"doc":"±😀"}]});
		let s = String::from_utf8(canonical(&v).unwrap()).unwrap();
		assert!(s.contains("\\u00b1\\ud83d\\ude00"));
		assert!(s.find("\"id\"").unwrap() < s.find("\"path\"").unwrap());
		assert!(s.find("\"cohort\"").unwrap() < s.find("\"origin\"").unwrap());
		assert!(s.ends_with("\n"));
	}
	#[test]
	fn canonical_rejects_unknown_or_float_metadata() {
		assert!(canonical(&json!({"rnx":[{"case":{"surprise":1}}]})).is_err());
		assert!(canonical(&json!({"float":1.5})).is_err());
	}

	#[test]
	fn plain_controller_checks_pass() {
		self_check().unwrap();
	}
	#[test]
	fn child_requires_one_input() {
		assert!(child(&[]).is_err());
	}
}
