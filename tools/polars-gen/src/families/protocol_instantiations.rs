//! Record 0123: a value-protocol impl on a generic head, bound onto one
//! listed wrapped instantiation. 0113 refuses such impls because "the
//! wrapped instantiations carry their own protocols"; a closed, cited row
//! names the impl and the wrapper that carries it, and the impl is emitted
//! through the protocol family unchanged (its shape check and code). The
//! compiler proves the bound for the instantiation (the binding does not
//! compile otherwise). A row may also ask for `DISPLAY_FMT` with the same
//! text, for a type Rust gives no `Display` (Schema: `derive(Debug)` only).
use crate::emit::Emitted;
use crate::families::{Claims, Family};
use crate::model::Callable;
use crate::world::World;
use std::fmt::Write as _;

#[derive(serde::Deserialize, Clone, Debug)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProtocolInstantiation {
	/// The impl's canonical path (`… as core::fmt::Debug`).
	pub(crate) impl_path: String,
	/// The wrapper that carries it (its canonical key).
	pub(crate) wrapper: String,
	/// Also install `DISPLAY_FMT` with the `Debug` text.
	#[serde(default)]
	pub(crate) display_from_debug: bool,
	pub(crate) cite: String,
}

/// The traits a row may name: the formatting protocol only.
const ALLOWED: &[&str] = &["Debug"];

fn row<'a>(world: &'a World, c: &Callable) -> Option<&'a ProtocolInstantiation> {
	world
		.release
		.families
		.protocol_instantiations
		.iter()
		.find(|r| r.impl_path == c.canonical_path)
}

/// Review of 0123: the carrier must instantiate the impl's owner. Its
/// wrapper identity (through at most one `Arc`, as `SchemaRef`'s alias
/// target `Arc<Schema<DataType, ()>>`) must be the owner's path with exactly
/// the impl head's number of arguments, each concrete (no impl parameter),
/// or the owner itself when the impl head has none.
pub(crate) fn carrier_instantiates(
	identity: &str,
	owner: &str,
	impl_for: &str,
) -> Result<(), String> {
	use crate::ty::{Ty, parse};
	let params = match parse(impl_for) {
		Ty::Path { path, args } if path == owner => args,
		_ => return Err(format!("the impl head `{impl_for}` is not on `{owner}`")),
	};
	let inner = match parse(identity) {
		Ty::Path { path, args } if path == "alloc::sync::Arc" && args.len() == 1 => args[0].clone(),
		t => t,
	};
	let Ty::Path { path, args } = &inner else {
		return Err(format!("the carrier `{identity}` is not a path type"));
	};
	if path != owner {
		return Err(format!(
			"the carrier `{identity}` does not instantiate `{owner}`"
		));
	}
	if args.len() != params.len() {
		return Err(format!(
			"the carrier `{identity}` has {} arguments where `{impl_for}` has {}",
			args.len(),
			params.len()
		));
	}
	let param_names: Vec<String> = params.iter().map(|p| p.render()).collect();
	// every nested type, not only the top-level arguments (review of 0123,
	// round 2: `Schema<Vec<Field>, ()>` left `Field` open inside `Vec`)
	if let Some(open) = args.iter().find_map(|a| open_parameter(a, &param_names)) {
		return Err(format!("the carrier `{identity}` leaves `{open}` open"));
	}
	Ok(())
}

/// The first impl parameter named anywhere inside `t`, if any.
fn open_parameter(t: &crate::ty::Ty, params: &[String]) -> Option<String> {
	use crate::ty::Ty;
	match t {
		Ty::Path { path, args } => {
			if args.is_empty() && params.contains(path) {
				return Some(path.clone());
			}
			args.iter().find_map(|a| open_parameter(a, params))
		}
		Ty::Generic(g) if params.contains(g) => Some(g.clone()),
		Ty::Generic(_) | Ty::Other(_) => None,
		Ty::Ref { inner, .. } | Ty::Slice(inner) => open_parameter(inner, params),
		Ty::Tuple(items) => items.iter().find_map(|a| open_parameter(a, params)),
		Ty::Impl(bounds) => bounds
			.iter()
			.flat_map(|b| b.args.iter().chain(b.item.as_deref()))
			.find_map(|a| open_parameter(a, params)),
	}
}

/// The rows, fail closed: cited, unique, an allowed trait, a wrapped
/// carrier that instantiates the impl's owner, and an impl present in the
/// inventory.
pub(crate) fn validate(
	world: &World,
	rows: &[ProtocolInstantiation],
	callables: &[Callable],
) -> Result<(), String> {
	let mut seen = std::collections::BTreeSet::new();
	for r in rows {
		if r.cite.trim().is_empty() {
			return Err(format!("{}: no citation", r.impl_path));
		}
		if !seen.insert(r.impl_path.clone()) {
			return Err(format!("{}: listed twice", r.impl_path));
		}
		let tr = r.impl_path.rsplit("::").next().unwrap_or("");
		if !ALLOWED.contains(&tr) {
			return Err(format!(
				"{}: `{tr}` is not an allowed protocol",
				r.impl_path
			));
		}
	}
	// then each row against the world: a wrapped carrier, a recorded impl,
	// and a carrier that instantiates the impl's owner
	for r in rows {
		if !world.wrappers.contains_key(&r.wrapper) {
			return Err(format!(
				"{}: carrier {} is not wrapped",
				r.impl_path, r.wrapper
			));
		}
		let Some(c) = callables.iter().find(|c| c.canonical_path == r.impl_path) else {
			return Err(format!("{}: not in the inventory", r.impl_path));
		};
		let identity = &world.wrappers[&r.wrapper].identity;
		carrier_instantiates(
			identity,
			&c.owner,
			c.impl_for.as_deref().unwrap_or(&c.owner),
		)
		.map_err(|why| format!("{}: {why}", r.impl_path))?;
	}
	Ok(())
}

pub(crate) struct ProtocolInstantiationRoute;
pub(crate) static PROTOCOL_INSTANTIATION: ProtocolInstantiationRoute = ProtocolInstantiationRoute;

impl Family for ProtocolInstantiationRoute {
	fn name(&self) -> &'static str {
		"protocol_instantiation"
	}
	fn claim<'a>(&self, cx: &mut Claims<'a, '_>, c: &'a Callable) -> bool {
		let Some(r) = row(cx.world, c) else {
			return false;
		};
		let mut on = c.clone();
		on.owner = r.wrapper.clone();
		crate::families::protocols::emit_foreign(cx.world, cx.out, &on);
		if r.display_from_debug {
			display(cx.world, cx.out, r);
		}
		true
	}
}

/// `DISPLAY_FMT` with the `Debug` text, for the listed carrier.
fn display(world: &World, out: &mut Emitted, r: &ProtocolInstantiation) {
	let w = &world.wrappers[&r.wrapper];
	let ident = format!(
		"pi_display_{}",
		crate::text::sanitize(&r.wrapper).to_lowercase()
	);
	writeln!(
		out.functions,
		"/// Record 0123: `{}` has no Rust `Display`; its text is Polars' own `Debug` ({}).\n#[rune::function(instance, protocol = DISPLAY_FMT)]\nfn {ident}(this: &{}, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {{ use rune::alloc::fmt::TryWrite; let s = format!(\"{{:?}}\", this.0); rune::vm_write!(f, \"{{s}}\") }}",
		r.wrapper, r.cite, w.rust
	)
	.unwrap();
	out.registrations
		.push(format!("m.function_meta({ident})?;"));
	// a protocol has no help path of its own (as the protocol family's
	// bindings): no catalogue entry
}

/// Record 0123: the rows refuse what they must.
pub(crate) fn protocol_instantiations_self_test() {
	use crate::model::{Inventory, Supporting};
	use crate::release::{FamilyTables, InstantiationScope, Release, ReleaseProvenance};
	let sup = |path: &str| Supporting {
		trait_params: vec![],
		key: path.to_string(),
		kind: "struct".to_string(),
		canonical_path: path.to_string(),
		found_paths: vec![format!(
			"polars::prelude::{}",
			path.rsplit("::").next().unwrap()
		)],
		crate_paths: vec![path.to_string()],
		public_fields: 0,
		fields_canonical: vec![],
		variant_shapes: vec![],
		variant_payloads: vec![],
		generic: false,
		lifetime: false,
		hidden: false,
		derived: vec![],
		alias_target: None,
		implementors: vec![],
		impls: vec![],
	};
	let carrier = "polars_core::frame::column::Column";
	let debug = "polars_schema::schema::Schema as core::fmt::Debug";
	let callable: Callable = serde_json::from_value(serde_json::json!({
		"key": "d", "kind": "foreign_trait_impl", "krate": "polars_schema", "owner": "polars_schema::schema::Schema", "name": "Debug",
		"canonical_path": debug, "found_paths": [], "crate_paths": [], "receiver": "&self",
		"params": [], "ret": null, "ret_canonical": null, "generics_canonical": [], "impl_for": null,
		"impl_bounds": [], "impl_head": null, "impl_where": [], "impl_assoc": [], "docs_first": null,
		"owner_generic": true, "is_unsafe": false, "is_async": false, "deprecated": false,
		"hidden": false, "implementors": [], "trait_reachable": false, "derived": false,
		"bucket": "generic", "rules": []
	}))
	.unwrap();
	let release = Release {
		name: "t".into(),
		source: "t".into(),
		provenance: ReleaseProvenance::default(),
		instantiation: InstantiationScope::default(),
		api_crates: vec!["polars_core".into()],
		unordered: vec![],
		excluded_oracle: vec![],
		refused: vec![],
		families: FamilyTables::default(),
	};
	let w = World::new(
		&Inventory {
			callables: vec![],
			provenance: None,
			supporting: vec![sup(carrier)],
		},
		&release,
		&["mechanical"],
	);
	let row = |impl_path: &str, wrapper: &str, cite: &str| ProtocolInstantiation {
		impl_path: impl_path.into(),
		wrapper: wrapper.into(),
		display_from_debug: true,
		cite: cite.into(),
	};
	let calls = vec![callable];
	// review of 0123: Column does not instantiate Schema; refused by name
	let e = validate(&w, &[row(debug, carrier, "c")], &calls).unwrap_err();
	assert!(e.contains("does not instantiate"), "{e}");
	// the structural check itself: SchemaRef's alias target passes; a
	// different head, a wrong arity or an open parameter is refused
	let owner = "polars_schema::schema::Schema";
	let head = "polars_schema::schema::Schema<Field, Metadata>";
	let schema_ref = "alloc::sync::Arc<polars_schema::schema::Schema<polars_core::datatypes::dtype::DataType, ()>>";
	assert!(carrier_instantiates(schema_ref, owner, head).is_ok());
	assert!(
		carrier_instantiates(
			"polars_schema::schema::Schema<polars_core::datatypes::dtype::DataType, ()>",
			owner,
			head
		)
		.is_ok()
	);
	assert!(
		carrier_instantiates("polars_core::frame::column::Column", owner, head)
			.unwrap_err()
			.contains("does not instantiate")
	);
	assert!(
		carrier_instantiates(
			"alloc::sync::Arc<polars_schema::schema::Schema<polars_core::datatypes::dtype::DataType>>",
			owner,
			head
		)
		.unwrap_err()
		.contains("arguments")
	);
	assert!(
		carrier_instantiates(
			"alloc::sync::Arc<polars_schema::schema::Schema<Field, ()>>",
			owner,
			head
		)
		.unwrap_err()
		.contains("open")
	);
	// nested anywhere: inside a container, an option, a reference, a tuple, a slice
	for nested in [
		"polars_schema::schema::Schema<alloc::vec::Vec<Field>, ()>",
		"polars_schema::schema::Schema<polars_core::datatypes::dtype::DataType, core::option::Option<Metadata>>",
		"polars_schema::schema::Schema<&Field, ()>",
		"polars_schema::schema::Schema<(polars_core::datatypes::dtype::DataType, Metadata), ()>",
		"polars_schema::schema::Schema<[Field], ()>",
	] {
		let e = carrier_instantiates(nested, owner, head).unwrap_err();
		assert!(e.contains("open"), "{nested}: {e}");
	}
	let refused = |rows: Vec<ProtocolInstantiation>, want: &str| {
		let e = validate(&w, &rows, &calls).unwrap_err();
		assert!(e.contains(want), "{e} (wanted {want})");
	};
	refused(vec![row(debug, carrier, " ")], "no citation");
	refused(
		vec![row(debug, carrier, "c"), row(debug, carrier, "c")],
		"listed twice",
	);
	refused(
		vec![row(
			"polars_schema::schema::Schema as core::cmp::PartialEq",
			carrier,
			"c",
		)],
		"not an allowed protocol",
	);
	refused(
		vec![row(debug, "polars_core::frame::DataFrame", "c")],
		"is not wrapped",
	);
	refused(
		vec![row(
			"polars_core::frame::Other as core::fmt::Debug",
			carrier,
			"c",
		)],
		"not in the inventory",
	);
	println!("protocol instantiations self-test: ok");
}

#[cfg(test)]
mod tests {
	#[test]
	fn protocol_instantiations() {
		super::protocol_instantiations_self_test();
	}
}
