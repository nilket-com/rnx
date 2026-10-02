pub(crate) mod callable;
pub(crate) mod free;
pub(crate) mod instantiations;
pub(crate) mod output;
pub(crate) mod types;
use crate::model::{Callable, Param};
use crate::text::{mentions, sanitize};
use crate::ty::last;
use crate::world::Wrapper;
use std::collections::BTreeMap;

// ---------------------------------------------------------------- emission

#[derive(serde::Serialize, Clone)]
pub(crate) struct Entry {
	pub(crate) key: String,
	pub(crate) canonical_path: String,
	pub(crate) kind: String,
	pub(crate) bucket: String,
	pub(crate) status: &'static str,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) fallible: Option<bool>,
	/// Parameter and return types, canonical, so a release diff sees reshapes.
	pub(crate) signature: String,
	/// For generated entries: `case` (an oracle case exists), or why not.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) execution: Option<String>,
	/// For the oracle generator: how to call the binding and the Polars function.
	#[serde(skip)]
	pub(crate) oracle: Option<OracleInfo>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) reason: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) rune: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) note: Option<String>,
	/// Record 0076: the emitted bindings of this callable, one per
	/// receiver route. The entry's status is callable coverage; each
	/// binding carries its own disposition and, when it has one, its case.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub(crate) bindings: Vec<Binding>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub(crate) exceptions: Vec<RouteException>,
	/// Record 0078: for a duplicate rustdoc listing of a `From` impl, the
	/// key of the retained listing (on the impl's `for` type) that binds it.
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) counterpart: Option<String>,
}

/// One emitted binding of a callable: its identity is the callable plus
/// the canonical receiver and the route that produced it.
#[derive(Clone, serde::Serialize)]
pub(crate) struct Binding {
	/// Unique across the surface: the callable's sanitized path, with
	/// `__on__<receiver>` for every receiver after the first.
	pub(crate) id: String,
	pub(crate) rune: String,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) receiver: Option<String>,
	/// `inherent`, `free`, `protocol`, `implementor` (a trait method on a
	/// wrapped implementor).
	pub(crate) route: &'static str,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) route_reason: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) reentry: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) disposition: Option<String>,
	#[serde(skip_serializing_if = "Option::is_none")]
	pub(crate) case_id: Option<String>,
	/// The Rust callee for the oracle when the route changes it (deref).
	#[serde(skip)]
	pub(crate) callee: Option<String>,
	/// The oracle information of this binding when it differs from the
	/// entry's (an instantiation has its own substituted signature).
	#[serde(skip)]
	pub(crate) info: Option<OracleInfo>,
}

/// A candidate receiver route that produced no binding, with its reason:
/// every candidate route of a callable has exactly one disposition, a
/// binding or one of these.
#[derive(Clone, serde::Serialize)]
pub(crate) struct RouteException {
	pub(crate) route: &'static str,
	pub(crate) receiver: String,
	pub(crate) reason: String,
}

/// Record 0120: a binding the freeze names keeps its frozen id; a new
/// binding that would take a frozen id gets the receiver-qualified form.
pub(crate) fn pin_frozen_ids(
	frozen: &BTreeMap<(String, String), String>,
	key: &str,
	canonical_path: &str,
	receivers: &[String],
	bindings: &mut [Binding],
) {
	let frozen_of = |rune: &str| frozen.get(&(key.to_string(), rune.to_string()));
	let pinned: std::collections::BTreeSet<String> = bindings
		.iter()
		.filter_map(|b| frozen_of(&b.rune).cloned())
		.collect();
	for (i, b) in bindings.iter_mut().enumerate() {
		if let Some(id) = frozen_of(&b.rune) {
			b.id = id.clone();
		} else if pinned.contains(&b.id) {
			b.id = binding_id(canonical_path, Some(&receivers[i]), false);
		}
	}
}

/// Record 0125: the frozen ids kept on every emission path, as a last pass
/// before the freeze. Some ids embed a rustdoc key (operator rows, receivers
/// that name their impl), and rustdoc renumbers keys when a crate gains
/// modules, so an unchanged binding would otherwise get a new id. A binding
/// whose (key, Rune path) is frozen takes its frozen id; a pinned id that
/// another binding already holds is refused, never silently shared. Returns
/// how many ids were restored.
pub(crate) fn pin_frozen_everywhere(
	frozen: &BTreeMap<(String, String), String>,
	entries: &mut [Entry],
) -> Result<usize, String> {
	let mut restored = 0;
	for e in entries.iter_mut().filter(|e| e.status == "generated") {
		for b in &mut e.bindings {
			if let Some(id) = frozen.get(&(e.key.clone(), b.rune.clone())) {
				if &b.id != id {
					b.id = id.clone();
					restored += 1;
				}
			}
		}
	}
	let mut seen = std::collections::BTreeSet::new();
	for e in entries.iter().filter(|e| e.status == "generated") {
		for b in &e.bindings {
			if !seen.insert(b.id.as_str()) {
				return Err(format!(
					"binding id {} is held by more than one binding",
					b.id
				));
			}
		}
	}
	Ok(restored)
}

/// Record 0125: a frozen id is restored on any path, and a pinned id that
/// another binding holds is refused.
pub(crate) fn pin_everywhere_self_test() {
	let frozen: BTreeMap<(String, String), String> = [(
		("k".to_string(), "polars::A | A".to_string()),
		"a_old_630_6881".to_string(),
	)]
	.into();
	let mut e = Entry::for_test("k");
	e.status = "generated";
	e.bindings = vec![Binding::for_test("a_new_717_6968", "polars::A | A")];
	let mut entries = vec![e];
	assert_eq!(pin_frozen_everywhere(&frozen, &mut entries), Ok(1));
	assert_eq!(entries[0].bindings[0].id, "a_old_630_6881");
	let mut other = Entry::for_test("j");
	other.status = "generated";
	other.bindings = vec![Binding::for_test("a_old_630_6881", "polars::B")];
	entries.push(other);
	let e = pin_frozen_everywhere(&frozen, &mut entries).unwrap_err();
	assert!(e.contains("held by more than one binding"), "{e}");
	println!("pin everywhere self-test: ok");
}

pub(crate) fn binding_id(canonical_path: &str, receiver: Option<&str>, first: bool) -> String {
	let base = sanitize(canonical_path).to_lowercase();
	match receiver {
		Some(r) if !first => format!("{base}__on__{}", sanitize(last(r)).to_lowercase()),
		_ => base,
	}
}

impl Entry {
	/// A bare entry for self-tests.
	pub(crate) fn for_test(key: &str) -> Entry {
		Entry {
			key: key.into(),
			canonical_path: key.into(),
			kind: "inherent".into(),
			bucket: "mechanical".into(),
			status: "unsupported",
			fallible: None,
			signature: String::new(),
			execution: None,
			oracle: None,
			reason: None,
			rune: None,
			note: None,
			bindings: vec![],
			exceptions: vec![],
			counterpart: None,
		}
	}
}

impl Binding {
	/// A bare binding for self-tests.
	pub(crate) fn for_test(id: &str, rune: &str) -> Binding {
		Binding {
			id: id.into(),
			rune: rune.into(),
			receiver: None,
			route: "implementor",
			route_reason: None,
			reentry: None,
			disposition: None,
			case_id: None,
			callee: None,
			info: None,
		}
	}
}

/// Everything the oracle test generator needs about one generated binding.
#[derive(Clone)]
pub(crate) struct OracleInfo {
	/// Rune-side call target: `polars::DataFrame::height` style path and the method name.
	pub(crate) rune_owner: Option<String>,
	pub(crate) rune_name: String,
	pub(crate) receiver: String,
	/// Owner canonical path (methods) and wrapper rust ident.
	pub(crate) owner: Option<(String, String)>,
	/// Rust callee expression prefix, e.g. `<polars::frame::DataFrame>::height` or `polars_ops::prelude::f`.
	pub(crate) callee: String,
	/// (rune shape, canonical type) per parameter.
	pub(crate) params: Vec<(String, String)>,
	/// Parameter names, for the order policy's recorded configuration.
	pub(crate) param_names: Vec<String>,
	pub(crate) ret_canonical: Option<String>,
	/// Wrapper return type as emitted.
	pub(crate) ret_rust: String,
	pub(crate) fallible: bool,
	pub(crate) generics: BTreeMap<String, String>,
	/// For trait methods: every generated implementor (canonical, wrapper ident).
	pub(crate) implementors: Vec<(String, String)>,
	/// The binding reaches the trait through `Deref`: the oracle's receiver is `&*recv`.
	pub(crate) deref: bool,
}

pub(crate) struct Emitted {
	/// Record 0078: the planned `from_<source>` name per `From` impl key.
	pub(crate) from_names: BTreeMap<String, String>,
	pub(crate) functions: String,
	pub(crate) registrations: Vec<String>,
	pub(crate) catalogue: Vec<(String, String)>,
	/// Record 0141: the Rune paths whose binding takes its receiver by
	/// value (a non-Clone owner moved out of the Rune value).
	pub(crate) consuming: Vec<String>,
	pub(crate) entries: Vec<Entry>,
	/// per (wrapper rust path, rune method name) -> canonical path that took it
	pub(crate) taken: BTreeMap<(String, String), String>,
	pub(crate) fn_index: usize,
	/// Record 0120: the frozen binding ids by (entry key, Rune path); a
	/// binding the freeze names keeps its id whatever receivers join it.
	pub(crate) frozen_ids: BTreeMap<(String, String), String>,
}

/// A stable Rust identifier for a binding: a short hash of the canonical
/// path plus its tail, so regenerating after an upstream change only
/// touches the lines that changed.
pub(crate) fn rust_ident(prefix: &str, canonical: &str, _idx: usize) -> String {
	let mut h: u64 = 0xcbf29ce484222325;
	for b in canonical.bytes() {
		h ^= b as u64;
		h = h.wrapping_mul(0x100000001b3);
	}
	let mut s = sanitize(canonical);
	if s.len() > 48 {
		s = s[s.len() - 48..].to_string();
	}
	format!("{prefix}_{:08x}_{s}", (h >> 32) as u32 ^ h as u32).to_lowercase()
}

pub(crate) fn doc_line(c: &Callable) -> String {
	let mut d = c.docs_first.clone().unwrap_or_default().replace('\n', " ");
	if d.len() > 160 {
		d.truncate(157);
		d.push_str("...");
	}
	d
}

impl Emitted {
	pub(crate) fn unsupported(&mut self, c: &Callable, reason: &str, detail: &str) {
		self.entries.push(Entry {
			key: c.key.clone(),
			canonical_path: c.canonical_path.clone(),
			kind: c.kind.clone(),
			bucket: c.bucket.clone(),
			status: "unsupported",
			fallible: None,
			signature: signature_of(c),
			execution: None,
			oracle: None,
			reason: Some(format!("{reason}: {detail}")),
			rune: None,
			note: None,
			bindings: vec![],
			exceptions: vec![],
			counterpart: None,
		});
	}
	pub(crate) fn adapted(&mut self, c: &Callable, reason: &str, rune: &str) {
		self.entries.push(Entry {
			key: c.key.clone(),
			canonical_path: c.canonical_path.clone(),
			kind: c.kind.clone(),
			bucket: c.bucket.clone(),
			status: "adapted",
			fallible: None,
			signature: signature_of(c),
			execution: None,
			oracle: None,
			reason: Some(reason.into()),
			rune: Some(rune.into()),
			note: None,
			bindings: vec![],
			exceptions: vec![],
			counterpart: None,
		});
	}
	pub(crate) fn generated(&mut self, c: &Callable, rune: &str, note: Option<String>) {
		self.entries.push(Entry {
			key: c.key.clone(),
			canonical_path: c.canonical_path.clone(),
			kind: c.kind.clone(),
			bucket: c.bucket.clone(),
			status: "generated",
			fallible: None,
			signature: signature_of(c),
			execution: None,
			oracle: None,
			reason: None,
			rune: Some(rune.into()),
			note,
			bindings: vec![Binding {
				id: binding_id(&c.canonical_path, None, true),
				rune: rune.into(),
				receiver: None,
				route: "inherent",
				route_reason: None,
				reentry: None,
				disposition: None,
				case_id: None,
				callee: None,
				info: None,
			}],
			exceptions: vec![],
			counterpart: None,
		});
	}
	pub(crate) fn generated_with(
		&mut self,
		c: &Callable,
		rune: &str,
		note: Option<String>,
		info: OracleInfo,
	) {
		let fallible = info.fallible;
		let route = match c.kind.as_str() {
			"inherent" => "inherent",
			"free_fn" => "free",
			"foreign_trait_impl" => "protocol",
			_ => "implementor",
		};
		let receiver = info.owner.as_ref().map(|(o, _)| o.clone());
		let bindings = vec![Binding {
			id: binding_id(&c.canonical_path, receiver.as_deref(), true),
			rune: rune.into(),
			receiver,
			route,
			route_reason: None,
			reentry: None,
			disposition: None,
			case_id: None,
			callee: None,
			info: None,
		}];
		self.entries.push(Entry {
			key: c.key.clone(),
			canonical_path: c.canonical_path.clone(),
			kind: c.kind.clone(),
			bucket: c.bucket.clone(),
			status: "generated",
			fallible: Some(fallible),
			signature: signature_of(c),
			execution: None,
			oracle: Some(info),
			reason: None,
			rune: Some(rune.into()),
			note,
			bindings,
			exceptions: vec![],
			counterpart: None,
		});
	}
	/// A trait method bound on several implementors: one binding per
	/// implementor, the entry's status counting the callable once.
	pub(crate) fn generated_on(
		&mut self,
		c: &Callable,
		per: &[(String, String, &'static str, Option<String>)],
		info: OracleInfo,
	) {
		let fallible = info.fallible;
		let mut bindings: Vec<Binding> = per
			.iter()
			.enumerate()
			.map(|(i, (rune, owner, route, callee))| Binding {
				id: binding_id(&c.canonical_path, Some(owner), i == 0),
				rune: rune.clone(),
				receiver: Some(owner.clone()),
				route,
				route_reason: None,
				reentry: None,
				disposition: None,
				case_id: None,
				callee: callee.clone(),
				info: None,
			})
			.collect();
		// record 0120: frozen ids are kept whatever receivers join the entry
		let receivers: Vec<String> = per.iter().map(|p| p.1.clone()).collect();
		pin_frozen_ids(
			&self.frozen_ids,
			&c.key,
			&c.canonical_path,
			&receivers,
			&mut bindings,
		);
		// record 0118: receivers whose last path segment is a shared generic
		// argument (`CsvReader<…Cursor<…Vec<u8>>>`, `IpcReader<…>`) would share
		// an id; only those are extended with the receiver's own type name
		let mut seen: BTreeMap<String, usize> = BTreeMap::new();
		for b in &bindings {
			*seen.entry(b.id.clone()).or_default() += 1;
		}
		for (i, b) in bindings.iter_mut().enumerate() {
			if seen[&b.id] > 1 && i > 0 {
				let r = &per[i].1;
				let head = r.split('<').next().unwrap_or(r);
				b.id = format!(
					"{}__on__{}_{}",
					sanitize(&c.canonical_path).to_lowercase(),
					sanitize(last(head)).to_lowercase(),
					sanitize(last(r)).to_lowercase()
				);
			}
		}
		let rune = per
			.iter()
			.map(|(r, _, _, _)| r.as_str())
			.collect::<Vec<_>>()
			.join(" ");
		self.entries.push(Entry {
			key: c.key.clone(),
			canonical_path: c.canonical_path.clone(),
			kind: c.kind.clone(),
			bucket: c.bucket.clone(),
			status: "generated",
			fallible: Some(fallible),
			signature: signature_of(c),
			execution: None,
			oracle: Some(info),
			reason: None,
			rune: Some(rune),
			note: None,
			bindings,
			exceptions: vec![],
			counterpart: None,
		});
	}
}

pub(crate) fn signature_of(c: &Callable) -> String {
	format!(
		"({}) -> {}",
		c.params
			.iter()
			.map(|p| p.ty_canonical.clone())
			.collect::<Vec<_>>()
			.join(", "),
		c.ret_canonical.clone().unwrap_or_else(|| "()".into())
	)
}

pub(crate) fn generics_map(c: &Callable) -> BTreeMap<String, String> {
	c.generics_canonical.iter().cloned().collect()
}

/// A generic parameter that no parameter type mentions cannot be inferred
/// from the arguments a binding passes.
pub(crate) fn unused_generic(c: &Callable) -> Option<String> {
	for (g, _) in &c.generics_canonical {
		if g.starts_with("impl ") {
			continue; // rustdoc's synthetic parameter for an `impl Trait` argument
		}
		// record 0084: a generic that only appears in another generic's bound
		// (`I: IntoIterator<Item = S>`, `E: AsRef<[IE]>`) is inferred with it;
		// whether that chain has a script mapping is decided by `bounds`
		let used = c.params.iter().any(|p| mentions(&p.ty_canonical, g))
			|| c.generics_canonical
				.iter()
				.any(|(h, b)| h != g && mentions(b, g));
		if !used {
			return Some(g.clone());
		}
	}
	None
}

/// Types whose operations do eager work on data: any binding whose owner,
/// parameter or return is one of these runs on the engine thread, like the
/// hand-written `collect`, so no Polars work runs on the session's runtime
/// thread. Plan construction (Expr, options, dtypes) does not.
pub(crate) const DATA_TYPES: &[&str] = &[
	"polars_core::frame::dataframe::DataFrame",
	"polars_core::series::Series",
	"polars_core::frame::column::Column",
	"polars_core::frame::column::scalar::ScalarColumn",
	"polars_core::frame::group_by::GroupBy",
	"polars_lazy::frame::LazyFrame",
	"polars_lazy::frame::LazyGroupBy",
	"polars_core::series::implementations::null::NullChunked",
];
/// Explicit additions by name prefix: I/O and query entry points that do
/// not mention a data type in their signature.
pub(crate) const ROUTED_PREFIXES: &[&str] = &[
	"collect", "fetch", "sink", "scan", "read", "write", "execute", "concat", "sort", "rechunk",
];

/// Types that are not `Send`, so their bindings cannot cross to the engine
/// thread; they run on the caller and are listed here on purpose.
pub(crate) const NOT_ROUTED_TYPES: &[&str] = &["polars_core::series::amortized_iter::AmortSeries"];

pub(crate) fn routed(name: &str, owner: Option<&str>, params: &[Param], ret: Option<&str>) -> bool {
	let unsendable = |t: &str| NOT_ROUTED_TYPES.iter().any(|d| mentions(t, d));
	if owner.is_some_and(unsendable)
		|| params.iter().any(|p| unsendable(&p.ty_canonical))
		|| ret.is_some_and(unsendable)
	{
		return false;
	}
	if ROUTED_PREFIXES.iter().any(|p| name.starts_with(p)) {
		return true;
	}
	let touches = |t: &str| DATA_TYPES.iter().any(|d| mentions(t, d));
	owner.is_some_and(touches)
		|| params.iter().any(|p| touches(&p.ty_canonical))
		|| ret.is_some_and(touches)
}

pub(crate) fn rune_path(w: &Wrapper) -> String {
	format!("{}::{}", w.rune_item.trim_start_matches("::"), w.rune_name)
}
