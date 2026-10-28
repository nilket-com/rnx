//! Versioned real-registration enrollment observer and isolated controller.
use rune::experimental_core::{
	execute::Executor,
	frontend,
	heap::{Limits, Store},
	value::{Root, Scalar, Type},
};
use rune::{Context, Diagnostics, Hash, Source, Sources};
use serde_json::{Value as Json, json};
use std::path::Path;

const OBSERVER_FUEL: usize = 100_000;
const DEPTH: usize = 4;

fn hex(bytes: &[u8]) -> String {
	bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn scalar(value: Scalar) -> Json {
	match value {
		Scalar::Unit => json!({"unit": true}),
		Scalar::Bool(v) => json!({"bool": v}),
		Scalar::I64(v) => json!({"i64": v}),
		Scalar::U64(v) => json!({"u64": v}),
		Scalar::F64(v) => json!({"f64_bits": format!("{:016x}", v.to_bits())}),
		Scalar::Char(v) => json!({"char": v as u32}),
		Scalar::Ordering(_) => panic!("unreviewed Ordering output"),
	}
}
fn typed_stock(value: &rune::Value, depth: usize) -> Json {
	use rune::TypeHash;
	assert!(depth <= DEPTH, "observer depth exceeded");
	let hash = value.type_hash();
	if value.into_unit().is_ok() {
		return scalar(Scalar::Unit);
	}
	if hash == <bool as TypeHash>::HASH {
		return scalar(Scalar::Bool(value.as_bool().unwrap()));
	}
	if hash == <i64 as TypeHash>::HASH {
		return scalar(Scalar::I64(value.as_integer::<i64>().unwrap()));
	}
	if hash == <u64 as TypeHash>::HASH {
		return scalar(Scalar::U64(value.as_integer::<u64>().unwrap()));
	}
	if hash == <f64 as TypeHash>::HASH {
		return scalar(Scalar::F64(value.as_float().unwrap()));
	}
	if hash == <char as TypeHash>::HASH {
		return scalar(Scalar::Char(value.as_char().unwrap()));
	}
	if let Ok(v) = value.borrow_string_ref() {
		return json!({"string_hex": hex(v.as_bytes())});
	}
	if let Ok(v) = value.borrow_ref::<Option<rune::Value>>() {
		return json!({"variant": "Option", "success": v.is_some(), "payload": (*v).as_ref().map(|v| typed_stock(v, depth + 1))});
	}
	if let Ok(v) = value.borrow_ref::<Result<rune::Value, rune::Value>>() {
		let (success, child) = match &*v {
			Ok(v) => (true, v),
			Err(v) => (false, v),
		};
		return json!({"variant": "Result", "success": success, "payload": typed_stock(child, depth + 1)});
	}
	if let Ok(v) = value.borrow_tuple_ref() {
		return json!({"tuple": v.iter().map(|v| typed_stock(v, depth + 1)).collect::<Vec<_>>()});
	}
	panic!("unreviewed stock output: {}", value.type_info());
}

fn typed_fresh(
	store: &mut Store,
	executor: &mut Executor,
	program: &frontend::Program,
	root: &Root,
	depth: usize,
) -> Json {
	assert!(depth <= DEPTH, "observer depth exceeded");
	if let Some(v) = store.root_scalar(root).unwrap() {
		return scalar(v);
	}
	match store.root_tag(root).unwrap() {
		Type::String => json!({"string_hex": hex(store.root_text(root).unwrap().as_bytes())}),
		Type::Tuple => {
			let mut children = Vec::new();
			for i in 0..store.root_len(root).unwrap() {
				let child = store.root_get(root, i).unwrap();
				children.push(typed_fresh(store, executor, program, &child, depth + 1));
				store.root_release(child).unwrap();
			}
			json!({"tuple": children})
		}
		Type::Option | Type::Result => {
			let (tag, success, payload) = store.root_variant_parts(root).unwrap();
			let helper = if tag == Type::Option {
				"r0187_option"
			} else {
				"r0187_result"
			};
			let id = program
				.function(Hash::type_hash([helper]))
				.expect("pure projector export missing");
			let projected = executor
				.invoke(program, store, id, &[root], OBSERVER_FUEL)
				.expect("observer invocation failed; never a subject failure");
			assert_eq!(store.root_tag(&projected).unwrap(), Type::Tuple);
			assert_eq!(store.root_len(&projected).unwrap(), 2);
			let flag = store.root_get(&projected, 0).unwrap();
			assert!(
				matches!(store.root_scalar(&flag).unwrap(), Some(Scalar::Bool(v)) if v == success)
			);
			store.root_release(flag).unwrap();
			let child = store.root_get(&projected, 1).unwrap();
			let result = if payload {
				Some(typed_fresh(store, executor, program, &child, depth + 1))
			} else {
				assert!(matches!(
					store.root_scalar(&child).unwrap(),
					Some(Scalar::Unit)
				));
				None
			};
			store.root_release(child).unwrap();
			store.root_release(projected).unwrap();
			json!({"variant": if tag == Type::Option { "Option" } else { "Result" }, "success": success, "payload": result})
		}
		_ => panic!("unreviewed fresh output"),
	}
}

fn context(lifecycle: &crate::lifecycle::Lifecycle) -> (Context, crate::http::State) {
	let mut context = Context::with_default_modules().unwrap();
	crate::install_core(&mut context).unwrap();
	crate::fs::install(&mut context).unwrap();
	crate::path::install(&mut context).unwrap();
	crate::time::install(&mut context).unwrap();
	crate::text::install(&mut context).unwrap();
	let http = crate::http::State::default();
	crate::http::install(&mut context, &http, lifecycle.scope("http")).unwrap();
	crate::env::install(&mut context, std::sync::Arc::from([])).unwrap();
	(context, http)
}
fn sources(source: &str) -> Sources {
	let mut s = Sources::new();
	s.insert(Source::new("case.rn", source).unwrap()).unwrap();
	s
}
fn write(path: &Path, value: &Json) {
	use std::io::Write;
	std::fs::OpenOptions::new()
		.write(true)
		.create_new(true)
		.open(path)
		.unwrap()
		.write_all(&serde_json::to_vec_pretty(value).unwrap())
		.unwrap();
}
fn wall_ms() -> i64 {
	std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.unwrap()
		.as_millis() as i64
}
// Complete anchored Display grammar; no row, source or policy input.
fn classify_stock_display(
	primary: &str,
	chain: &[String],
) -> Option<(&'static str, Json, Option<usize>)> {
	fn quoted(s: &str) -> Option<&str> {
		let s = s.strip_prefix('`')?.strip_suffix('`')?;
		(!s.is_empty() && !s.contains('`')).then_some(s)
	}
	let mut matches = Vec::new();
	if let Some(s) = primary.strip_prefix("Wrong number of arguments ")
		&& let Some((a, e)) = s.split_once(", expected ")
		&& !a.is_empty()
		&& !e.is_empty()
		&& a.bytes().all(|b| b.is_ascii_digit())
		&& e.bytes().all(|b| b.is_ascii_digit())
	{
		matches.push(("BadArgumentCount", json!({"actual":a,"expected":e})));
	}
	if let Some(s) = primary.strip_prefix("Expected type ")
		&& let Some((e, a)) = s.split_once(" but found ")
		&& let (Some(e), Some(a)) = (quoted(e), quoted(a))
	{
		matches.push(("ExpectedType", json!({"expected":e,"actual":a})));
	}
	if let Some(a) = primary
		.strip_prefix("Expected number type, but found ")
		.and_then(quoted)
	{
		matches.push(("ExpectedNumber", json!({"actual":a})));
	}
	if matches.len() != 1 || chain.len() > 1 {
		return None;
	}
	let arg = if let Some(s) = chain.first() {
		match s.as_str() {
			"Bad argument #0" => Some(0),
			"Bad argument #1" => Some(1),
			_ => return None,
		}
	} else {
		None
	};
	let (kind, captures) = matches.pop().unwrap();
	Some((kind, captures, arg))
}
fn classifier_controls() {
	for bad in [
		"Expected number type, but found `bool` ",
		"Expected type `bool but found `i64`",
		"Expected type `a`, but found `b`",
		"Bad argument #2",
		"",
		"Missing field `a`",
	] {
		assert!(classify_stock_display(bad, &[]).is_none());
	}
	assert!(
		classify_stock_display(
			"Expected number type, but found `bool`",
			&["Bad argument #2".into()]
		)
		.is_none()
	);
	assert!(
		classify_stock_display(
			"Expected number type, but found `bool`",
			&["Bad argument #0".into(), "Bad argument #1".into()]
		)
		.is_none()
	);
	for good in [
		"Wrong number of arguments 0, expected 1",
		"Expected type `bool` but found `i64`",
		"Expected number type, but found `bool`",
	] {
		assert!(classify_stock_display(good, &[]).is_some());
	}
}
fn stock_error(e: &rune::runtime::VmError, output: &Path) -> Json {
	let display = e.error().to_string();
	let chain = e
		.chain()
		.iter()
		.map(ToString::to_string)
		.collect::<Vec<_>>();
	let classification = classify_stock_display(&display, &chain);
	write(
		&output.with_extension("raw-error.json"),
		&json!({"engine":"stock","error":format!("{e:?}"),"display":display,"chain":chain,"classification":classification.as_ref().map(|(k,c,a)|json!({"primary":k,"captures":c,"arg":a}))}),
	);
	let Some((primary, captures, arg)) = classification else {
		return json!({"status":"classifier_reject","message":display,"chain":chain});
	};
	let mut sites = Vec::new();
	for at in e.stacktrace() {
		assert!(at.frames.is_empty(), "unreviewed stock frames");
		let inst = at
			.unit
			.debug_info()
			.and_then(|d| d.instruction_at(at.ip))
			.expect("stock site missing");
		let range = inst.span.range();
		sites.push(
			json!({"source":inst.source_id.into_index(), "start":range.start,"end":range.end}),
		);
	}
	assert_eq!(sites.len(), 1, "unreviewed stock trace shape");
	json!({"status":"error", "primary":primary,"arg":arg,"message":display,"captures":captures,"site":sites[0],"frames":[]})
}
fn fresh_error(
	f: &rune::experimental_core::execute::Failure,
	program: &frontend::Program,
	output: &Path,
) -> Json {
	use rune::experimental_core::error::{Detail, ErrorKind as K, HostCause as H};
	write(
		&output.with_extension("raw-error.json"),
		&json!({"engine":"fresh","error":format!("{:?}",f.error),"host":format!("{:?}",f.host_message),"cleanup":format!("{:?}",f.cleanup_error),"frames":format!("{:?}",f.call_chain)}),
	);
	assert!(f.cleanup_error.is_none(), "secondary cleanup error");
	let (primary, arg) = match (f.error.kind, f.error.detail) {
		(
			K::Host,
			Detail::HostMessage {
				cause: H::ExpectedNumber,
				arg,
			},
		) => ("ExpectedNumber", arg),
		(
			K::Host,
			Detail::HostMessage {
				cause: H::ExpectedType,
				arg,
			},
		) => ("ExpectedType", arg),
		(K::Host, Detail::HostExpectedType { arg, .. }) => ("ExpectedType", arg),
		(K::Host, Detail::HostExpectedNumber { arg, .. }) => ("ExpectedNumber", arg),
		(K::FuelExhausted, _) => ("FuelExhausted", None),
		(K::StoreCapacity, _) => ("StoreCapacity", None),
		(K::InvalidCode, _) => ("InvalidCode", None),
		_ => panic!("unreviewed fresh error"),
	};
	assert!(f.call_chain.is_empty(), "unreviewed fresh frames");
	let site = f
		.error
		.location
		.map(|s| json!({"source":s.source,"start":s.start,"end":s.end}));
	let message = f.message(program).to_string();
	let mut result = json!({"status":"error","primary":primary,"arg":arg,"message":message,"site":site,"frames":[]});
	if matches!(primary, "ExpectedType" | "ExpectedNumber") {
		let chain = arg
			.map(|n| vec![format!("Bad argument #{n}")])
			.unwrap_or_default();
		let (kind, captures, projected_arg) =
			classify_stock_display(&message, &chain).expect("unreviewed fresh host Display");
		assert_eq!(kind, primary);
		fn native_hash(name: &str) -> rune::Hash {
			match name {
				"std::string::String" => <rune::alloc::String as rune::TypeHash>::HASH,
				"std::i64" => <i64 as rune::TypeHash>::HASH,
				"std::f64" => <f64 as rune::TypeHash>::HASH,
				_ => panic!("unreviewed host type name"),
			}
		}
		match f.error.detail {
			Detail::HostExpectedType {
				expected, actual, ..
			} => {
				assert_eq!(
					expected,
					native_hash(captures["expected"].as_str().unwrap())
				);
				assert_eq!(actual, native_hash(captures["actual"].as_str().unwrap()));
			}
			Detail::HostExpectedNumber { actual, .. } => {
				assert_eq!(actual, native_hash(captures["actual"].as_str().unwrap()));
			}
			Detail::HostMessage { .. } => {}
			_ => panic!("unreviewed host type detail"),
		}

		assert_eq!(projected_arg, arg.map(usize::from));
		result["captures"] = captures;
	}
	result
}

fn invoke_fresh(
	executor: &mut Executor,
	store: &mut Store,
	program: &frontend::Program,
	main: u32,
	args: &[&Root],
	fuel: usize,
	output: &Path,
) -> Json {
	match executor.invoke(program, store, main, args, fuel) {
		Ok(root) => {
			let v = typed_fresh(store, executor, program, &root, 0);
			store.root_release(root).unwrap();
			json!({"status":"value","typed":v})
		}
		Err(f) => fresh_error(&f, program, output),
	}
}
fn fresh(
	row: &Json,
	ctx: &Context,
	lifecycle: &crate::lifecycle::Lifecycle,
	source: &str,
	output: &Path,
) -> Json {
	let mut src = sources(source);
	let mut diagnostics = Diagnostics::new();
	let built = frontend::compile(&mut src, ctx, &mut diagnostics, 1);
	write(
		&output.with_extension("compile.json"),
		&json!({"diagnostics":format!("{:?}",diagnostics.diagnostics())}),
	);
	let program = match built {
		Ok(p) => p,
		Err(e) => {
			write(
				&output.with_extension("raw-refusal.json"),
				&json!({"error":format!("{e:?}")}),
			);
			assert_eq!(
				row["policy"], "Retained-arity",
				"unrelated compilation refusal"
			);
			assert_eq!(
				e.refusal,
				Some(rune::experimental_core::error::Refusal::Arity)
			);
			return json!({"outcome":{"status":"refused","primary":"Arity"},"extra":null,"wall_before":0,"wall_after":0,"balance":true,"observer_fuel":0,"helper_checks":null});
		}
	};
	let main = program.function(Hash::type_hash(["main"])).unwrap();
	assert!(
		program
			.function(Hash::type_hash(["r0187_option"]))
			.is_some()
	);
	assert!(
		program
			.function(Hash::type_hash(["r0187_result"]))
			.is_some()
	);
	let mut store = Store::new(Limits {
		entries: row["store_entries"].as_u64().unwrap() as usize,
		bytes: row["store_bytes"].as_u64().unwrap() as usize,
	})
	.unwrap();
	let mut executor = Executor::new().unwrap();
	let runtime = rune::sync::Arc::try_new(ctx.runtime().unwrap()).unwrap();
	let control = row["control"].as_str();
	let mut wrong_keepalive = None;
	if control == Some("wrong") {
		let (other, http) = context(lifecycle);
		executor
			.set_host_runtime(Some(
				rune::sync::Arc::try_new(other.runtime().unwrap()).unwrap(),
			))
			.unwrap();
		wrong_keepalive = Some((other, http));
	} else if control != Some("absent") && control != Some("unused-table") {
		executor.set_host_runtime(Some(runtime.clone())).unwrap();
	}
	if control == Some("cleared") {
		executor.set_host_runtime(None).unwrap();
	}
	let args: Vec<Root> = row["args"]
		.as_array()
		.unwrap()
		.iter()
		.map(|v| store.root_bool(v.as_bool().unwrap()))
		.collect();
	let refs: Vec<_> = args.iter().collect();
	let mut helper_checks = Json::Null;
	if row["watch"] == "helper-control" {
		executor.set_host_runtime(None).unwrap();
		let child = store.root_string("probe").unwrap();
		let mut checks = Vec::new();
		for (tag, success, payload) in [
			(Type::Result, true, true),
			(Type::Result, false, true),
			(Type::Option, true, true),
			(Type::Option, false, false),
		] {
			let root = store
				.root_variant(tag, success, if payload { Some(&child) } else { None })
				.unwrap();
			let observed = typed_fresh(&mut store, &mut executor, &program, &root, 0);
			let expected = json!({"variant":if tag==Type::Option{"Option"}else{"Result"},"success":success,"payload":if payload{Some(json!({"string_hex":hex(b"probe")}))}else{None}});
			assert_eq!(observed, expected);
			checks.push(observed);
			store.root_release(root).unwrap();
		}
		store.root_release(child).unwrap();
		helper_checks = json!(checks);
	}
	let before = wall_ms();
	let outcome = invoke_fresh(
		&mut executor,
		&mut store,
		&program,
		main,
		&refs,
		row["fuel"].as_u64().unwrap() as usize,
		output,
	);
	let after = wall_ms();
	let mut extra = Json::Null;
	if let Some(control) = control {
		if control != "reuse" {
			assert_eq!(
				outcome["primary"], "InvalidCode",
				"missing runtime accepted = HOLD"
			);
			assert_eq!(
				std::fs::metadata(row["stderr_path"].as_str().unwrap())
					.unwrap()
					.len(),
				0
			);
			assert!(!Path::new("w").exists());
			let negative_tree = tree_snapshot(Path::new("."));
			assert!(executor.is_idle());
			executor.set_host_runtime(Some(runtime.clone())).unwrap();
			let recovery = invoke_fresh(
				&mut executor,
				&mut store,
				&program,
				main,
				&refs,
				100_000,
				&output.with_extension("recovery"),
			);
			assert_eq!(recovery["status"], "value");
			if control == "unused-table" {
				assert_eq!(recovery["typed"], json!({"i64":7}));
				assert_eq!(
					std::fs::metadata(row["stderr_path"].as_str().unwrap())
						.unwrap()
						.len(),
					0
				);
				assert!(!Path::new("w").exists());
			}
			if control == "cleared" {
				executor.set_host_runtime(None).unwrap();
				let mut src = sources("pub fn main(){42}");
				let free = frontend::compile(&mut src, ctx, &mut Diagnostics::new(), 1).unwrap();
				let id = free.function(Hash::type_hash(["main"])).unwrap();
				let free_result = invoke_fresh(
					&mut executor,
					&mut store,
					&free,
					id,
					&[],
					100_000,
					&output.with_extension("host-free"),
				);
				assert_eq!(free_result["typed"], json!({"i64":42}));
			}
			extra = json!({"control":control,"negative_tree":negative_tree,"negative_stderr_bytes":0,"recovery":recovery});
		} else {
			assert_eq!(outcome["status"], "value");
			let second = invoke_fresh(
				&mut executor,
				&mut store,
				&program,
				main,
				&refs,
				100_000,
				&output.with_extension("second"),
			);
			assert_eq!(second["status"], "value");
			extra = json!({"control":control,"second":second});
		}
	}
	for arg in args {
		store.root_release(arg).unwrap();
	}
	assert!(executor.is_idle());
	assert_eq!(store.live_entries(), 0);
	assert_eq!(store.payload_bytes(), 0);
	drop(wrong_keepalive);
	json!({"outcome":outcome,"extra":extra,"wall_before":before,"wall_after":after,"balance":true,"observer_fuel":OBSERVER_FUEL,"helper_checks":helper_checks})
}

pub fn tree_snapshot(root: &Path) -> Json {
	fn visit(base: &Path, path: &Path, out: &mut Vec<Json>) {
		let mut entries = std::fs::read_dir(path)
			.unwrap()
			.map(|e| e.unwrap().path())
			.collect::<Vec<_>>();
		entries.sort();
		for p in entries {
			let m = std::fs::symlink_metadata(&p).unwrap();
			use std::os::unix::fs::PermissionsExt;
			let kind = if m.is_symlink() {
				"symlink"
			} else if m.is_dir() {
				"dir"
			} else if m.is_file() {
				"file"
			} else {
				"other"
			};
			let mut r = json!({"path":p.strip_prefix(base).unwrap().to_string_lossy(),"kind":kind,"mode":m.permissions().mode()&0o7777});
			if m.is_symlink() {
				r["target"] = json!(std::fs::read_link(&p).unwrap().to_string_lossy());
			}
			if m.is_file() {
				r["size"] = json!(m.len());
				r["content_hex"] = json!(hex(&std::fs::read(&p).unwrap()));
			}
			out.push(r);
			if m.is_dir() {
				visit(base, &p, out);
			}
		}
	}
	let mut out = Vec::new();
	visit(root, root, &mut out);
	json!(out)
}

fn child_inner(args: &[String]) -> Json {
	// The frozen0192 controller applied umask077 before exec. This standalone
	// child sets that same process policy before creating lifecycle threads or
	// executing any registered filesystem operation. Never alter observed modes.
	// SAFETY: umask takes an integer only; this child is still single-threaded
	// and owns its process policy. No controller or normal-product state changes.
	unsafe {
		libc::umask(0o077);
	}
	assert_eq!(args.len(), 2);
	let row: Json = serde_json::from_slice(&std::fs::read(&args[0]).unwrap()).unwrap();
	let engine = row["engine"].as_str().unwrap();
	let output = Path::new(&args[1]);
	classifier_controls();
	let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
	let (ctx, http) = context(&lifecycle);
	let source = std::fs::read_to_string(row["source"].as_str().unwrap()).unwrap();
	let observed = if engine == "fresh" {
		fresh(&row, &ctx, &lifecycle, &source, output)
	} else {
		assert_eq!(engine, "stock");
		let mut src = sources(&source);
		let mut diagnostics = Diagnostics::new();
		let mut vm = rune::prepare(&mut src)
			.with_context(&ctx)
			.with_diagnostics(&mut diagnostics)
			.build_vm()
			.unwrap();
		let before = wall_ms();
		let result = rune::runtime::budget::with(row["fuel"].as_u64().unwrap() as usize, || {
			vm.call(["main"], ())
		})
		.call();
		let after = wall_ms();
		let outcome = match result {
			Ok(v) => json!({"status":"value","typed":typed_stock(&v,0)}),
			Err(e) => {
				let observed = stock_error(&e, output);
				if row["watch"] == "classifier-control" {
					assert_eq!(observed["status"], "classifier_reject");
				} else {
					assert_ne!(
						observed["status"], "classifier_reject",
						"unreviewed stock error"
					);
				}
				observed
			}
		};
		json!({"outcome":outcome,"extra":null,"wall_before":before,"wall_after":after,"observer_fuel":0,"helper_checks":null})
	};
	let envelope = if row["watch"] == "wall-clock" {
		let value = observed["outcome"]["typed"]["i64"].as_i64().unwrap();
		let pass = observed["wall_before"].as_i64().unwrap() <= value
			&& value <= observed["wall_after"].as_i64().unwrap() + 1;
		assert!(pass);
		Some(pass)
	} else {
		None
	};
	drop(http);
	drop(ctx);
	lifecycle.close().unwrap();
	json!({"schema":1,"label":row["label"],"outcome":observed["outcome"],"extra":observed["extra"],"envelope":envelope,"wall_before":observed["wall_before"],"wall_after":observed["wall_after"],"observer_error":null,"balance":observed["balance"],"observer_fuel":observed["observer_fuel"],"helper_checks":observed["helper_checks"],"child_umask":"0077"})
}

pub(super) fn child(args: &[String]) -> Result<Json, Box<dyn std::error::Error>> {
	std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| child_inner(args))).map_err(|panic| {
		std::io::Error::other(
			panic
				.downcast_ref::<String>()
				.cloned()
				.or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
				.unwrap_or_else(|| "observer panic".into()),
		)
		.into()
	})
}

pub(super) fn self_check() -> Result<Json, Box<dyn std::error::Error>> {
	classifier_controls();
	let result = controller::self_check()?;
	Ok(
		json!({"status":"PASS","classifier_fixed_string_rejects":6,"classifier_chain_rejects":2,"controller":result}),
	)
}
pub(super) fn run(args: &[String]) -> Result<Json, Box<dyn std::error::Error>> {
	controller::run(args)
}

mod controller {
	use super::{Json, json};
	use std::{
		collections::BTreeMap,
		error::Error,
		fs::{self, File, OpenOptions},
		io::Write,
		os::unix::fs::{PermissionsExt, symlink},
		path::{Component, Path, PathBuf},
		process::{Command, Stdio},
		time::Instant,
	};
	type Result<T> = std::result::Result<T, Box<dyn Error>>;
	const MANIFEST: &str = include_str!("../../tools/verify/fixtures/enrollment/manifest.json");
	const TREE: &str = include_str!("../../tools/verify/fixtures/enrollment/tree.json");
	fn read(p: &Path) -> Result<Json> {
		Ok(serde_json::from_slice(&fs::read(p)?)?)
	}
	fn write(p: &Path, v: &Json) -> Result<()> {
		let mut f = OpenOptions::new().write(true).create_new(true).open(p)?;
		serde_json::to_writer_pretty(&mut f, v)?;
		f.write_all(b"\n")?;
		Ok(())
	}
	fn hex(bytes: &[u8]) -> String {
		bytes.iter().map(|b| format!("{b:02x}")).collect()
	}
	fn unhex(text: &str) -> Result<Vec<u8>> {
		if !text.len().is_multiple_of(2) {
			return Err(std::io::Error::other("odd hexadecimal length").into());
		}
		Ok(text
			.as_bytes()
			.as_chunks::<2>()
			.0
			.iter()
			.map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16))
			.collect::<std::result::Result<Vec<_>, _>>()?)
	}
	fn hash(path: &Path) -> Result<String> {
		let output = Command::new("/usr/bin/sha256sum")
			.arg("--")
			.arg(path)
			.output()?;
		assert!(
			output.status.success(),
			"hash child failed: {}",
			path.display()
		);
		let text = String::from_utf8(output.stdout)?;
		let digest = text.split_whitespace().next().ok_or("missing digest")?;
		assert!(
			digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()),
			"invalid SHA256 output"
		);
		Ok(digest.to_owned())
	}
	fn hash_bytes(bytes: &[u8]) -> Result<String> {
		let mut child = Command::new("/usr/bin/sha256sum")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.spawn()?;
		child.stdin.take().unwrap().write_all(bytes)?;
		let output = child.wait_with_output()?;
		assert!(output.status.success());
		Ok(String::from_utf8(output.stdout)?
			.split_whitespace()
			.next()
			.ok_or("missing digest")?
			.to_owned())
	}
	fn relative(path: &str) -> Result<&Path> {
		let p = Path::new(path);
		if p.as_os_str().is_empty() || p.components().any(|p| !matches!(p, Component::Normal(_))) {
			return Err(std::io::Error::other("invalid fixture relative path").into());
		}
		Ok(p)
	}
	fn checked_fixture(root: &Path, name: &str) -> Result<PathBuf> {
		let relative = relative(name)?;
		let mut path = root.to_path_buf();
		for part in relative.components() {
			path.push(part);
			if fs::symlink_metadata(&path)?.file_type().is_symlink() {
				return Err(std::io::Error::other(
					"fixture symlink is not an explicit rooted input",
				)
				.into());
			}
		}
		Ok(path)
	}
	fn spec_bytes(spec: &Json) -> Result<Vec<u8>> {
		if let Some(hex) = spec["hex"].as_str() {
			unhex(hex)
		} else if let Some(text) = spec["text"].as_str() {
			Ok(text.as_bytes().to_vec())
		} else {
			let n = spec["bytes"].as_u64().ok_or("missing repeat length")? as usize;
			assert!(n <= 64 << 20);
			let byte = spec["repeat_byte"]
				.as_u64()
				.map(|b| u8::try_from(b).unwrap())
				.unwrap_or_else(|| {
					let s = spec["repeat"].as_str().unwrap();
					assert_eq!(s.len(), 1);
					s.as_bytes()[0]
				});
			Ok(vec![byte; n])
		}
	}
	fn fixture(root: &Path) -> Result<()> {
		let spec: Json = serde_json::from_str(TREE)?;
		fs::create_dir(root)?;
		fs::set_permissions(root, fs::Permissions::from_mode(0o700))?;
		for (name, value) in spec["files"].as_object().unwrap() {
			let file = root.join(relative(name)?);
			fs::create_dir_all(file.parent().unwrap())?;
			fs::write(&file, spec_bytes(value)?)?;
			fs::set_permissions(file, fs::Permissions::from_mode(0o600))?;
		}
		for name in spec["dirs"].as_array().unwrap() {
			fs::create_dir_all(root.join(relative(name.as_str().unwrap())?))?;
		}
		fn modes(root: &Path) -> Result<()> {
			for entry in fs::read_dir(root)? {
				let p = entry?.path();
				if p.is_dir() {
					fs::set_permissions(&p, fs::Permissions::from_mode(0o700))?;
					modes(&p)?;
				}
			}
			Ok(())
		}
		modes(root)?;
		for (name, target) in spec["symlinks"].as_object().unwrap() {
			symlink(target.as_str().unwrap(), root.join(relative(name)?))?;
		}
		Ok(())
	}
	fn snapshot(root: &Path) -> Result<Json> {
		fn visit(base: &Path, dir: &Path, rows: &mut Vec<Json>) -> Result<()> {
			for entry in fs::read_dir(dir)? {
				let p = entry?.path();
				let meta = fs::symlink_metadata(&p)?;
				let kind = if meta.is_symlink() {
					"symlink"
				} else if meta.is_dir() {
					"dir"
				} else if meta.is_file() {
					"file"
				} else {
					"other"
				};
				let mut row = json!({"path":p.strip_prefix(base)?.to_string_lossy(),"kind":kind,"mode":meta.permissions().mode()&0o7777});
				if kind == "file" {
					row["size"] = json!(meta.len());
					row["sha256"] = json!(hash(&p)?);
				}
				if kind == "symlink" {
					row["target"] = json!(fs::read_link(&p)?.to_string_lossy());
				}
				rows.push(row);
				if meta.is_dir() {
					visit(base, &p, rows)?;
				}
			}
			Ok(())
		}
		let mut rows = Vec::new();
		visit(root, root, &mut rows)?;
		rows.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
		Ok(json!(rows))
	}
	fn semantic(record: &Json) -> Json {
		let o = &record["observation"];
		let mut outcome = o["outcome"].clone();
		if !o["envelope"].is_null() {
			assert_eq!(o["envelope"], true);
			assert_eq!(outcome["status"], "value");
			outcome = json!({"status":"value","clock-envelope":"passed"});
		}
		json!({"outcome":outcome,"stderr_hex":record["stderr_hex"],"stdout_sha256":record["record"]["stdout_sha256"],"tree":record["record"]["final_tree"],"envelope":o["envelope"]})
	}
	// Hide the host /home tree before creating canonical historical names.
	// Rebind only explicit physical inputs/executable/output, so absent old
	// directories are created on private tmpfs, never on the host filesystem.
	fn namespace_args(
		out: &Path,
		sources: &Path,
		child: &Path,
		scratch: &Path,
		canonical_parent: &Path,
	) -> Vec<String> {
		vec![
			"--die-with-parent".into(),
			"--ro-bind".into(),
			"/".into(),
			"/".into(),
			"--tmpfs".into(),
			"/home".into(),
			"--ro-bind".into(),
			sources.display().to_string(),
			sources.display().to_string(),
			"--ro-bind".into(),
			child.display().to_string(),
			child.display().to_string(),
			"--bind".into(),
			out.display().to_string(),
			out.display().to_string(),
			"--bind".into(),
			scratch.display().to_string(),
			canonical_parent.display().to_string(),
			"--proc".into(),
			"/proc".into(),
			"--dev".into(),
			"/dev".into(),
			"--chdir".into(),
			canonical_parent.join("F").display().to_string(),
		]
	}
	struct Runner {
		out: PathBuf,
		sources: PathBuf,
		child: PathBuf,
		canonical_parent: PathBuf,
		child_hash: String,
	}
	impl Runner {
		fn invoke(
			&self,
			row: &Json,
			engine: &str,
			phase: &str,
			iteration: Option<usize>,
		) -> Result<Json> {
			let id = row["id"].as_str().ok_or("missing row id")?;
			relative(id)?;
			let tag = iteration
				.map(|n| format!("{id}-fuel-{n:04}"))
				.unwrap_or_else(|| id.to_owned());
			let dir = self.out.join("records").join(phase).join(&tag);
			fs::create_dir_all(dir.parent().unwrap())?;
			fs::create_dir(&dir)?;
			let scratch = self.out.join("scratch").join(phase).join(&tag);
			fs::create_dir_all(scratch.parent().unwrap())?;
			fs::create_dir(&scratch)?;
			let actual = scratch.join("F");
			fixture(&actual)?;
			let initial = snapshot(&actual)?;
			let stdout = dir.join("stdout");
			let stderr = dir.join("stderr");
			let config = dir.join("config.json");
			let output = dir.join("observation.json");
			let mut config_row = row.clone();
			config_row["engine"] = json!(engine);
			config_row["source"] = json!(
				self.sources
					.join(relative(row["source"].as_str().unwrap())?)
			);
			config_row["fixture_root"] = json!(self.canonical_parent.join("F"));
			config_row["stderr_path"] = json!(stderr);
			write(&config, &config_row)?;
			let canonical = self.canonical_parent.join("F");
			let mut argv: Vec<String> = [
				"/usr/bin/prlimit",
				"--cpu=25",
				"--as=2147483648",
				"--fsize=67108864",
				"--core=0",
				"--",
				"/usr/bin/bwrap",
			]
			.into_iter()
			.map(str::to_owned)
			.collect();
			argv.extend(namespace_args(
				&self.out,
				&self.sources,
				&self.child,
				&scratch,
				&self.canonical_parent,
			));
			argv.extend([
				self.child.display().to_string(),
				"enrollment-child".into(),
				config.display().to_string(),
				output.display().to_string(),
			]);
			let mut command = Command::new("/usr/bin/timeout");
			command
				.args(["--signal=TERM", "--kill-after=2s", "30s"])
				.args(&argv)
				.env_clear()
				.env("PATH", "/usr/bin:/bin")
				.env("TZ", "UTC")
				.env("LANG", "C.UTF-8")
				.env("HOME", &canonical)
				.env("TMPDIR", canonical.join("tmp"));
			command
				.stdout(
					OpenOptions::new()
						.write(true)
						.create_new(true)
						.open(&stdout)?,
				)
				.stderr(
					OpenOptions::new()
						.write(true)
						.create_new(true)
						.open(&stderr)?,
				);
			let stdin = spec_bytes(&row["stdin_spec"])?;
			assert_eq!(hash_bytes(&stdin)?, row["stdin_sha256"].as_str().unwrap());
			command.stdin(if row["stdin_pipe"].as_bool().unwrap() {
				Stdio::piped()
			} else {
				Stdio::null()
			});
			let start = Instant::now();
			let mut child = command.spawn()?;
			let writer = child
				.stdin
				.take()
				.map(|mut pipe| std::thread::spawn(move || pipe.write_all(&stdin)));
			let status = child.wait()?;
			if let Some(writer) = writer
				&& let Err(error) = writer.join().unwrap()
			{
				assert_eq!(error.kind(), std::io::ErrorKind::BrokenPipe);
			}
			let final_tree = snapshot(&actual)?;
			let record = json!({"case":row["id"],"label":row["label"],"argv":argv,"env":{"TZ":"UTC","LANG":"C.UTF-8","HOME":canonical,"TMPDIR":canonical.join("tmp"),"PATH":"/usr/bin:/bin"},"rc":status.code(),"elapsed_s":start.elapsed().as_secs_f64(),"stdout_sha256":hash(&stdout)?,"stderr_sha256":hash(&stderr)?,"initial_tree":initial,"final_tree":final_tree,"child_binary_sha256":self.child_hash,"child_umask":"0077"});
			write(&dir.join("receipt.json"), &record)?;
			assert!(status.success(), "enrollment child failed {phase}/{tag}");
			assert!(fs::read(&stdout)?.is_empty(), "unexpected subject stdout");
			let observed = read(&output)?;
			assert_eq!(observed["schema"], 1);
			assert_eq!(
				observed["child_umask"], "0077",
				"child filesystem policy differs"
			);
			assert_eq!(observed["label"], row["label"]);
			assert!(observed["observer_error"].is_null());
			if engine == "fresh" {
				assert_eq!(observed["balance"], true);
			}
			let result = json!({"record":record,"observation":observed,"stderr_hex":hex(&fs::read(&stderr)?)});
			// Only this invocation's create-new scratch path is removed, never
			// the canonical historical path or any shared fixture directory.
			fs::remove_dir_all(&actual)?;
			Ok(result)
		}
	}
	pub(super) fn run(args: &[String]) -> Result<Json> {
		let mut options = BTreeMap::new();
		let (pairs, remainder) = args.as_chunks::<2>();
		assert!(remainder.is_empty());
		for pair in pairs {
			assert!(
				matches!(pair[0].as_str(), "--fixtures-root" | "--out"),
				"unknown enrollment option"
			);
			assert!(
				options.insert(pair[0].as_str(), pair[1].as_str()).is_none(),
				"duplicate enrollment option"
			);
		}
		let external = fs::canonicalize(
			options
				.get("--fixtures-root")
				.ok_or("fixtures root required")?,
		)?;
		let out = PathBuf::from(options.get("--out").ok_or("output required")?).join("enrollment");
		fs::create_dir(&out)?;
		let out = fs::canonicalize(out)?;
		write(&out.join("SELF-CHECKS.json"), &super::self_check()?)?;
		let bwrap = Command::new("/usr/bin/bwrap").arg("--version").output()?;
		assert!(bwrap.status.success(), "bubblewrap prerequisite failed");
		write(
			&out.join("BWRAP.json"),
			&json!({"path":"/usr/bin/bwrap","version":String::from_utf8(bwrap.stdout)?,"sha256":hash(Path::new("/usr/bin/bwrap"))?}),
		)?;
		let manifest: Json = serde_json::from_str(MANIFEST)?;
		let sources = checked_fixture(&external, "enrollment")?;
		let verify_sources = || -> Result<()> {
			for (path, digest) in manifest["source_pins"].as_object().unwrap() {
				assert_eq!(
					hash(&checked_fixture(&sources, path)?)?,
					digest.as_str().unwrap(),
					"source fixture drift"
				);
			}
			Ok(())
		};
		verify_sources()?;
		for row in manifest["rows"].as_array().unwrap() {
			let bytes = fs::read(sources.join(relative(row["source"].as_str().unwrap())?))?;
			let split = row["main_source_bytes"]
				.as_u64()
				.ok_or("missing main source boundary")? as usize;
			assert!(split <= bytes.len());
			assert_eq!(
				hash_bytes(&bytes[..split])?,
				row["main_source_sha256"].as_str().unwrap()
			);
			assert_eq!(
				hash_bytes(&bytes[split..])?,
				row["projector_suffix_sha256"].as_str().unwrap()
			);
		}

		let oracle_path = checked_fixture(&external, manifest["oracle"]["path"].as_str().unwrap())?;
		assert_eq!(
			hash(&oracle_path)?,
			manifest["oracle"]["sha256"].as_str().unwrap(),
			"frozen enrollment oracle drift"
		);
		assert_eq!(
			fs::metadata(&oracle_path)?.len(),
			manifest["oracle"]["bytes"].as_u64().unwrap()
		);
		let oracle = read(&oracle_path)?;
		assert_eq!(oracle["status"], "PASS");
		let oracle = oracle["records"]["after"]
			.as_array()
			.ok_or("missing stock oracle")?;
		let corpus = manifest["rows"].as_array().ok_or("missing corpus rows")?;
		assert_eq!(corpus.len(), 115);
		let rows: Vec<_> = corpus.iter().filter(|r| r["policy"] != "Control").collect();
		assert_eq!(rows.len(), 110);
		assert_eq!(oracle.len(), 110);
		let labels: Vec<_> = rows.iter().map(|r| r["label"].as_str().unwrap()).collect();
		let expected: Vec<_> = oracle
			.iter()
			.map(|r| r["record"]["label"].as_str().unwrap())
			.collect();
		assert_eq!(labels, expected);
		assert_eq!(
			labels
				.iter()
				.collect::<std::collections::BTreeSet<_>>()
				.len(),
			110
		);
		let child = std::env::current_exe()?;
		let runner = Runner {
			out: out.clone(),
			sources: sources.clone(),
			child_hash: hash(&child)?,
			child,
			canonical_parent: PathBuf::from(manifest["canonical_parent"].as_str().unwrap()),
		};
		write(
			&out.join("INPUTS.json"),
			&json!({"manifest_sha256":hash_bytes(MANIFEST.as_bytes())?,"tree_sha256":hash_bytes(TREE.as_bytes())?,"oracle":oracle_path,"oracle_sha256":manifest["oracle"]["sha256"],"oracle_bytes":manifest["oracle"]["bytes"],"source_pins":manifest["source_pins"],"child_binary":runner.child,"child_binary_sha256":runner.child_hash,"scratch_policy":"per-run and per-child create-new roots; canonical F only private namespace mount","child_umask":"0077; frozen0192 pre-exec policy set before lifecycle threads"}),
		)?;
		let started = Instant::now();
		let mut stock = Vec::new();
		for (row, old) in rows.iter().zip(oracle) {
			let got = runner.invoke(row, "stock", "stock", None)?;
			assert_eq!(
				semantic(&got),
				semantic(old),
				"stock historical delta {}",
				row["id"]
			);
			stock.push(got);
		}
		write(
			&out.join("STOCK.json"),
			&json!({"status":"PASS","records":stock}),
		)?;
		let mut fresh = Vec::new();
		let mut exact = 0;
		let mut arity = 0;
		let mut resource = 0;
		for (row, old) in rows.iter().zip(oracle) {
			let got = runner.invoke(row, "fresh", "fresh", None)?;
			match row["policy"].as_str().unwrap() {
				"Retained-arity" => {
					assert_eq!(got["observation"]["outcome"]["status"], "refused");
					assert_eq!(got["observation"]["outcome"]["primary"], "Arity");
					assert_eq!(got["stderr_hex"], "");
					assert_eq!(got["record"]["initial_tree"], got["record"]["final_tree"]);
					assert_eq!(old["observation"]["outcome"]["primary"], "BadArgumentCount");
					arity += 1;
				}
				"Retained-resource" => {
					assert_eq!(old["observation"]["outcome"]["status"], "value");
					assert_eq!(got["observation"]["outcome"]["status"], "error");
					assert_eq!(got["observation"]["outcome"]["primary"], "StoreCapacity");
					assert_eq!(got["stderr_hex"], "");
					assert_eq!(got["record"]["initial_tree"], got["record"]["final_tree"]);
					resource += 1;
				}
				_ => {
					assert_eq!(
						semantic(&got),
						semantic(old),
						"fresh exact delta {}",
						row["id"]
					);
					exact += 1;
				}
			}
			if row["watch"] == "fuel-full" {
				assert_eq!(got["stderr_hex"], "65".repeat(64));
				assert_eq!(old["stderr_hex"], "65".repeat(64));
			}
			fresh.push(got);
		}
		assert_eq!((exact, arity, resource), (107, 2, 1));
		let controlrows: Vec<_> = corpus.iter().filter(|r| r["policy"] == "Control").collect();
		assert_eq!(controlrows.len(), 5);
		let mut controls = Vec::new();
		for row in &controlrows {
			let got = runner.invoke(row, "fresh", "runtime-controls", None)?;
			let o = &got["observation"];
			let extra = &o["extra"];
			let initial = &got["record"]["initial_tree"];
			let final_tree = &got["record"]["final_tree"];
			if row["control"] == "reuse" {
				assert_eq!(got["stderr_hex"], "7777");
				assert_eq!(o["outcome"]["typed"]["variant"], "Result");
				assert_eq!(o["outcome"]["typed"]["success"], true);
				assert_eq!(extra["second"]["typed"]["variant"], "Result");
				assert_eq!(extra["second"]["typed"]["success"], false);
			} else {
				assert_eq!(o["outcome"]["primary"], "InvalidCode");
				assert_eq!(extra["negative_stderr_bytes"], 0);
				let mut negative = extra["negative_tree"].as_array().unwrap().clone();
				for entry in &mut negative {
					if entry["kind"] == "file" {
						let bytes = unhex(entry["content_hex"].as_str().unwrap())?;
						entry.as_object_mut().unwrap().remove("content_hex");
						entry["sha256"] = json!(hash_bytes(&bytes)?);
					}
				}
				negative.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
				assert_eq!(&json!(negative), initial);
				assert_eq!(extra["recovery"]["status"], "value");
				if row["control"] == "unused-table" {
					assert_eq!(final_tree, initial);
					assert_eq!(got["stderr_hex"], "");
					assert_eq!(extra["recovery"]["typed"], json!({"i64":7}));
				} else {
					assert_eq!(got["stderr_hex"], "77");
					assert_eq!(extra["recovery"]["typed"]["success"], true);
				}
			}
			if row["control"] != "unused-table" {
				let mut expected = initial.as_array().unwrap().clone();
				expected.push(json!({"path":"w","kind":"dir","mode":0o700}));
				expected.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
				assert_eq!(final_tree, &json!(expected));
			}
			controls.push(got);
		}
		let indexed: BTreeMap<_, _> = rows
			.iter()
			.zip(&fresh)
			.map(|(r, g)| (r["label"].as_str().unwrap(), g))
			.collect();
		let mut helpers = 0;
		let mut projections = 0;
		for row in &rows {
			let got = indexed[row["label"].as_str().unwrap()];
			if row["watch"] == "helper-control" {
				assert_eq!(
					got["observation"]["helper_checks"]
						.as_array()
						.unwrap()
						.len(),
					4
				);
				assert_eq!(got["record"]["initial_tree"], got["record"]["final_tree"]);
				assert_eq!(got["stderr_hex"], "");
				helpers += 4;
			}
			if row["watch"] == "projector-crosscheck" {
				let tuple = got["observation"]["outcome"]["typed"]["tuple"]
					.as_array()
					.unwrap();
				assert_eq!(tuple.len(), 2);
				let success = tuple[0]["bool"].as_bool().unwrap();
				let variant = &row["crosscheck_variant"];
				let payload = if variant == "Option" && !success {
					Json::Null
				} else {
					tuple[1].clone()
				};
				let canonical = json!({"variant":variant,"success":success,"payload":payload});
				assert_eq!(
					canonical,
					indexed[row["crosscheck_target"].as_str().unwrap()]["observation"]["outcome"]["typed"]
				);
				projections += 1;
			}
		}
		assert_eq!((helpers, projections), (4, 4));
		let fuelrow = rows.iter().find(|r| r["label"] == "fuel/full").unwrap();
		let mut frontier = Vec::new();
		let mut first = None;
		let mut previous = 0;
		for budget in 0..=manifest["max_frontier"].as_u64().unwrap() as usize {
			let mut row: Json = (**fuelrow).clone();
			row["fuel"] = json!(budget);
			row["watch"] = json!("fuel-frontier");
			let got = runner.invoke(&row, "fresh", "frontier", Some(budget))?;
			let effects = unhex(got["stderr_hex"].as_str().unwrap())?;
			assert!(effects.len() <= 64 && effects.iter().all(|b| *b == b'e'));
			assert!(effects.len() >= previous);
			previous = effects.len();
			let outcome = &got["observation"]["outcome"];
			let success = outcome["status"] == "value";
			if success {
				assert!(budget > 0);
				assert_eq!(effects.len(), 64);
				first = Some(budget);
			} else {
				assert_eq!(outcome["status"], "error");
				assert_eq!(outcome["primary"], "FuelExhausted");
			}
			frontier.push(got);
			if success {
				break;
			}
		}
		assert_eq!(first, Some(66));
		assert_eq!(frontier.len(), 67);
		let classifiers = manifest["classifier_rows"].as_array().unwrap();
		assert_eq!(classifiers.len(), 2);
		let mut classifier_results = Vec::new();
		for row in classifiers {
			let got = runner.invoke(row, "stock", "classifier-controls", None)?;
			assert_eq!(got["observation"]["outcome"]["status"], "classifier_reject");
			classifier_results.push(got);
		}
		verify_sources()?;
		assert_eq!(
			hash(&oracle_path)?,
			manifest["oracle"]["sha256"].as_str().unwrap()
		);
		assert_eq!(hash(&runner.child)?, runner.child_hash);
		write(
			&out.join("FRESH.json"),
			&json!({"status":"PASS","records":fresh,"runtime_controls":controls,"frontier":frontier,"classifier_subjects":classifier_results}),
		)?;
		let summary = json!({"status":"PASS","stock_rows":110,"execution_rows":110,"exact_rows":exact,"retained_arity":arity,"retained_resource":resource,"runtime_controls":5,"helper_checks":helpers,"projection_checks":projections,"frontier_budgets":67,"first_success_budget":66,"classifier_subject_rejects":2,"fixed_string_rejects":6,"invalid_chain_rejects":2,"seconds":started.elapsed().as_secs_f64(),"child_binary_sha256":runner.child_hash,"oracle_bytes":manifest["oracle"]["bytes"]});
		write(&out.join("RESULT.json"), &summary)?;
		Ok(summary)
	}
	mod checks {
		use super::*;
		pub(super) fn private_namespace_creates_missing_canonical_parents_without_host_changes() {
			let root = std::env::temp_dir()
				.join(format!("rnx-enrollment-namespace-{}", std::process::id()));
			fs::create_dir(&root).unwrap();
			let scratch = root.join("scratch");
			fs::create_dir(&scratch).unwrap();
			fs::create_dir(scratch.join("F")).unwrap();
			fs::write(scratch.join("F/sentinel"), "private").unwrap();
			let historical = PathBuf::from(std::env::var_os("HOME").unwrap())
				.join(format!(".rnx-enrollment-missing-{}", std::process::id()));
			assert!(!historical.exists());
			let args = namespace_args(
				&root,
				&root,
				Path::new("/usr/bin/test"),
				&scratch,
				&historical,
			);
			let status = Command::new("/usr/bin/bwrap")
				.args(args)
				.args(["/usr/bin/test", "-f", "sentinel"])
				.status()
				.unwrap();
			assert!(status.success());
			assert!(!historical.exists());
			// A preexisting host F is hidden rather than read, reset or deleted.
			fs::create_dir(&historical).unwrap();
			fs::create_dir(historical.join("F")).unwrap();
			fs::write(historical.join("F/host-sentinel"), "untouched").unwrap();
			let args = namespace_args(
				&root,
				&root,
				Path::new("/usr/bin/test"),
				&scratch,
				&historical,
			);
			assert!(
				Command::new("/usr/bin/bwrap")
					.args(args)
					.args(["/usr/bin/test", "!", "-e", "host-sentinel"])
					.status()
					.unwrap()
					.success()
			);
			assert_eq!(
				fs::read(historical.join("F/host-sentinel")).unwrap(),
				b"untouched"
			);
			fs::remove_dir_all(&historical).unwrap();
			fs::remove_dir_all(root).unwrap();
		}
		pub(super) fn paths_cannot_escape_fixture_root() {
			for p in ["../x", "/host/old", "a/../b", ""] {
				assert!(relative(p).is_err());
			}
			assert!(relative("sources/000.rn").is_ok());
		}
		pub(super) fn semantic_keeps_float_bits_and_filesystem_identity() {
			let a = json!({"observation":{"outcome":{"status":"value","typed":{"f64_bits":"8000000000000000"}},"envelope":null},"stderr_hex":"77","record":{"stdout_sha256":"x","final_tree":[{"mode":384,"path":"a","kind":"file","sha256":"x"}]}});
			let mut b = a.clone();
			b["observation"]["outcome"]["typed"]["f64_bits"] = json!("0000000000000000");
			assert_ne!(semantic(&a), semantic(&b));
			b = a.clone();
			b["record"]["final_tree"][0]["mode"] = json!(420);
			assert_ne!(semantic(&a), semantic(&b));
		}
	}
	pub(super) fn self_check() -> Result<Json> {
		checks::paths_cannot_escape_fixture_root();
		checks::semantic_keeps_float_bits_and_filesystem_identity();
		checks::private_namespace_creates_missing_canonical_parents_without_host_changes();
		Ok(
			json!({"status":"PASS","bad_paths":4,"float_bit_and_mode_negative_checks":2,"actual_namespace_checks":2,"host_sentinel":"untouched"}),
		)
	}
	#[cfg(test)]
	mod unit_tests {
		#[test]
		fn paths() {
			super::checks::paths_cannot_escape_fixture_root();
		}
		#[test]
		fn exact_values() {
			super::checks::semantic_keeps_float_bits_and_filesystem_identity();
		}
		#[test]
		fn namespace() {
			super::checks::private_namespace_creates_missing_canonical_parents_without_host_changes(
			);
		}
	}
}

#[test]
fn enrollment_classifier_boundaries() {
	classifier_controls();
}
