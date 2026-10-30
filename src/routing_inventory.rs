//! R1b: executable routing policy, confined to test/controller builds.
#![deny(clippy::all)]
mod syntax;
mod weights;
use rune::experimental_core::{
	error::{ErrorKind, Refusal},
	execute::Executor,
	frontend,
	heap::{Limits, Store},
	value::Type,
};
use rune::{Context, Diagnostics, Hash, Source, Sources, TypeHash};
use serde_json::{Value as Json, json};
use std::{
	collections::{BTreeMap, BTreeSet},
	path::Path,
};

pub(crate) type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
	Typed,
	DynamicOutput,
	Undeclared,
	Opaque,
}
#[derive(Clone, Copy, Debug)]
struct HostPolicy {
	path: &'static str,
	args: &'static str,
	class: Class,
	input: &'static str,
	output: &'static str,
}
fn host_policy() -> Vec<HostPolicy> {
	let mut rows = Vec::new();
	let mut add = |paths: &'static str, args, class, input, output| {
		for path in paths.split_whitespace() {
			rows.push(HostPolicy {
				path,
				args,
				class,
				input,
				output,
			});
		}
	};
	add(
		"json::parse",
		"\"{}\"",
		Class::DynamicOutput,
		"&str",
		"JSON_RESULT: owned readable acyclic tree",
	);
	add(
		"io::stdin",
		"",
		Class::Typed,
		"none",
		"Result<String,String>",
	);
	add(
		"io::eprint web::decode_component fs::read fs::exists fs::absolute fs::mkdir fs::mkdir_all fs::remove_file fs::remove_dir fs::remove_dir_all path::parent path::file_name path::file_stem path::extension path::is_absolute time::parse",
		"\"\"",
		Class::Typed,
		"&str",
		"closed scalar/Option/Result",
	);
	add(
		"fs::cwd fs::temp_dir path::separator time::now_ms time::monotonic_ms",
		"",
		Class::Typed,
		"none",
		"closed scalar/Result",
	);
	add(
		"fs::copy fs::rename path::join path::with_extension text::find",
		"\"\",\"\"",
		Class::Typed,
		"(&str,&str)",
		"closed scalar/Option/Result",
	);
	add(
		"time::format",
		"0,\"\",\"\"",
		Class::Typed,
		"(i64,&str,&str)",
		"Result<String,String>",
	);
	add(
		"time::rfc3339",
		"0,\"\"",
		Class::Typed,
		"(i64,&str)",
		"Result<String,String>",
	);
	add("text::group_digits", "1", Class::Typed, "i64", "String");
	add(
		"json::stringify",
		"()",
		Class::Undeclared,
		"Value: outside typed input set",
		"Result<String,String>",
	);
	add(
		"process::run process::run_bytes",
		"\"\",[],#{}",
		Class::Undeclared,
		"Value args/options",
		"undeclared dynamic output",
	);
	add(
		"process::exit",
		"0",
		Class::Undeclared,
		"i64: importable, unenrolled",
		"Result<(),String>: importable, unenrolled",
	);
	add(
		"web::escape_html",
		"\"\"",
		Class::Undeclared,
		"&str",
		"Result<String,VmError>: unsupported HostOutput",
	);
	add(
		"web::parse_form",
		"()",
		Class::Undeclared,
		"Value: outside typed input set",
		"undeclared dynamic output",
	);
	add(
		"web::response",
		"200,#{},\"\",()",
		Class::Undeclared,
		"Value options/body",
		"undeclared dynamic output",
	);
	add(
		"web::html web::text",
		"200,()",
		Class::Undeclared,
		"Value body",
		"undeclared dynamic output",
	);
	add(
		"fs::read_bytes",
		"\"\"",
		Class::Undeclared,
		"&str",
		"Bytes: no Root representation",
	);
	add(
		"fs::write_new fs::write fs::append",
		"\"\",\"\"",
		Class::Undeclared,
		"Value: outside typed input set",
		"Result<(),String>",
	);
	add(
		"fs::metadata",
		"\"\"",
		Class::Undeclared,
		"&str",
		"Result<Value,String>: no output certificate",
	);
	add(
		"fs::read_dir",
		"\"\"",
		Class::Undeclared,
		"&str",
		"Vec output: outside typed output set",
	);
	add(
		"time::parts",
		"0,\"\"",
		Class::Undeclared,
		"(i64,&str)",
		"Result<Value,String>: no output certificate",
	);
	add(
		"time::from_parts",
		"#{},\"\"",
		Class::Undeclared,
		"Object: outside typed input set",
		"Result<i64,String>",
	);
	add(
		"time::sleep",
		"0",
		Class::Undeclared,
		"i64",
		"future: no new polling boundary",
	);
	add(
		"text::split_max",
		"\"\",1",
		Class::Undeclared,
		"(&str,i64)",
		"Result<Vec<String>,String>: unsupported typed output",
	);
	add(
		"env::args",
		"",
		Class::Undeclared,
		"none",
		"Vec<String>: unsupported typed output",
	);
	add(
		"env::var",
		"\"\"",
		Class::Undeclared,
		"&str",
		"Result<Option<String>,String>: unsupported nested typed output",
	);
	add(
		"env::home_dir",
		"",
		Class::Undeclared,
		"none",
		"Result<Option<String>,String>: unsupported nested typed output",
	);
	add(
		"env::vars",
		"",
		Class::Undeclared,
		"none",
		"Result<Value,String>: no output certificate",
	);
	add(
		"http::get http::get_bytes",
		"\"\"",
		Class::Undeclared,
		"&str",
		"tracked future",
	);
	add(
		"http::request http::request_bytes",
		"\"GET\",\"\",#{}",
		Class::Undeclared,
		"Value options",
		"tracked future",
	);
	add(
		"interchange::Dense::shape interchange::Dense::names interchange::Dense::dtype",
		"",
		Class::Opaque,
		"&Dense: no Root representation",
		"opaque receiver",
	);
	add(
		"interchange::Dense::<DISPLAY_FMT>",
		"",
		Class::Opaque,
		"&Dense and &mut Formatter",
		"opaque receiver",
	);
	rows
}
fn context() -> Result<(
	Context,
	crate::lifecycle::Lifecycle,
	Vec<crate::host::HostFunction>,
)> {
	let life = crate::lifecycle::Lifecycle::new(true)?;
	let mut c = Context::with_default_modules()?;
	let mut names = crate::install_core(&mut c)?;
	names.extend(crate::fs::install(&mut c)?);
	names.extend(crate::path::install(&mut c)?);
	names.extend(crate::time::install(&mut c)?);
	names.extend(crate::text::install(&mut c)?);
	names.extend(crate::env::install(&mut c, std::sync::Arc::from([]))?);
	let http = crate::http::State::default();
	names.extend(crate::http::install(&mut c, &http, life.scope("http"))?);
	Ok((c, life, names))
}
fn sources(text: &str) -> Sources {
	let mut s = Sources::new();
	s.insert(Source::new("routing.rn", text).unwrap()).unwrap();
	s
}
fn compile(
	c: &Context,
	text: &str,
) -> std::result::Result<frontend::Program, rune::experimental_core::error::Error> {
	frontend::compile(&mut sources(text), c, &mut Diagnostics::new(), 1)
}
fn host_inventory(root: &Path) -> Result<Json> {
	let (c, life, names) = context()?;
	let policies = host_policy();
	let mut keys = BTreeSet::new();
	let mut rows = Vec::new();
	let actual = syntax::registrations(root)?;
	let templates = syntax::registration_templates(root)?;
	for r in templates.iter().filter(|r| !r.file.starts_with("src/")) {
		assert_eq!(
			r.class, "undeclared",
			"adapter template enrolled without a routing review: {}",
			r.file
		);
	}
	let base: BTreeMap<_, _> = actual
		.iter()
		.filter(|r| r.file.starts_with("src/"))
		.map(|r| (r.path.clone(), r.class.as_str()))
		.collect();
	let expected: BTreeSet<_> = policies.iter().map(|p| p.path.to_owned()).collect();
	let observed: BTreeSet<_> = base.keys().cloned().collect();
	if observed != expected {
		return Err(format!(
			"unclassified/missing production host registrations: added {:?}; missing {:?}",
			observed.difference(&expected).collect::<Vec<_>>(),
			expected.difference(&observed).collect::<Vec<_>>()
		)
		.into());
	}
	let help: BTreeSet<_> = names
		.iter()
		.map(|p| p.path.as_str())
		.filter(|p| *p != "interchange::Dense")
		.collect();
	let mut declared: BTreeSet<_> = policies
		.iter()
		.map(|p| p.path)
		.filter(|p| !p.contains("<DISPLAY_FMT>"))
		.collect();
	if cfg!(feature = "test-support") {
		for p in &help {
			if p.starts_with("rnx_test::") {
				declared.insert(p);
			}
		}
	}
	assert_eq!(
		help, declared,
		"help and real live registrations must not drift"
	);
	let runtime = c.runtime()?;
	for p in policies {
		assert!(keys.insert(p.path), "duplicate host policy");
		let class = match p.class {
			Class::Typed => "typed",
			Class::DynamicOutput => "dynamic-output",
			Class::Undeclared | Class::Opaque => "undeclared",
		};
		assert_eq!(
			base[p.path], class,
			"{} enrollment changed without a routing policy",
			p.path
		);
		let hash = if p.path == "interchange::Dense::<DISPLAY_FMT>" {
			Hash::associated_function(
				crate::interchange::Dense::HASH,
				rune::runtime::Protocol::DISPLAY_FMT.hash,
			)
		} else if let Some(name) = p.path.strip_prefix("interchange::Dense::") {
			Hash::associated_function(crate::interchange::Dense::HASH, Hash::ident(name))
		} else {
			let mut parts = p.path.split("::");
			let item = rune::ItemBuf::with_crate_item(parts.next().unwrap(), parts)?;
			Hash::type_hash(&item)
		};
		assert!(
			runtime.function(&hash).is_some(),
			"missing live handler {}",
			p.path
		);
		let source = if p.class == Class::Opaque {
			None
		} else {
			Some(format!("pub fn main(){{{}({})}}", p.path, p.args))
		};
		let verdict = if let Some(s) = &source {
			let observed = compile(&c, s);
			match p.class {
				Class::Typed | Class::DynamicOutput => {
					assert!(observed.is_ok(), "{}: {:?}", p.path, observed.err());
					"conditional-admission"
				}
				Class::Undeclared => {
					assert_eq!(
						observed.err().and_then(|e| e.refusal),
						Some(Refusal::Native),
						"{}",
						p.path
					);
					"compile-refusal:Native"
				}
				Class::Opaque => unreachable!(),
			}
		} else {
			"not-reachable:opaque-receiver"
		};
		rows.push(json!({"path":p.path,"hash":hash.to_string(),"signature_class":class,"input_shape":p.input,"output_shape":p.output,"verdict":verdict,"source":source}));
	}
	let adapters: Vec<_> = actual
		.iter()
		.filter(|r| !r.file.starts_with("src/"))
		.collect();
	assert!(!adapters.is_empty());
	for r in &adapters {
		assert_eq!(
			r.class, "undeclared",
			"adapter host was enrolled without a routing policy: {} {}",
			r.file, r.path
		);
	}
	let mut counts = BTreeMap::new();
	for r in &rows {
		*counts
			.entry(r["verdict"].as_str().unwrap())
			.or_insert(0usize) += 1;
	}
	life.close()?;
	Ok(
		json!({"rows":rows,"counts":counts,"registration_evidence":actual,"registration_templates":templates,"rust_function_shapes":syntax::function_shapes(root)?,"adapter_registrations":adapters,"adapter_rule":"ordinary legacy registrations are undeclared; typed Rust signatures alone do not enroll a new-engine handler"}),
	)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
	New,
	Fallback,
	Blocked,
}
struct Entry {
	name: &'static str,
	route: Route,
	file: &'static str,
	owner: &'static str,
	call: &'static str,
	needs: &'static str,
}
fn entries() -> Vec<Entry> {
	use Route::*;
	vec![
		Entry {
			name: "file-run",
			route: Fallback,
			file: "src/runner.rs",
			owner: "run_loaded",
			call: "CALL execution::run",
			needs: "entry/root/result/error/budget/lifecycle adapters",
		},
		Entry {
			name: "module-and-project-sources",
			route: Fallback,
			file: "src/runner.rs",
			owner: "compile",
			call: "METHOD with_source_loader",
			needs: "select legacy before effects while external loader is unsupported",
		},
		Entry {
			name: "project-run",
			route: Fallback,
			file: "tools/project/src/workflow.rs",
			owner: "launch",
			call: "CALL assembly::command_checked",
			needs: "launch options forwarded to the artifact without entering its cache identity",
		},
		Entry {
			name: "generated-project-assembly",
			route: Fallback,
			file: "tools/project/src/generate.rs",
			owner: "wrapper",
			call: "CALL wrapper_style",
			needs: "actual generated wrapper and artifact routes are exercised by dual-path CLI acceptance",
		},
		Entry {
			name: "one-shot-eval",
			route: Blocked,
			file: "src/lib.rs",
			owner: "serve",
			call: "METHOD eval",
			needs: "Ctrl-C cancellation requires a separately designed resumable boundary",
		},
		Entry {
			name: "config",
			route: Fallback,
			file: "src/config.rs",
			owner: "evaluate_selected",
			call: "CALL execution::evaluate",
			needs: "with_config(false), settings schema and budget/source-offset adapters",
		},
		Entry {
			name: "persistent-session",
			route: Blocked,
			file: "src/session.rs",
			owner: "execute",
			call: "CALL Vm::new",
			needs: "cross-program functions, shared identity, partial mutation and resumable slices",
		},
		Entry {
			name: "worker-jupyter",
			route: Blocked,
			file: "src/worker.rs",
			owner: "run",
			call: "METHOD eval_with_armed",
			needs: "persistent session, interrupt and transport presentation",
		},
		Entry {
			name: "dep-in-session",
			route: Blocked,
			file: "src/repl.rs",
			owner: "run",
			call: "CALL crate::dep_transition::prepare",
			needs: "session ownership across assembly transition",
		},
		Entry {
			name: "polars-file-run",
			route: Fallback,
			file: "adapters/polars/src/main.rs",
			owner: "main",
			call: "CALL rnx::main_with",
			needs: "uncertified/opaque adapter calls select legacy",
		},
		Entry {
			name: "candle-file-run",
			route: Fallback,
			file: "adapters/candle/src/main.rs",
			owner: "main",
			call: "CALL rnx::main_with",
			needs: "uncertified/opaque adapter calls select legacy",
		},
		Entry {
			name: "postgres-file-run",
			route: Fallback,
			file: "adapters/postgres/src/main.rs",
			owner: "main",
			call: "CALL rnx::main_with",
			needs: "query futures select legacy",
		},
		Entry {
			name: "server-invocation",
			route: Blocked,
			file: "src/server.rs",
			owner: "prepare_with",
			call: "METHOD runtime",
			needs: "Value/root boundary, polling and compile/runtime handler pairing",
		},
		Entry {
			name: "server-slot",
			route: Blocked,
			file: "src/server.rs",
			owner: "slot",
			call: "METHOD runtime",
			needs: "per-slot handlers, native state, lifecycle generation and async retirement",
		},
		Entry {
			name: "http-serve",
			route: Blocked,
			file: "src/http_server/worker.rs",
			owner: "invoke",
			call: "METHOD rune",
			needs: "Bytes envelope, route metadata, polling/cancellation and slot lifetime",
		},
		Entry {
			name: "http-postgres",
			route: Blocked,
			file: "servers/http-postgres/src/main.rs",
			owner: "worker",
			call: "METHOD prepare",
			needs: "Bytes envelope, leased callbacks and database future",
		},
		Entry {
			name: "selfcheck",
			route: Blocked,
			file: "src/lib.rs",
			owner: "serve",
			call: "CALL session::checks",
			needs: "retained legacy Unit/function identity",
		},
		Entry {
			name: "fixed-startup-probe",
			route: New,
			file: "src/lib.rs",
			owner: "serve",
			call: "CALL engine::startup",
			needs: "typed compile/prepare/invoke scalar witness",
		},
	]
}

fn policy_rows(root: &Path) -> Result<Json> {
	let source = syntax::inventory(root)?;
	let mut rows = Vec::new();
	let mut counts = BTreeMap::new();
	let mut missing = Vec::new();
	for e in entries() {
		assert!(!e.needs.is_empty());
		if !source
			.iter()
			.any(|site| site.file == e.file && site.owner == e.owner && site.tokens == e.call)
		{
			missing.push(format!("{} {}::{} {}", e.name, e.file, e.owner, e.call));
		}
		let verdict = match e.route {
			Route::New => "route-to-new",
			Route::Fallback => "route-with-fallback",
			Route::Blocked => "blocked",
		};
		*counts.entry(verdict).or_insert(0usize) += 1;
		rows.push(json!({"path":e.name,"verdict":verdict,"source_file":e.file,"source_owner":e.owner,"requires":e.needs,"current_route":if matches!(e.route,Route::Blocked){"legacy"}else{"execution with pre-effect selection"}}));
	}
	for (file, owner, call) in [
		(
			"src/runner.rs",
			"run_loaded",
			"CALL crate::engine::requires_loader",
		),
		(
			"src/runner/execution.rs",
			"attempt",
			"CALL frontend::compile",
		),
		("src/runner/execution.rs", "invoke", "METHOD prepare"),
		("src/runner/execution.rs", "invoke", "METHOD invoke"),
		(
			"src/config/execution.rs",
			"evaluate",
			"CALL frontend::compile",
		),
		("src/config/execution.rs", "evaluate", "METHOD prepare"),
		("src/config/execution.rs", "evaluate", "METHOD invoke"),
		("src/engine.rs", "startup", "METHOD prepare"),
		("src/engine.rs", "startup", "METHOD invoke"),
		(
			"tools/project/src/workflow/git_sources.rs",
			"git_launch",
			"CALL assembly::command_checked",
		),
		(
			"src/http_server/value.rs",
			"rune",
			"CALL rune::runtime::Bytes::from_slice",
		),
		("src/server.rs", "execute", "METHOD resume"),
		("src/server.rs", "prepare_with", "METHOD runtime"),
		("src/server.rs", "slot", "METHOD runtime"),
	] {
		if !source
			.iter()
			.any(|site| site.file == file && site.owner == owner && site.tokens == call)
		{
			missing.push(format!("{file}::{owner} {call}"));
		}
	}

	assert!(
		missing.is_empty(),
		"routing wiring mismatches: {missing:#?}"
	);

	// New interpreter construction sites must be assigned an entry policy;
	// adding a file or alias cannot silently inherit a one-shot verdict.
	let constructors = syntax::vm_constructors(root)?;
	let allowed: BTreeSet<_> = [
		("src/config.rs", "evaluate_selected"),
		("src/lib.rs", "call"),
		("src/runner.rs", "run_loaded"),
		("src/server.rs", "http_function"),
		("src/server.rs", "execute"),
		("src/session.rs", "execute"),
	]
	.into_iter()
	.collect();
	for site in &constructors {
		assert!(
			allowed.contains(&(site.file.as_str(), site.owner.as_str())),
			"unclassified VM constructor: {}::{}",
			site.file,
			site.owner
		);
	}
	for &(file, owner) in &allowed {
		assert!(
			constructors
				.iter()
				.any(|s| s.file == file && s.owner == owner),
			"missing VM boundary {file}::{owner}"
		);
	}
	Ok(
		json!({"rows":rows,"counts":counts,"vm_boundaries":constructors,"observations":"live routing policy; API invariants belong to Rune, per-path runtime evidence belongs to CLI comparisons"}),
	)
}

fn session_blocker() -> Result<Json> {
	let (c, life, _) = context()?;
	let mut legacy = crate::session::Session::new(c)?;
	let inputs = [
		"let n=7;let c=||n;let shared=[1];0",
		"shared.push(2);c()",
		"shared.len()",
	];
	let mut generated = Vec::new();
	let mut values = Vec::new();
	for input in inputs {
		let value = legacy.eval(input)?;
		values.push(value.as_integer::<i64>()?);
		generated.push(legacy.last_generated().to_owned());
	}
	assert_eq!(values, [0, 7, 2]);
	legacy.close()?;
	life.close()?;
	let (context, life, _) = context()?;
	let mut store = Store::new(Limits::default()).map_err(|e| format!("{e:?}"))?;
	let mut executor = Executor::new().map_err(|e| format!("{e:?}"))?;
	executor
		.set_host_runtime(Some(rune::sync::Arc::try_new(context.runtime()?)?))
		.map_err(|e| format!("{e:?}"))?;
	let mut state = store.root_record(None, &[]).map_err(|e| format!("{e:?}"))?;
	let mut programs = Vec::new();
	let mut fresh = Vec::new();
	for (i, source) in generated.iter().take(2).enumerate() {
		let program = frontend::compile(
			&mut sources(source),
			&context,
			&mut Diagnostics::new(),
			i as u64,
		)
		.map_err(|e| format!("{e:?}"))?;
		let entry = program
			.function(Hash::type_hash(["main"]))
			.ok_or("missing generated entry")?;
		assert!(program.entry_eligible(entry));
		match executor.invoke(&program, &mut store, entry, &[&state], 10_000) {
			Ok(result) => {
				assert_eq!(i, 0);
				let delta = store.root_get(&result, 0).map_err(|e| format!("{e:?}"))?;
				let value = store.root_get(&result, 1).map_err(|e| format!("{e:?}"))?;
				fresh.push(json!({"returned":store.root_scalar(&value).map_err(|e|format!("{e:?}"))? .map(|s|format!("{s:?}"))}));
				store.root_release(value).unwrap();
				store.root_release(result).unwrap();
				store.root_release(state).unwrap();
				state = delta;
			}
			Err(failure) => {
				assert_eq!(i, 1);
				assert_eq!(failure.error.kind, ErrorKind::Type);
				assert_eq!(failure.error.refusal, None);
				assert!(
					failure
						.message(&program)
						.to_string()
						.contains("another program")
				);
				let shared = store
					.root_field(&state, "shared")
					.map_err(|e| format!("{e:?}"))?;
				assert_eq!(store.root_len(&shared).map_err(|e| format!("{e:?}"))?, 2);
				store.root_release(shared).unwrap();
				fresh.push(json!({"runtime_failure":failure.message(&program).to_string(),"refusal":null,"shared_len_after":2}));
			}
		}
		programs.push(program);
	}
	assert_eq!(programs.len(), 2);
	store.root_release(state).unwrap();
	store.close().map_err(|e| format!("{e:?}"))?;
	life.close()?;
	Ok(
		json!({"inputs":inputs,"generated_sources":generated,"legacy":values,"execution":fresh,"verdict":"blocked: never retry after runtime effects"}),
	)
}
fn synchronous_and_byte_boundaries() -> Result<Json> {
	// Exhaustive: a new Root representation requires an explicit inventory decision.
	fn represented(t: Type) -> &'static str {
		match t {
			Type::Unit
			| Type::Bool
			| Type::I64
			| Type::U64
			| Type::F64
			| Type::Char
			| Type::Ordering => "scalar",
			Type::String | Type::Vec | Type::Tuple | Type::Object | Type::Option | Type::Result => {
				"builtin payload"
			}
			Type::Struct(_) => "script nominal",
			Type::Range
			| Type::RangeFrom
			| Type::RangeFull
			| Type::RangeInclusive
			| Type::RangeToInclusive
			| Type::RangeTo => "range",
			Type::Iterator => "builtin iterator",
			Type::Function => "same-program function",
		}
	}
	assert_eq!(represented(Type::Function), "same-program function");
	// Rune owns the exhaustive public-API invariant. rnx asserts the exact
	// types it consumes, without parsing source from a different repository.
	let _: fn(&mut Executor) = Executor::request_cancel;
	type PrepareEntry = for<'a> fn(
		&'a mut Executor,
		&'a frontend::Program,
		&'a mut Store,
		u32,
		&'a [&'a rune::experimental_core::value::Root],
	) -> std::result::Result<
		rune::experimental_core::execute::PreparedInvocation<'a>,
		rune::experimental_core::execute::PrepareError,
	>;
	let _: PrepareEntry = Executor::prepare;
	Ok(
		json!({"Bytes":"no Root type; HTTP input envelope cannot be imported","executor_api":"synchronous invoke, exclusive cancellation, no resumable polling/slice API"}),
	)
}

/// Runs inside the bound dev executable before its cross-suite completes.
pub(crate) fn run(root: &Path, rune_root: &Path) -> Result<Json> {
	syntax::self_check();
	weights::self_check();
	Ok(
		json!({"host_shapes":host_inventory(root)?,"entry_paths":policy_rows(root)?,"session":session_blocker()?,"driver_and_bytes":synchronous_and_byte_boundaries()?,"source_weights":weights::run(root,rune_root)?}),
	)
}
#[test]
fn registered_host_shapes_and_source_entry_paths_match_policy() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	host_inventory(root).unwrap();
	policy_rows(root).unwrap();
}
#[test]
fn retained_function_fails_after_mutation_so_session_fallback_is_unsafe() {
	session_blocker().unwrap();
}

#[test]
#[ignore = "requires RNX_VERIFY_RUNE_ROOT; cross --suite all runs this inventory"]
fn complete_inventory_of_bound_sources() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"));
	let rune = std::env::var_os("RNX_VERIFY_RUNE_ROOT").expect("explicit Rune source checkout");
	let report = run(root, Path::new(&rune)).unwrap();
	if let Some(out) = std::env::var_os("RNX_R1B_OUT") {
		std::fs::write(out, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
	}
}
