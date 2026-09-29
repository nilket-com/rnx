use crate::census::callback_census;
use crate::emit::callable::emit_callable;
use crate::emit::{Emitted, generics_map, routed};
use crate::model::{Callable, Inventory, Param, Supporting};
use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
use crate::text::{mentions, sanitize, split_top_on};
use crate::ty;
use crate::ty::{Ty, last};
use crate::world::World;
use crate::world::mapping::{Arg, Ret, Unsupported, ok_arg};
use std::collections::BTreeMap;

/// Record 0078 gate 1: every `From` impl and assignment/unary operator
/// with the name the rule gives and its disposition.
// ---- record 0079 gate 1: the callback census ----

/// A mutable closure argument the release file has audited: what Polars
/// does with the writes, with the citation.
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct CallbackMutable {
	pub(crate) path: String,
	pub(crate) param: String,
	/// `vector` (the slice is never read back: passed as a Rune vector of
	/// clones) or `result buffer` (the buffer's contents are the result:
	/// the Rune callback returns a string written into it).
	pub(crate) contract: String,
	pub(crate) cite: String,
}

/// Where an operation invokes one closure parameter, from the pinned
/// sources: `immediate` (before the call returns) or `stored` (kept by
/// Polars and invoked from the listed sinks), with the citation. A
/// lifetime bound is a signature fact, never the classification.
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct CallbackInvocation {
	pub(crate) path: String,
	pub(crate) param: String,
	pub(crate) invocation: String,
	#[serde(default)]
	pub(crate) sinks: Vec<String>,
	pub(crate) cite: String,
}

/// A method of a plan-holding type whose result is not the plan: an
/// execution sink (`plan execution`, `schema resolution`) or `none`, with
/// the citation. Every such method must be classified, or stored callbacks
/// are unresolved.
#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct CallbackSink {
	pub(crate) path: String,
	pub(crate) sink: String,
	pub(crate) cite: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct CallbackSafe {
	pub(crate) path: String,
	pub(crate) cite: String,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub(crate) struct CallbackRecipe {
	pub(crate) signature: String,
	pub(crate) rune: String,
	pub(crate) rust: String,
	#[serde(default)]
	pub(crate) uses: Vec<String>,
}

/// One closure parameter as the inventory spells it: the `Fn` kind, the
/// argument and return types, and whether the bound requires `'static`
/// (a signature fact; the invocation comes from the audit).
#[derive(Clone, Debug)]
pub(crate) struct ClosureSig {
	pub(crate) param: String,
	pub(crate) kind: String,
	pub(crate) args: Vec<String>,
	pub(crate) ret: String,
	/// The bound requires `'static`: Polars may keep the closure. A fact,
	/// not the invocation (an operation may invoke a `'static` closure
	/// immediately, or keep a borrowing closure in a lifetime-bound holder).
	pub(crate) static_bound: bool,
	pub(crate) udf: bool,
}

pub(crate) fn closure_signature(c: &Callable, p: &Param) -> Option<ClosureSig> {
	let generics = generics_map(c);
	let bare = p
		.ty_canonical
		.trim()
		.trim_start_matches("&mut ")
		.trim_start_matches('&')
		.trim();
	let text = generics
		.get(bare)
		.cloned()
		.unwrap_or_else(|| p.ty_canonical.clone());
	if text.contains("dyn ") && text.contains("Udf") {
		return Some(ClosureSig {
			param: p.name.clone(),
			kind: "Udf".into(),
			args: vec![],
			ret: String::new(),
			static_bound: true,
			udf: true,
		});
	}
	let mut t = text.trim();
	for prefix in ["&mut ", "&", "dyn ", "impl ", "core::ops::function::"] {
		t = t.trim_start_matches(prefix).trim();
	}
	// `Fn(A, B) -> R + 'static + Send`
	let kind = ["FnOnce", "FnMut", "Fn"]
		.iter()
		.find(|k| t.starts_with(**k) && t[k.len()..].starts_with('('))?
		.to_string();
	let rest = &t[kind.len()..];
	let close = {
		let mut depth = 0i32;
		let mut idx = None;
		for (i, ch) in rest.char_indices() {
			match ch {
				'(' | '<' | '[' => depth += 1,
				')' | '>' | ']' => {
					depth -= 1;
					if depth == 0 {
						idx = Some(i);
						break;
					}
				}
				_ => {}
			}
		}
		idx?
	};
	let args = split_top_on(&rest[1..close], ',');
	let after = rest[close + 1..].trim();
	let ret = match after.strip_prefix("->") {
		Some(r) => split_top_on(r.trim(), '+')
			.into_iter()
			.next()
			.unwrap_or_default(),
		None => "()".into(),
	};
	Some(ClosureSig {
		param: p.name.clone(),
		kind,
		args,
		ret,
		static_bound: text.contains("+ 'static") || text.contains("'static +"),
		udf: false,
	})
}

/// A plan-holding type: a method of one whose result is not a plan type
/// may execute or resolve the plan, and must be classified by the release
/// file's `[[callback_sink]]` audit (plan execution, schema resolution, or
/// none, each with a citation).
pub(crate) fn plan_holder_non_plan_method(c: &Callable) -> bool {
	const HOLDERS: &[&str] = &[
		"polars_lazy::frame::LazyFrame",
		"polars_plan::dsl::plan::DslPlan",
		"polars_plan::dsl::builder_dsl::DslBuilder",
		"polars_lazy::frame::JoinBuilder",
		"polars_lazy::frame::LazyGroupBy",
		"polars_plan::dsl::expr::Expr",
	];
	const PLAN_TYPES: &[&str] = &[
		"Self",
		"LazyFrame",
		"DslPlan",
		"DslBuilder",
		"JoinBuilder",
		"LazyGroupBy",
		"Expr",
		"OptFlags",
		"ExprIR",
		"Selector",
	];
	if !HOLDERS.iter().any(|h| c.owner == *h)
		|| c.kind == "foreign_trait_impl"
		|| c.bucket == "unsupported"
		|| c.bucket == "unknown"
	{
		return false;
	}
	let ret = c.ret_canonical.as_deref().unwrap_or("()");
	!PLAN_TYPES.iter().any(|t| mentions(ret, t))
}

/// Record 0079 gate 1 controls, from a synthetic inventory: the closure
/// classifier's dispositions and the sink rule.
pub(crate) fn callback_self_test() {
	fn sup(path: &str, derived: &[&str]) -> Supporting {
		Supporting {
			trait_params: vec![],
			key: path.to_string(),
			kind: "struct".into(),
			canonical_path: path.to_string(),
			found_paths: vec![format!("polars::{}", path.rsplit("::").next().unwrap())],
			crate_paths: vec![path.to_string()],
			public_fields: 0,
			fields_canonical: vec![],
			variant_shapes: vec![],
			variant_payloads: vec![],
			generic: false,
			lifetime: false,
			hidden: false,
			derived: derived.iter().map(|d| d.to_string()).collect(),
			alias_target: None,
			implementors: vec![],
			impls: vec![],
		}
	}
	let series = "polars_core::series::Series";
	let column = "polars_core::frame::column::Column";
	let field = "polars_core::datatypes::field::Field";
	let expr = "polars_plan::dsl::expr::Expr";
	let mk = |key: &str,
	          owner: &str,
	          name: &str,
	          receiver: &str,
	          params: Vec<(&str, &str)>,
	          generics: Vec<(&str, &str)>,
	          ret: Option<&str>,
	          owner_generic: bool| Callable {
		key: key.into(),
		kind: "inherent".into(),
		krate: "polars_core".into(),
		owner: owner.into(),
		name: name.into(),
		canonical_path: format!("{owner}::{name}"),
		found_paths: vec![],
		crate_paths: vec![],
		receiver: receiver.into(),
		params: params
			.iter()
			.map(|(n, t)| Param {
				name: n.to_string(),
				ty: t.to_string(),
				ty_canonical: t.to_string(),
			})
			.collect(),
		ret: None,
		ret_canonical: ret.map(String::from),
		generics_canonical: generics
			.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect(),
		impl_for: None,
		impl_bounds: vec![],
		impl_head: None,
		impl_where: vec![],
		impl_assoc: vec![],
		docs_first: None,
		owner_generic,
		is_unsafe: false,
		is_async: false,
		deprecated: false,
		hidden: false,
		implementors: vec![],
		trait_reachable: false,
		derived: false,
		bucket: "callback".into(),
		rules: vec![],
		trait_lifetimes: vec![],
	};
	let ca = "polars_core::chunked_array::ChunkedArray";
	let inv = Inventory {
		callables: vec![
			mk(
				"ok",
				column,
				"apply_unary_elementwise",
				"&self",
				vec![("f", "F")],
				vec![(
					"F",
					"impl core::ops::function::Fn(&polars_core::series::Series) -> polars_core::series::Series",
				)],
				Some(column),
				false,
			),
			mk(
				"stored",
				expr,
				"map",
				"self",
				vec![("function", "F"), ("output_type", "DT")],
				vec![
					(
						"F",
						"core::ops::function::Fn(polars_core::frame::column::Column) -> polars_error::PolarsResult<polars_core::frame::column::Column> + 'static + core::marker::Send + core::marker::Sync",
					),
					(
						"DT",
						"core::ops::function::Fn(&polars_core::schema::Schema, &polars_core::datatypes::field::Field) -> polars_error::PolarsResult<polars_core::datatypes::field::Field> + 'static + core::marker::Send + core::marker::Sync",
					),
				],
				Some("Self"),
				false,
			),
			mk(
				"free",
				column,
				"try_apply_with",
				"&self",
				vec![("f", "F")],
				vec![
					(
						"F",
						"core::ops::function::FnMut(polars_core::series::Series) -> core::result::Result<K, E>",
					),
					("K", ""),
					("E", ""),
				],
				Some("core::result::Result<K, E>"),
				false,
			),
			mk(
				"arrow",
				column,
				"apply_kernel",
				"&self",
				vec![("f", "F")],
				vec![(
					"F",
					"core::ops::function::Fn(&polars_arrow::array::Array) -> polars_arrow::array::ArrayRef",
				)],
				Some(column),
				false,
			),
			mk(
				"amort",
				column,
				"amortized",
				"&self",
				vec![("f", "F")],
				vec![(
					"F",
					"core::ops::function::FnMut(core::option::Option<polars_core::series::amortized_iter::AmortSeries>) -> polars_core::series::Series",
				)],
				Some(column),
				false,
			),
			mk(
				"udf",
				expr,
				"with_udf",
				"self",
				vec![(
					"schema",
					"core::option::Option<alloc::sync::Arc<dyn polars_plan::dsl::UdfSchema>>",
				)],
				vec![],
				Some("Self"),
				false,
			),
			mk(
				"borrowed",
				ca,
				"apply_mut",
				"&self",
				vec![("f", "F")],
				vec![("F", "core::ops::function::FnMut(&str) -> &str")],
				Some("Self"),
				true,
			),
			mk(
				"slice",
				expr,
				"map_many",
				"self",
				vec![
					("function", "F"),
					("arguments", "&[polars_plan::dsl::expr::Expr]"),
				],
				vec![(
					"F",
					"core::ops::function::Fn(&mut [polars_core::frame::column::Column]) -> polars_error::PolarsResult<polars_core::frame::column::Column> + 'static + core::marker::Send + core::marker::Sync",
				)],
				Some("Self"),
				false,
			),
			mk(
				"buffer",
				ca,
				"apply_into_string_amortized",
				"&self",
				vec![("f", "F")],
				vec![(
					"F",
					"core::ops::function::FnMut(T::Physical, &mut alloc::string::String)",
				)],
				Some("polars_core::datatypes::StringChunked"),
				true,
			),
			mk(
				"readback",
				"polars_core::schema::Schema",
				"retain_mut",
				"&mut self",
				vec![("f", "F")],
				vec![(
					"F",
					"core::ops::function::FnMut(&mut polars_core::datatypes::field::Field) -> bool",
				)],
				None,
				false,
			),
			mk(
				"native",
				ca,
				"apply_mut",
				"&mut self",
				vec![("f", "F")],
				vec![(
					"F",
					"core::ops::function::Fn(T::Native) -> T::Native + core::marker::Copy",
				)],
				None,
				true,
			),
			mk(
				"otherarg",
				column,
				"apply_with_state",
				"&self",
				vec![("f", "F"), ("state", "polars_arrow::bitmap::Bitmap")],
				vec![(
					"F",
					"core::ops::function::Fn(&polars_core::series::Series) -> polars_core::series::Series",
				)],
				Some(column),
				false,
			),
			mk(
				"sink",
				"polars_lazy::frame::LazyFrame",
				"collect",
				"self",
				vec![],
				vec![],
				Some("polars_error::PolarsResult<polars_core::frame::dataframe::DataFrame>"),
				false,
			),
			mk(
				"plan",
				"polars_lazy::frame::LazyFrame",
				"filter",
				"self",
				vec![("p", expr)],
				vec![],
				Some("Self"),
				false,
			),
		],
		supporting: vec![
			sup(series, &["Clone", "Debug"]),
			sup(column, &["Clone", "Debug"]),
			sup(field, &["Clone", "Debug"]),
			sup(expr, &["Clone", "Debug"]),
			sup("polars_core::schema::Schema", &["Clone", "Debug"]),
			sup("polars_core::datatypes::StringChunked", &["Clone"]),
			sup("polars_lazy::frame::LazyFrame", &["Clone"]),
			sup(
				"polars_core::frame::dataframe::DataFrame",
				&["Clone", "Debug"],
			),
		],
		provenance: None,
	};
	let mut release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope {
			families: vec!["numeric".into()],
			exclude: vec![],
		},
		api_crates: vec![
			"polars_core".into(),
			"polars_plan".into(),
			"polars_lazy".into(),
		],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	release.families.callback_mutable.push(CallbackMutable {
		path: format!("{expr}::map_many"),
		param: "function".into(),
		contract: "vector".into(),
		cite: "t".into(),
	});
	release.families.callback_mutable.push(CallbackMutable {
		path: format!("{ca}::apply_into_string_amortized"),
		param: "f".into(),
		contract: "result buffer".into(),
		cite: "t".into(),
	});
	// the source audit: every feasible closure gets its invocation; `stored`
	// names its sink groups; every non-plan-returning method of a plan holder is classified
	let audit = |path: String, param: &str, invocation: &str, sinks: &[&str]| CallbackInvocation {
		path,
		param: param.into(),
		invocation: invocation.into(),
		sinks: sinks.iter().map(|s| s.to_string()).collect(),
		cite: "t".into(),
	};
	for (path, param, inv_, sinks) in [
		(
			format!("{column}::apply_unary_elementwise"),
			"f",
			"immediate",
			vec![],
		),
		(
			format!("{expr}::map"),
			"function",
			"stored",
			vec!["plan execution"],
		),
		(
			format!("{expr}::map"),
			"output_type",
			"stored",
			vec!["schema resolution"],
		),
		(
			format!("{expr}::map_many"),
			"function",
			"stored",
			vec!["plan execution"],
		),
		(
			format!("{ca}::apply_into_string_amortized"),
			"f",
			"immediate",
			vec![],
		),
		(format!("{ca}::apply_mut"), "f", "immediate", vec![]),
	] {
		release
			.families
			.callback_invocation
			.push(audit(path, param, inv_, &sinks));
	}
	release.families.callback_sink.push(CallbackSink {
		path: "polars_lazy::frame::LazyFrame::collect".into(),
		sink: "plan execution".into(),
		cite: "t".into(),
	});
	release.families.callback_sink.push(CallbackSink {
		path: "polars_lazy::frame::LazyFrame::collect_schema".into(),
		sink: "schema resolution".into(),
		cite: "t".into(),
	});
	let mut inv = inv;
	inv.callables.push(mk(
		"schema",
		"polars_lazy::frame::LazyFrame",
		"collect_schema",
		"self",
		vec![],
		vec![],
		Some("polars_error::PolarsResult<polars_core::schema::SchemaRef>"),
		false,
	));
	// a `'static` closure the audit does not cover: neither stored nor immediate, unresolved
	inv.callables.push(mk(
		"unaudited",
		column,
		"apply_later",
		"&self",
		vec![("f", "F")],
		vec![(
			"F",
			"core::ops::function::Fn(&polars_core::series::Series) -> polars_core::series::Series + 'static",
		)],
		Some(column),
		false,
	));
	let world = World::new(&inv, &release, &["mechanical", "conversion", "callback"]);
	let census = callback_census(&world, &inv, &[]);
	let row = |key: &str| {
		census["rows"]
			.as_array()
			.unwrap()
			.iter()
			.find(|r| r["key"] == key)
			.unwrap_or_else(|| panic!("no row {key}"))
			.clone()
	};
	let disp = |key: &str| row(key)["disposition"].as_str().unwrap().to_string();
	let refusals = |key: &str| {
		row(key)["refusals"]
			.as_array()
			.map(|v| {
				v.iter()
					.map(|x| x.as_str().unwrap().to_string())
					.collect::<Vec<_>>()
			})
			.unwrap_or_default()
	};
	let unresolved = |key: &str| {
		row(key)["unresolved"]
			.as_array()
			.map(|v| {
				v.iter()
					.map(|x| x.as_str().unwrap().to_string())
					.collect::<Vec<_>>()
			})
			.unwrap_or_default()
	};
	assert_eq!(
		disp("ok"),
		"feasible",
		"{:?} {:?}",
		refusals("ok"),
		unresolved("ok")
	);
	assert_eq!(row("ok")["invocation"][0]["invocation"], "immediate");
	assert_eq!(
		disp("stored"),
		"feasible",
		"{:?} {:?}",
		refusals("stored"),
		unresolved("stored")
	);
	assert_eq!(row("stored")["invocation"][0]["invocation"], "stored");
	assert_eq!(
		row("stored")["invocation"][1]["sinks"][0],
		"schema resolution",
		"each closure has its own audited sinks"
	);
	assert_eq!(
		row("stored")["closures"].as_array().unwrap().len(),
		2,
		"two closures, one row"
	);
	assert_eq!(
		disp("unaudited"),
		"unresolved",
		"a 'static bound does not classify: without an audit entry the operation is unresolved"
	);
	assert_eq!(
		row("unaudited")["invocation"][0]["static_bound"],
		true,
		"the bound stays a recorded fact"
	);
	assert!(unresolved("unaudited")[0].contains("invocation not audited"));
	assert_eq!(disp("free"), "refused");
	assert!(
		refusals("free")
			.iter()
			.any(|r| r.contains("is the free generic `E`") || r.contains("is the free generic `K`")),
		"{:?}",
		refusals("free")
	);
	assert!(
		refusals("arrow")
			.iter()
			.any(|r| r.contains("argument `&polars_arrow::array::Array`")),
		"{:?}",
		refusals("arrow")
	);
	assert!(
		refusals("amort").iter().any(|r| r.contains("AmortSeries")),
		"{:?}",
		refusals("amort")
	);
	assert!(
		refusals("udf")
			.iter()
			.any(|r| r.contains("Udf trait object")),
		"{:?}",
		refusals("udf")
	);
	assert!(
		refusals("borrowed")
			.iter()
			.any(|r| r.contains("borrowed from the argument")),
		"{:?}",
		refusals("borrowed")
	);
	assert_eq!(
		disp("slice"),
		"feasible (vector argument)",
		"{:?}",
		refusals("slice")
	);
	assert_eq!(
		disp("buffer"),
		"feasible (per family, result buffer)",
		"{:?}",
		refusals("buffer")
	);
	assert!(
		refusals("readback")
			.iter()
			.any(|r| r.contains("cannot write back")),
		"{:?}",
		refusals("readback")
	);
	assert_eq!(
		disp("native"),
		"feasible (per family)",
		"{:?}",
		refusals("native")
	);
	assert!(
		refusals("otherarg")
			.iter()
			.any(|r| r.starts_with("parameter `state`")),
		"a closure that maps on a callable whose other argument does not is not feasible: {:?}",
		refusals("otherarg")
	);
	let sinks = census["sinks"]["bindings"].as_array().unwrap();
	assert!(
		sinks.iter().any(|s| s["key"] == "sink") && !sinks.iter().any(|s| s["key"] == "plan"),
		"collect is a classified sink, filter (returns the plan) is not a candidate"
	);
	assert!(
		census["sinks"]["unclassified"]
			.as_array()
			.unwrap()
			.is_empty()
	);
	let empty = || Emitted {
		from_names: BTreeMap::new(),
		functions: String::new(),
		registrations: vec![],
		catalogue: vec![],
		entries: vec![],
		taken: BTreeMap::new(),
		fn_index: 0,
		frozen_ids: Default::default(),
	};
	let mut emitted = empty();
	emit_callable(
		&world,
		&mut emitted,
		inv.callables.iter().find(|c| c.key == "ok").unwrap(),
		&["callback"],
	);
	assert_eq!(emitted.entries[0].status, "generated");
	assert!(
		emitted.functions.contains("engine::run"),
		"an immediate callback is routed in emitted code"
	);
	let mut no_invocation = release.clone();
	no_invocation
		.families
		.callback_invocation
		.retain(|a| a.path != format!("{column}::apply_unary_elementwise"));
	let world_no_invocation = World::new(&inv, &no_invocation, &["callback"]);
	let mut refused = empty();
	emit_callable(
		&world_no_invocation,
		&mut refused,
		inv.callables.iter().find(|c| c.key == "ok").unwrap(),
		&["callback"],
	);
	assert_eq!(
		refused.entries[0].status, "unsupported",
		"missing invocation audit must stop emission"
	);
	// removing the classification of one execution path makes every stored operation unresolved, immediate ones stay feasible
	let mut fewer = release.clone();
	fewer
		.families
		.callback_sink
		.retain(|k| !k.path.ends_with("::collect_schema"));
	let world2 = World::new(&inv, &fewer, &["mechanical", "conversion", "callback"]);
	let mut guarded = empty();
	emit_callable(
		&world2,
		&mut guarded,
		inv.callables.iter().find(|c| c.key == "stored").unwrap(),
		&["callback"],
	);
	assert_eq!(
		guarded.entries[0].status, "unsupported",
		"missing sink classification must stop a stored binding"
	);
	let mut immediate = empty();
	emit_callable(
		&world2,
		&mut immediate,
		inv.callables.iter().find(|c| c.key == "ok").unwrap(),
		&["callback"],
	);
	assert_eq!(
		immediate.entries[0].status, "generated",
		"independently audited immediate callback remains bound"
	);
	let census2 = callback_census(&world2, &inv, &[]);
	let row2 = |key: &str| {
		census2["rows"]
			.as_array()
			.unwrap()
			.iter()
			.find(|r| r["key"] == key)
			.unwrap()
			.clone()
	};
	assert_eq!(
		census2["sinks"]["unclassified"][0],
		"polars_lazy::frame::LazyFrame::collect_schema"
	);
	assert_eq!(
		row2("stored")["disposition"],
		"unresolved",
		"an unclassified execution path leaves stored callbacks unresolved"
	);
	assert!(
		row2("stored")["unresolved"][0]
			.as_str()
			.unwrap()
			.contains("unclassified execution path")
	);
	assert_eq!(
		row2("ok")["disposition"],
		"feasible",
		"an immediate callback does not depend on the sinks"
	);
	// the routing rule: a closure-taking callable on a non-data owner is routed with the reason `callback`
	assert!(
		!routed(
			"wrap_msg",
			Some("polars_error::PolarsError"),
			&[],
			Some("Self")
		),
		"today's rules leave wrap_msg unrouted"
	);
	assert!(
		routed_for_callbacks(&mk(
			"w",
			"polars_error::PolarsError",
			"wrap_msg",
			"&self",
			vec![("func", "F")],
			vec![(
				"F",
				"core::ops::function::FnOnce(&str) -> alloc::string::String"
			)],
			Some("Self"),
			false
		)),
		"the rule routes it"
	);
	println!("callback self-test: ok");
}

/// Record 0079: the routing rule for callbacks. Every binding that accepts
/// a closure is routed through `engine::run`, whatever its owner, name or
/// types would decide, because the callback may be invoked immediately by
/// that call and its failures are translated only at that boundary.
pub(crate) fn routed_for_callbacks(c: &Callable) -> bool {
	c.params.iter().any(|p| closure_signature(c, p).is_some())
		|| routed(
			&c.name,
			Some(&c.owner),
			&c.params,
			c.ret_canonical.as_deref(),
		)
}

pub(crate) fn routed_binding(world: &World, c: &Callable, owner: Option<&str>) -> bool {
	if world
		.release
		.families
		.callback_safe
		.iter()
		.any(|safe| safe.path == c.canonical_path)
	{
		return false;
	}
	if world
		.release
		.families
		.callback_sink
		.iter()
		.any(|sink| sink.path == c.canonical_path && sink.sink != "none")
	{
		return true;
	}
	c.params.iter().any(|p| closure_signature(c, p).is_some())
		|| routed(&c.name, owner, &c.params, c.ret_canonical.as_deref())
}

pub(crate) fn binding_route_reason(world: &World, c: &Callable, routed: bool) -> Option<String> {
	if world
		.release
		.families
		.callback_safe
		.iter()
		.any(|safe| safe.path == c.canonical_path)
	{
		return Some("callback-safe (audited)".into());
	}
	if c.params.iter().any(|p| closure_signature(c, p).is_some()) {
		return Some("callback".into());
	}
	if world
		.release
		.families
		.callback_sink
		.iter()
		.any(|sink| sink.path == c.canonical_path && sink.sink != "none")
	{
		return Some("executes callbacks".into());
	}
	if routed {
		Some("engine thread".into())
	} else {
		None
	}
}

/// The source audit is an admission rule. A missing invocation or execution
/// path cannot acquire a binding merely because its Rust types map.
pub(crate) fn callback_gate(world: &World, c: &Callable) -> Result<(), String> {
	if c.params.iter().any(|p| closure_signature(c, p).is_some()) {
		if let Some(disposition) = world.callback_dispositions.get(&c.key) {
			if !disposition.starts_with("feasible") {
				return Err(disposition.clone());
			}
		}
	}
	for p in &c.params {
		let Some(sig) = closure_signature(c, p) else {
			continue;
		};
		let Some(audit) = world
			.release
			.families
			.callback_invocation
			.iter()
			.find(|a| a.path == c.canonical_path && a.param == p.name)
		else {
			return Err(format!("{}: invocation not audited", p.name));
		};
		if audit.invocation == "stored" {
			if !world.callback_unclassified_sinks.is_empty() {
				return Err(format!(
					"{}: unclassified execution path(s): {}",
					p.name,
					world.callback_unclassified_sinks.join(", ")
				));
			}
			for group in &audit.sinks {
				if !world.callback_sink_groups.contains(group) {
					return Err(format!(
						"{}: sink group `{group}` has no classified member",
						p.name
					));
				}
			}
		}
		for arg in &sig.args {
			if arg.trim().starts_with("&mut ")
				&& !world
					.release
					.families
					.callback_mutable
					.iter()
					.any(|m| m.path == c.canonical_path && m.param == p.name)
			{
				return Err(format!(
					"{}: mutable argument `{arg}` has no audited contract",
					p.name
				));
			}
		}
	}
	Ok(())
}

/// Map a Polars callback argument into an owned Rune value. Slices are
/// copied as vectors of wrapped values; mutable slices are deliberately
/// one-way, under the release file's audited vector contract.
/// Record 0093: a fallible element conversion is allowed into a callback only
/// when it is one of the checked read-backs the typed unwind can report.
pub(crate) fn checked_callback_conv(conv: &str) -> bool {
	conv.contains("support::widen::<") || conv.contains("support::copy_slice(")
}
pub(crate) fn callback_input(
	world: &World,
	t: &Ty,
	var: &str,
	owner: Option<&str>,
) -> Result<String, Unsupported> {
	// a fallible collection element fails the callback (typed unwind) before the script runs
	let collect = |mapped: &Ret, what: &'static str| -> Result<String, Unsupported> {
		if mapped.materialize.is_some() {
			return Err(Unsupported(what, t.render()));
		}
		if !mapped.fallible {
			return Ok(format!(
				"{var}.iter().map(|__r| {{ let __r = __r.clone(); {} }}).collect::<Vec<_>>()",
				mapped.conv
			));
		}
		if !checked_callback_conv(&mapped.conv) {
			return Err(Unsupported(what, t.render()));
		}
		Ok(format!(
			"match {var}.iter().map(|__r| {{ let __r = __r.clone(); Ok::<_, Error>({}) }}).collect::<Result<Vec<_>, Error>>() {{ Ok(__v) => __v, Err(__e) => support::callback::unwind(crate::engine::CallbackFailure {{ op: \"__OP__\".into(), cause: __e.1 }}) }}",
			mapped.conv
		))
	};
	match t {
		Ty::Ref { inner, .. } if matches!(&**inner, Ty::Slice(_)) => {
			let Ty::Slice(elem) = &**inner else {
				unreachable!()
			};
			let mapped = world.ret(elem, owner, 0)?;
			collect(&mapped, "callback slice element")
		}
		Ty::Path { path, args } if path == "alloc::vec::Vec" && args.len() == 1 => {
			let mapped = world.ret(&args[0], owner, 0)?;
			collect(&mapped, "callback vector element")
		}
		_ => {
			let mapped = world.ret(t, owner, 0)?;
			if mapped.materialize.is_some() {
				return Err(Unsupported("callback argument", t.render()));
			}
			if mapped.fallible {
				// record 0082: only a bounded slice copy is fallible here; its
				// refusal is the callback's typed failure, naming the operation
				if !checked_callback_conv(&mapped.conv) {
					return Err(Unsupported("callback argument", t.render()));
				}
				return Ok(format!(
					"{{ let __r = {var}; match (|| Ok::<_, Error>({}))() {{ Ok(__v) => __v, Err(__e) => support::callback::unwind(crate::engine::CallbackFailure {{ op: \"__OP__\".into(), cause: __e.1 }}) }} }}",
					mapped.conv
				));
			}
			Ok(format!("{{ let __r = {var}; {} }}", mapped.conv))
		}
	}
}

pub(crate) fn callback_rust_type(world: &World, raw: &str, owner: Option<&str>) -> String {
	let mut result = raw
		.replace("Self", owner.unwrap_or("Self"))
		.replace("alloc::string::String", "String")
		.replace("alloc::vec::Vec", "Vec");
	let mut paths: Vec<_> = world.wrappers.iter().collect();
	paths.sort_by_key(|(path, _)| std::cmp::Reverse(path.len()));
	for (path, wrapper) in paths {
		result = result.replace(path, &wrapper.spell);
	}
	result
}

pub(crate) fn callback_arg(
	world: &World,
	c: &Callable,
	sig: &ClosureSig,
	name: &str,
	owner: Option<&str>,
) -> Result<Arg, Unsupported> {
	if sig.udf {
		return Err(Unsupported("callback Udf", sig.param.clone()));
	}
	let operation = if let Some(o) = owner {
		format!("{}::{}", last(o), c.name)
	} else {
		c.name.clone()
	};
	let audit = world
		.release
		.families
		.callback_mutable
		.iter()
		.find(|m| m.path == c.canonical_path && m.param == sig.param);
	let copy_bound = c
		.params
		.iter()
		.find(|p| sanitize(&p.name) == name)
		.is_some_and(|p| p.ty_canonical.contains("Copy"))
		|| c.generics_canonical.iter().any(|(key, bound)| {
			key == &c
				.params
				.iter()
				.find(|p| sanitize(&p.name) == name)
				.map(|p| p.ty_canonical.clone())
				.unwrap_or_default()
				&& bound.contains("Copy")
		});
	let mut params = Vec::new();
	let mut values = Vec::new();
	let mut buffer: Option<String> = None;
	for (i, raw) in sig.args.iter().enumerate() {
		let t = ty::parse(raw);
		let var = format!("__cb_a{i}");
		let rust_type = callback_rust_type(world, raw, owner);
		params.push(format!("{var}: {rust_type}"));
		if let Ty::Ref {
			mutable: true,
			inner,
		} = &t
		{
			if matches!(&**inner, Ty::Path { path, .. } if path == "alloc::string::String")
				&& audit.is_some_and(|a| a.contract == "result buffer")
			{
				buffer = Some(var);
				continue;
			}
			if !matches!(&**inner, Ty::Slice(_)) || !audit.is_some_and(|a| a.contract == "vector") {
				return Err(Unsupported("callback mutable contract", raw.clone()));
			}
		}
		values.push(callback_input(world, &t, &var, owner)?.replace("__OP__", &operation));
	}
	let args = if values.is_empty() {
		"()".to_string()
	} else {
		format!("({},)", values.join(", "))
	};
	let result = ty::parse(&sig.ret);
	let (inner, polars_result) = match &result {
		Ty::Path { path, args } if path == "polars_error::PolarsResult" && args.len() == 1 => {
			(&args[0], true)
		}
		_ => (&result, false),
	};
	let result_ty = if buffer.is_some() {
		"String".to_string()
	} else {
		let mapping = world.arg(inner, "__cb_result", &BTreeMap::new(), owner, 0)?;
		let rune_type = mapping
			.rust_ty
			.strip_prefix("&mut ")
			.or_else(|| mapping.rust_ty.strip_prefix('&'))
			.unwrap_or(&mapping.rust_ty)
			.to_string();
		rune_type
	};
	let bridge_ref = if copy_bound {
		format!("__cb_{name}_ref")
	} else {
		format!("&__cb_{name}")
	};
	let bridge = format!(
		"support::callback::bridge::<_, {result_ty}>(\"{operation}\", {bridge_ref}, {args})"
	);
	let converted = if let Some(buffer) = buffer {
		format!("{bridge}.map(|text| {{ *{buffer} = text; }})")
	} else if matches!(inner, Ty::Tuple(ts) if ts.is_empty()) {
		bridge
	} else {
		let mapping = world.arg(inner, "__cb_result", &BTreeMap::new(), owner, 0)?;
		let conv = mapping.conv;
		format!(
			"{bridge}.and_then(|__cb_result| support::callback::convert(\"{operation}\", || Ok::<_, Error>({conv})))"
		)
	};
	let delivered = if polars_result {
		format!("{converted}.map_err(support::callback::compute_error)")
	} else {
		format!("{converted}.unwrap_or_else(support::callback::unwind)")
	};
	let closure = format!("move |{}| {{ {delivered} }}", params.join(", "));
	let borrow = c
		.params
		.iter()
		.find(|p| sanitize(&p.name) == name)
		.map(|p| p.ty_canonical.as_str())
		.unwrap_or("");
	let (conv, declaration) = if borrow.starts_with("&mut ") {
		(
			format!("&mut __cb_callable_{name}"),
			Some(format!("let mut __cb_callable_{name} = {closure};")),
		)
	} else if borrow.starts_with('&') {
		(
			format!("&__cb_callable_{name}"),
			Some(format!("let __cb_callable_{name} = {closure};")),
		)
	} else {
		(closure, None)
	};
	let mut arg = ok_arg("rune::runtime::Function", conv, "callback")?;
	arg.fallible = true;
	arg.pre.push(format!(
		"let __cb_{name} = support::callback::install(\"{operation}\", {name})?;"
	));
	if copy_bound {
		arg.pre
			.push(format!("let __cb_{name}_ref = __cb_{name}.as_ref();"));
	}
	if let Some(declaration) = declaration {
		arg.pre.push(declaration);
	}
	arg.shape = format!("callback:{}({})->{}", sig.kind, sig.args.join(","), sig.ret);
	Ok(arg)
}

#[cfg(test)]
mod tests {
	#[test]
	fn callback() {
		super::callback_self_test();
	}
}
