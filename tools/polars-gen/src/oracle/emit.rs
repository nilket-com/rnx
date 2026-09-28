use crate::emit::{Entry, rune_path};
use crate::families::protocols::{ASSIGN_OPS, OPS, owner_has_trait};
use crate::model::Inventory;
use crate::oracle::fixtures::{FIXTURES, TYPED_FIXTURES};
use crate::oracle::{Oracle, OracleCase, setup_fn, staged};
use crate::text::sanitize;
use crate::ty;
use crate::ty::Ty;
use crate::world::World;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;

pub(crate) fn emit_oracle(
	world: &World,
	entries: &mut [Entry],
	inv: &Inventory,
) -> (String, String, Vec<(String, String)>, serde_json::Value) {
	for recipe in &world.release.callback_recipe {
		for used in &recipe.uses {
			let safe = entries.iter().any(|entry| {
				entry.status == "generated"
					&& entry.bindings.iter().any(|binding| {
						&binding.rune == used
							&& binding.route_reason.as_deref() != Some("engine thread")
							&& binding.route_reason.as_deref() != Some("executes callbacks")
							&& binding.route_reason.as_deref() != Some("callback")
					})
			});
			assert!(
				safe,
				"callback recipe {} calls routed or missing binding {used}",
				recipe.signature
			);
		}
	}
	let mut debuggable: BTreeSet<String> = inv
		.supporting
		.iter()
		.filter(|s| s.derived.iter().any(|d| d == "Debug"))
		.map(|s| s.canonical_path.clone())
		.collect();
	let mut defaultable: BTreeSet<String> = inv
		.supporting
		.iter()
		.filter(|s| s.derived.iter().any(|d| d == "Default"))
		.map(|s| s.canonical_path.clone())
		.collect();
	for c in &inv.callables {
		if c.kind == "foreign_trait_impl" {
			if c.name.starts_with("Debug") {
				debuggable.insert(c.owner.clone());
			}
			if c.name.starts_with("Default") {
				defaultable.insert(c.owner.clone());
			}
		}
	}
	for (path, w) in &world.wrappers {
		if w.base.is_some() {
			if world.trait_holds(&w.identity, "Debug", 0) {
				debuggable.insert(path.clone());
			}
			if world.trait_holds(&w.identity, "Default", 0) {
				defaultable.insert(path.clone());
			}
		}
	}
	let default_bound: BTreeSet<String> = entries
		.iter()
		.filter(|e| e.status == "generated")
		.filter_map(|e| {
			e.canonical_path
				.strip_suffix(" as core::default::Default")
				.map(|s| s.to_string())
		})
		.collect();
	let mut o = Oracle {
		world,
		debuggable,
		defaultable,
		default_bound,
		shown: std::cell::RefCell::new(BTreeSet::new()),
		recipes: BTreeMap::new(),
		no_recipe: BTreeMap::new(),
		mask_len: std::cell::Cell::new(3),
		hash_ret: std::cell::Cell::new(false),
		chunk_native: std::cell::RefCell::new(None),
		indexed_native: std::cell::RefCell::new(None),
		array_native: std::cell::RefCell::new(None),
		iter_native: std::cell::RefCell::new(None),
		view_native: std::cell::RefCell::new(None),
		owned_native: std::cell::RefCell::new(None),
		layout_native: std::cell::RefCell::new(None),
	};
	o.derive_recipes(entries);
	let mut cases = Vec::new();
	let mut skipped: Vec<(String, String)> = Vec::new();
	// Every binding of every generated entry gets exactly one disposition:
	// a case with its own id, or a reason. A trait method bound on several
	// implementors is visited once per binding.
	let plan: Vec<(usize, usize)> = entries
		.iter()
		.enumerate()
		.filter(|(_, e)| e.status == "generated")
		.flat_map(|(ei, e)| (0..e.bindings.len().max(1)).map(move |bi| (ei, bi)))
		.collect();
	for (ei, bi) in plan {
		let e = &mut entries[ei];
		let single = e.bindings.len() <= 1;
		let mut skip = |e: &mut Entry, why: String| {
			if let Some(b) = e.bindings.get_mut(bi) {
				b.disposition = Some(why.clone());
			}
			if single {
				e.execution = Some(why.clone());
			}
			skipped.push((e.canonical_path.clone(), why));
		};
		let Some(mut info) = e.oracle.clone() else {
			skip(e, "no call information".into());
			continue;
		};
		let id = e
			.bindings
			.get(bi)
			.map(|b| b.id.clone())
			.unwrap_or_else(|| sanitize(&e.canonical_path).to_lowercase());
		// the route decides the callee and the receiver spelling, for one
		// binding or many; an instantiation carries its own information
		let mut own_info = false;
		if let Some(b) = e.bindings.get(bi) {
			if let Some(i) = &b.info {
				info = i.clone();
				own_info = true;
			}
			if let Some(cal) = b.callee.clone() {
				info.callee = cal;
			}
			info.deref = b.route == "deref";
		}
		// a binding on a specific implementor: that receiver, not the first one with a fixture
		if !single && !own_info {
			let Some(r) = e.bindings[bi].receiver.clone() else {
				skip(e, "binding without a receiver".into());
				continue;
			};
			if o.rune_wrapped(&r).is_none() || o.rust_wrapped(&r).is_none() {
				skip(
					e,
					format!(
						"no fixture for the receiver type ({})",
						o.no_recipe
							.get(&r)
							.cloned()
							.unwrap_or_else(|| "no fixture".into())
					),
				);
				continue;
			}
			if e.bindings[bi].callee.is_none() {
				if let Some((c0, _)) = info.owner.clone() {
					info.callee = info.callee.replacen(
						&format!("<{}", world.wrappers[&c0].spell),
						&format!("<{}", world.wrappers[&r].spell),
						1,
					);
					// a `Self` in the return names this receiver's wrapper, not the first implementor's
					let (w0, wr) = (
						world.wrappers[&c0].rust.clone(),
						world.wrappers[&r].rust.clone(),
					);
					if w0 != wr {
						info.ret_rust = info.ret_rust.replace(&w0, &wr);
					}
				}
			}
			info.rune_owner = Some(rune_path(&world.wrappers[&r]));
			info.owner = Some((r.clone(), world.wrappers[&r].rust.clone()));
			info.implementors.clear();
		}
		let owner = info.owner.as_ref().map(|(c, _)| c.as_str());
		if let Some(x) = world
			.release
			.excluded_oracle
			.iter()
			.find(|x| x.path == e.canonical_path)
		{
			skip(
				e,
				format!("excluded: nondeterministic oracle ({})", x.reason),
			);
			continue;
		}
		// fallible script value: Ok(v) -> value side (text), Err(e) -> error kind
		let value_side = |inner: &str| {
			format!(
				"match rune::from_value::<Result<rune::Value, rune::Value>>(v) {{ Ok(Ok(v)) => ({inner}).map(|s| crate_oracle::Side::Value(crate_oracle::Repr::Text(s))).unwrap_or_else(crate_oracle::Side::Broken), Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(crate_oracle::Side::Error).unwrap_or_else(crate_oracle::Side::Broken), Err(e) => crate_oracle::Side::Broken(e.to_string()) }}"
			)
		};
		let plain_side = |inner: &str| {
			format!(
				"({inner}).map(|s| crate_oracle::Side::Value(crate_oracle::Repr::Text(s))).unwrap_or_else(crate_oracle::Side::Broken)"
			)
		};
		// a wrapped value at the top level keeps its structure for the policy
		let wrapped_side = |helper: &str, fallible: bool| {
			if fallible {
				format!(
					"match rune::from_value::<Result<rune::Value, rune::Value>>(v) {{ Ok(Ok(v)) => {helper}(&v).map(crate_oracle::Side::Value).unwrap_or_else(crate_oracle::Side::Broken), Ok(Err(e)) => crate_oracle::rune_error_kind(&e).map(crate_oracle::Side::Error).unwrap_or_else(crate_oracle::Side::Broken), Err(e) => crate_oracle::Side::Broken(e.to_string()) }}"
				)
			} else {
				format!(
					"{helper}(&v).map(crate_oracle::Side::Value).unwrap_or_else(crate_oracle::Side::Broken)"
				)
			}
		};
		// protocols
		if info.receiver == "protocol" {
			let tname = info.callee.as_str();
			let Some(recv_rune) = o.rune_wrapped(owner.unwrap()) else {
				skip(e, "no fixture for the receiver type".into());
				continue;
			};
			let Some(recv_rust) = o.rust_wrapped(owner.unwrap()) else {
				skip(e, "no Rust fixture for the receiver type".into());
				continue;
			};
			let one = |e: &str| vec![("__recv".to_string(), e.to_string())];
			let (script, fmt, oracle) = match tname {
				"Display" => (
					format!(
						"{} pub fn main(__fx) {{ let a = __fx[0]; (`${{a}}`, ()) }}",
						setup_fn(&[recv_rune.clone()])
					),
					plain_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
					staged(
						&one(&recv_rust),
						"crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{}\", __recv)))",
					),
				),
				"PartialEq" => (
					format!(
						"{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; (a == b, ()) }}",
						setup_fn(&[recv_rune.clone(), recv_rune.clone()])
					),
					plain_side(
						"rune::from_value::<bool>(v).map(|x| format!(\"{x}\")).map_err(|e| e.to_string())",
					),
					staged(
						&[
							("__recv".to_string(), recv_rust.clone()),
							("__recv2".to_string(), recv_rust.clone()),
						],
						"crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{}\", __recv == __recv2)))",
					),
				),
				// record 0113: `a[k]` against the same Rust `Index`, the value cloned out
				"IndexGen" => {
					let out_c = info.ret_canonical.clone().unwrap_or_default();
					let Some(show) = o.show(&out_c) else {
						skip(e, "index result has no comparison".into());
						continue;
					};
					o.shown.borrow_mut().insert(out_c.clone());
					let out_w = &world.wrappers[&out_c];
					let (rk, xk) = if info.params[0].0 == "usize" {
						("0", "0usize")
					} else {
						("\"x\"", "\"x\"")
					};
					(
						format!(
							"{} pub fn main(__fx) {{ let a = __fx[0]; (a[{rk}], ()) }}",
							setup_fn(&[recv_rune.clone()])
						),
						wrapped_side(
							&format!(
								"rnx_polars::generated::fixtures::show_{}",
								out_w.rust.to_lowercase()
							),
							true,
						),
						staged(
							&one(&recv_rust),
							&format!(
								"{{ let __r = __recv[{xk}].clone(); crate_oracle::Side::Value({{ let v = &__r; {show} }}) }}"
							),
						),
					)
				}
				// record 0113: `Type::from_iter_x(v)` against `from_iter` over the
				// same item iterator in Rust, on the same fixture vector
				"FromIterGen" => {
					let c = owner.unwrap();
					let w = &world.wrappers[c];
					let (shape, vec_ty) = info.params[0].clone();
					let (Some(rv), Some(xv)) = (
						o.rune_value(&shape),
						o.rust_value(&ty::parse(&vec_ty), &BTreeMap::new(), Some(c), 0),
					) else {
						skip(e, format!("no fixture for the item vector ({vec_ty})"));
						continue;
					};
					let Some(show) = o.show(c) else {
						skip(e, "receiver type has no comparison".into());
						continue;
					};
					o.shown.borrow_mut().insert(c.to_string());
					let iter = info.param_names[0].clone();
					(
						format!(
							"{} pub fn main(__fx) {{ ({}::{}({rv}), ()) }}",
							setup_fn(&[]),
							rune_path(w),
							info.rune_name
						),
						wrapped_side(
							&format!(
								"rnx_polars::generated::fixtures::show_{}",
								w.rust.to_lowercase()
							),
							true,
						),
						staged(
							&[],
							&format!(
								"{{ let __v = {xv}; let __r: {} = <{} as core::iter::FromIterator<_>>::from_iter({iter}); crate_oracle::Side::Value({{ let v = &__r; {show} }}) }}",
								w.spell, w.spell
							),
						),
					)
				}
				// record 0113: `a OP b` against the same borrowed or owned Rust
				// expression, the result shown through its wrapper on both sides
				"GenericOp" => {
					let (rshape, rspec) = info.params[0].clone();
					let (rhs_rune, rhs_rust, rhs_e) = if let Some(p) = rshape.strip_prefix("W:") {
						let (Some(a), Some(b)) = (o.rune_wrapped(p), o.rust_wrapped(p)) else {
							skip(e, format!("no fixture for the operand ({p})"));
							continue;
						};
						(
							a,
							b,
							if rspec.starts_with('&') {
								"&__rhs"
							} else {
								"__rhs"
							},
						)
					} else if rshape == "int" {
						("2".to_string(), "2i64".to_string(), "__rhs")
					} else {
						("1.5".to_string(), "1.5f64".to_string(), "__rhs")
					};
					let out_c = info.ret_canonical.clone().unwrap_or_default();
					let Some(show) = o.show(&out_c) else {
						skip(e, "operator result has no comparison".into());
						continue;
					};
					o.shown.borrow_mut().insert(out_c.clone());
					let out_w = &world.wrappers[&out_c];
					let lhs_e = if info.param_names.first().map(String::as_str) == Some("ref") {
						"&__recv"
					} else {
						"__recv"
					};
					let fallible = info.param_names.get(1).map(String::as_str) == Some("fallible");
					let body = if fallible {
						format!(
							"{{ let __r = {lhs_e} {} {rhs_e}; match __r {{ Ok(__r) => crate_oracle::Side::Value({{ let v = &__r; {show} }}), Err(e) => crate_oracle::Side::Error(crate_oracle::error_kind(&e)) }} }}",
							info.rune_name
						)
					} else {
						format!(
							"{{ let __r = {lhs_e} {} {rhs_e}; crate_oracle::Side::Value({{ let v = &__r; {show} }}) }}",
							info.rune_name
						)
					};
					(
						format!(
							"{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; (a {} b, ()) }}",
							setup_fn(&[recv_rune.clone(), rhs_rune]),
							info.rune_name
						),
						wrapped_side(
							&format!(
								"rnx_polars::generated::fixtures::show_{}",
								out_w.rust.to_lowercase()
							),
							true,
						),
						staged(
							&[
								("__recv".to_string(), recv_rust.clone()),
								("__rhs".to_string(), rhs_rust),
							],
							&body,
						),
					)
				}
				// record 0112: the JSON text against serde_json on the same value
				"Serialize" => (
					format!(
						"{} pub fn main(__fx) {{ let a = __fx[0]; (a.to_json(), ()) }}",
						setup_fn(&[recv_rune.clone()])
					),
					value_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
					staged(
						&one(&recv_rust),
						"match serde_json::to_string(&__recv) { Ok(s) => crate_oracle::Side::Value(crate_oracle::Repr::Text(s)), Err(_) => crate_oracle::Side::Error(\"Json\".into()) }",
					),
				),
				// record 0112: a round trip needs the type's own JSON text; the
				// result is compared by its re-serialized text on both sides
				"Deserialize" => {
					let c = owner.unwrap();
					let w = &world.wrappers[c];
					if !owner_has_trait(world, c, "Serialize") {
						skip(
							e,
							"Deserialize without Serialize: no JSON source of this type".into(),
						);
						continue;
					}
					(
						format!(
							"{} pub fn main(__fx) {{ let a = __fx[0]; (match a.to_json() {{ Ok(s) => match {}::from_json(s) {{ Ok(v) => v.to_json(), Err(e) => Err(e) }}, Err(e) => Err(e) }}, ()) }}",
							setup_fn(&[recv_rune.clone()]),
							rune_path(w)
						),
						value_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
						staged(
							&one(&recv_rust),
							&format!(
								"match serde_json::to_string(&__recv).ok().and_then(|s| serde_json::from_str::<{}>(&s).ok()).and_then(|v| serde_json::to_string(&v).ok()) {{ Some(t) => crate_oracle::Side::Value(crate_oracle::Repr::Text(t)), None => crate_oracle::Side::Error(\"Json\".into()) }}",
								w.spell
							),
						),
					)
				}
				// record 0111: a real map lookup through HASH and EQ; equal
				// fixtures must find each other, as in a Rust HashSet
				"Hash" => (
					format!(
						"{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; let m = std::collections::HashMap::new(); m.insert(a, 1); (`${{m.contains_key(b)}} ${{std::ops::eq(a, b)}} ${{m.len()}}`, ()) }}",
						setup_fn(&[recv_rune.clone(), recv_rune.clone()])
					),
					plain_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
					staged(
						&[
							("__recv".to_string(), recv_rust.clone()),
							("__recv2".to_string(), recv_rust.clone()),
						],
						"{ let eq = __recv == __recv2; let mut s = std::collections::HashSet::new(); s.insert(__recv); crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{} {} {}\", s.contains(&__recv2), eq, s.len()))) }",
					),
				),
				"PartialOrd" | "Ord" => {
					let (f, rust) = if tname == "Ord" {
						("cmp", "Some(core::cmp::Ord::cmp(&__recv, &__recv2))")
					} else {
						(
							"partial_cmp",
							"core::cmp::PartialOrd::partial_cmp(&__recv, &__recv2)",
						)
					};
					let wrap = if tname == "Ord" {
						"Some(std::ops::cmp(a, b))"
					} else {
						"std::ops::partial_cmp(a, b)"
					};
					let _ = f;
					(
						format!(
							"{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; let o = {wrap}; (match o {{ Some(x) => if x == std::cmp::Ordering::Less {{ \"Some(Less)\" }} else if x == std::cmp::Ordering::Equal {{ \"Some(Equal)\" }} else {{ \"Some(Greater)\" }}, None => \"None\" }}, ()) }}",
							setup_fn(&[recv_rune.clone(), recv_rune.clone()])
						),
						plain_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
						staged(
							&[
								("__recv".to_string(), recv_rust.clone()),
								("__recv2".to_string(), recv_rust.clone()),
							],
							&format!(
								"crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"{{:?}}\", {rust})))"
							),
						),
					)
				}
				// record 0111: `parse` of the fixture's own Display text,
				// compared by equality with the fixture on both sides
				"FromStr" => {
					let c = owner.unwrap();
					let w = &world.wrappers[c];
					if !owner_has_trait(world, c, "Display")
						|| !owner_has_trait(world, c, "PartialEq")
					{
						skip(
							e,
							"FromStr round trip needs Display and PartialEq on the type".into(),
						);
						continue;
					}
					(
						format!(
							"{} pub fn main(__fx) {{ let a = __fx[0]; (match {}::parse(`${{a}}`) {{ Ok(v) => `ok ${{v == a}}`, Err(e) => \"err\" }}, ()) }}",
							setup_fn(&[recv_rune.clone()]),
							rune_path(w)
						),
						plain_side("rune::from_value::<String>(v).map_err(|e| e.to_string())"),
						staged(
							&one(&recv_rust),
							&format!(
								"crate_oracle::Side::Value(crate_oracle::Repr::Text(match <{} as core::str::FromStr>::from_str(&format!(\"{{}}\", __recv)) {{ Ok(v) => format!(\"ok {{}}\", v == __recv), Err(_) => \"err\".to_string() }}))",
								w.spell
							),
						),
					)
				}
				"Default" => {
					let c = owner.unwrap();
					let Some(show) = o.show(c) else {
						skip(e, "receiver type has no comparison".into());
						continue;
					};
					let w = &world.wrappers[c];
					o.shown.borrow_mut().insert(c.to_string());
					// the constructor under test is not a fixture: setup is empty
					(
						format!(
							"{} pub fn main(__fx) {{ let a = {}::default_(); (a, ()) }}",
							setup_fn(&[]),
							rune_path(w)
						),
						wrapped_side(
							&format!(
								"rnx_polars::generated::fixtures::show_{}",
								w.rust.to_lowercase()
							),
							false,
						),
						staged(
							&[],
							&format!(
								"crate_oracle::Side::Value({{ let __o = <{}>::default(); let v = &__o; {show} }})",
								w.spell
							),
						),
					)
				}
				t if OPS.iter().any(|(n, _, _)| *n == t) || t == "Neg" => {
					let op = if t == "Neg" {
						"-".to_string()
					} else {
						OPS.iter().find(|(n, _, _)| *n == t).unwrap().2.to_string()
					};
					let ret_t = info.ret_canonical.as_deref().map(ty::parse);
					let Some(ret_t) = ret_t else {
						skip(e, "operator without output".into());
						continue;
					};
					let Some(of) = o.oracle_fmt(&ret_t, owner, 0) else {
						skip(e, "return type has no Rust comparison".into());
						continue;
					};
					let ret_rust = match &ret_t {
						Ty::Path { path, .. } => world.wrappers.get(path).map(|w| w.rust.clone()),
						_ => None,
					};
					let Some(ret_rust) = ret_rust else {
						skip(e, "operator result type not wrapped".into());
						continue;
					};
					let Some(sf) = o.script_fmt(&ret_rust, Some(&ret_t), owner, 0) else {
						skip(e, "return type has no comparison".into());
						continue;
					};
					let of = format!("crate_oracle::Repr::Text({of})");
					if t == "Neg" {
						(
							format!(
								"{} pub fn main(__fx) {{ let a = __fx[0]; (a.neg(), ()) }}",
								setup_fn(&[recv_rune.clone()])
							),
							plain_side(&sf),
							staged(
								&one(&recv_rust),
								&format!(
									"{{ let __r = -__recv; crate_oracle::Side::Value({of}) }}"
								),
							),
						)
					} else {
						let Some((shape, canonical)) = info.params.first() else {
							skip(e, "operator without rhs".into());
							continue;
						};
						let _ = shape;
						let rhs_t = ty::parse(canonical);
						let rhs_rune = match &rhs_t {
							Ty::Path { path, .. } => o.rune_wrapped(path),
							Ty::Ref { inner, .. } => match &**inner {
								Ty::Path { path, .. } => o.rune_wrapped(path),
								_ => None,
							},
							_ => None,
						};
						let Some(rhs_rune) = rhs_rune else {
							skip(e, "no fixture for the operand".into());
							continue;
						};
						let Some(rhs_rust) = o.rust_value(&rhs_t, &BTreeMap::new(), owner, 0)
						else {
							skip(e, "no Rust fixture for the operand".into());
							continue;
						};
						(
							format!(
								"{} pub fn main(__fx) {{ let a = __fx[0]; let b = __fx[1]; (a {op} b, ()) }}",
								setup_fn(&[recv_rune.clone(), rhs_rune.clone()])
							),
							plain_side(&sf),
							staged(
								&[
									("__recv".to_string(), recv_rust.clone()),
									("__rhs".to_string(), rhs_rust.clone()),
								],
								&format!(
									"{{ let __r = __recv {op} __rhs; crate_oracle::Side::Value({of}) }}"
								),
							),
						)
					}
				}
				_ => {
					skip(e, format!("protocol {tname} has no script-level trigger"));
					continue;
				}
			};
			let d = format!(
				"case{}",
				o.recipe_note(&e.canonical_path, &[script.as_str()])
			);
			if let Some(b) = e.bindings.get_mut(bi) {
				b.disposition = Some(d.clone());
				b.case_id = Some(id.clone());
			}
			if single {
				e.execution = Some(d);
			}
			cases.push(OracleCase {
				id,
				path: e.canonical_path.clone(),
				script,
				has_receiver: false,
				fmt,
				oracle,
				unordered: false,
				policy: "ordered (protocol)".into(),
			});
			continue;
		}
		// receiver: a single-binding trait method takes its one implementor
		if !info.implementors.is_empty() {
			if let Some((c, w)) = info
				.implementors
				.iter()
				.find(|(c, _)| o.rune_wrapped(c).is_some() && o.rust_wrapped(c).is_some())
				.cloned()
			{
				info.callee = info.callee.replacen(
					&format!("<{}", world.wrappers[&info.owner.as_ref().unwrap().0].spell),
					&format!("<{}", world.wrappers[&c].spell),
					1,
				);
				info.rune_owner = Some(rune_path(&world.wrappers[&c]));
				info.owner = Some((c, w));
			}
		}
		let owner = info.owner.as_ref().map(|(c, _)| c.as_str());
		let recv_rune = match owner {
			Some(c) => o.rune_wrapped(c),
			None => None,
		};
		if info.receiver != "none" && recv_rune.is_none() {
			skip(
				e,
				format!(
					"no fixture for the receiver type ({})",
					owner
						.and_then(|c| o.no_recipe.get(c))
						.cloned()
						.unwrap_or_else(|| "no wrapper".into())
				),
			);
			continue;
		}
		let recv_rust = match owner {
			Some(c) => o.rust_wrapped(c),
			None => None,
		};
		if info.receiver != "none" && recv_rust.is_none() {
			skip(e, "no Rust fixture for the receiver type".into());
			continue;
		}
		// arguments
		let mut rune_args = Vec::new();
		let mut rust_args = Vec::new();
		let mut missing = None;
		for (shape, canonical) in &info.params {
			if let Some(signature) = shape.strip_prefix("callback:") {
				match world
					.release
					.callback_recipe
					.iter()
					.find(|r| r.signature == signature)
				{
					Some(recipe) => {
						rune_args.push(recipe.rune.clone());
						rust_args.push(recipe.rust.clone());
					}
					None => {
						missing = Some(format!(
							"no callback-safe recipe for the closure signature ({signature})"
						));
						break;
					}
				}
				continue;
			}
			o.mask_len.set(if shape.contains("mask1") {
				1
			} else if shape.contains("mask2") {
				2
			} else {
				3
			});
			match (
				o.rune_value(shape),
				o.rust_value(&ty::parse(canonical), &info.generics, owner, 0),
			) {
				(Some(a), Some(b)) => {
					rune_args.push(a);
					rust_args.push(b);
				}
				_ => {
					missing = Some(format!("no fixture for a parameter ({canonical})"));
					break;
				}
			}
		}
		if let Some(why) = missing {
			skip(e, why);
			continue;
		}
		let named: Vec<(String, String)> = info
			.param_names
			.iter()
			.cloned()
			.zip(rune_args.iter().cloned())
			.collect();
		let (unordered, policy) = world.release.policy(&e.canonical_path, &named);
		let mutating = info.receiver == "&mut self";
		let ret_ty = info.ret_canonical.as_deref().map(ty::parse);
		let rust_is_result = matches!(&ret_ty, Some(Ty::Path { path, .. }) if path == "polars_error::PolarsResult" || path == "core::result::Result");
		// return formatting on both sides
		// top-level wrapped returns are compared as structured values under the
		// case's policy; everything else as ordered text
		let top = if mutating {
			None
		} else {
			ret_ty.as_ref().and_then(|t| o.top_wrapped(t, owner))
		};
		let (ret_fmt_script, ret_fmt_rust) = if info.ret_rust == "()" {
			// unit, or a `&mut Self` chain reduced to unit; Rust may still be a Result
			let of = if rust_is_result {
				"match __r { Ok(_) => \"()\".to_string(), Err(e) => format!(\"<<ERR:{}>>\", crate_oracle::error_kind(&e)) }".to_string()
			} else {
				"{ let _ = __r; \"()\".to_string() }".to_string()
			};
			("Ok(\"()\".to_string())".to_string(), of)
		} else if let Some((canonical, _)) = &top {
			let w = &world.wrappers[canonical];
			o.shown.borrow_mut().insert(canonical.clone());
			(
				format!(
					"rnx_polars::generated::fixtures::show_{}",
					w.rust.to_lowercase()
				),
				o.show(canonical).unwrap(),
			)
		} else {
			*o.layout_native.borrow_mut() = world
				.release
				.layout_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.pair_for(&w.identity))
						.map(|(_, k)| k)
				});
			let Some(sf) = o.script_fmt(&info.ret_rust, ret_ty.as_ref(), owner, 0) else {
				*o.layout_native.borrow_mut() = None;
				skip(
					e,
					format!("return type has no comparison ({})", info.ret_rust),
				);
				continue;
			};
			o.hash_ret.set(
				world.release.hash_tokens.iter().any(|h| {
					h.key == e.key && h.path == e.canonical_path && h.direction == "return"
				}),
			);
			*o.layout_native.borrow_mut() = world
				.release
				.layout_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.pair_for(&w.identity))
						.map(|(_, k)| k)
				});
			*o.owned_native.borrow_mut() = world
				.release
				.owned_iter_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.native_for(&w.identity))
						.map(String::from)
				});
			*o.view_native.borrow_mut() = world
				.release
				.view_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.native_for(&w.identity))
						.map(String::from)
				});
			*o.iter_native.borrow_mut() = world
				.release
				.iter_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.native_for(&w.identity))
						.map(String::from)
				});
			*o.array_native.borrow_mut() = world
				.release
				.array_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.native_for(&w.identity))
						.map(String::from)
				});
			*o.indexed_native.borrow_mut() = world
				.release
				.indexed_chunk_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.native_for(&w.identity))
						.map(String::from)
				});
			*o.chunk_native.borrow_mut() = world
				.release
				.chunk_snapshots
				.iter()
				.find(|m| m.key == e.key && m.path == e.canonical_path)
				.and_then(|m| {
					owner
						.and_then(|a| world.wrappers.get(a))
						.and_then(|w| m.native_for(&w.identity))
						.map(String::from)
				});
			let of = match &ret_ty {
				None => Some("\"()\".to_string()".to_string()),
				Some(t) => o.oracle_fmt(t, owner, 0),
			};
			o.hash_ret.set(false);
			*o.chunk_native.borrow_mut() = None;
			*o.indexed_native.borrow_mut() = None;
			*o.array_native.borrow_mut() = None;
			*o.iter_native.borrow_mut() = None;
			*o.view_native.borrow_mut() = None;
			*o.owned_native.borrow_mut() = None;
			*o.layout_native.borrow_mut() = None;
			let Some(of) = of else {
				skip(e, "return type has no Rust comparison".into());
				continue;
			};
			(sf, of)
		};
		// Rune: the prepared fixtures arrive in `__fx`; a two-call case gets
		// a second, separately prepared argument set for the second call
		let n = rune_args.len();
		let recv_in_fx = info.receiver != "none";
		let off = if recv_in_fx { 1 } else { 0 };
		let args_r = (0..n)
			.map(|i| format!("__fx[{}]", off + i))
			.collect::<Vec<_>>()
			.join(", ");
		let args_r2 = (0..n)
			.map(|i| format!("__fx[{}]", off + n + i))
			.collect::<Vec<_>>()
			.join(", ");
		let mut setup_rune: Vec<String> = Vec::new();
		if recv_in_fx {
			setup_rune.push(recv_rune.clone().unwrap());
		}
		setup_rune.extend(rune_args.iter().cloned());
		// Rust: the same fixtures, named, built in the staged setup
		let mut fixtures: Vec<(String, String)> = Vec::new();
		let mut passed = Vec::new();
		for (i, a) in rust_args.iter().enumerate() {
			let callback = info.params[i].0.starts_with("callback:");
			let raw = info.params[i].1.as_str();
			let mut_borrow = callback && raw.starts_with("&mut ");
			fixtures.push((
				format!("{}__a{i}", if mut_borrow { "mut " } else { "" }),
				a.clone(),
			));
			passed.push(format!(
				"{}__a{i}",
				if mut_borrow {
					"&mut "
				} else if callback && raw.starts_with('&') {
					"&"
				} else {
					""
				}
			));
		}
		let args_s = passed.join(", ");
		let (script, fmt, oracle) = if mutating {
			let c = owner.unwrap();
			let Some(show) = o.show(c) else {
				skip(e, "receiver type has no comparison".into());
				continue;
			};
			let w = &world.wrappers[c];
			o.shown.borrow_mut().insert(c.to_string());
			// script: (receiver after the call, return); both compared
			let script = if ASSIGN_OPS.iter().any(|(_, _, op, _)| *op == info.rune_name) {
				// an assignment operator: the statement form, no return value
				format!(
					"{} pub fn main(__fx) {{ let a = __fx[0]; a {} {args_r}; let r = (); ((a, r), ()) }}",
					setup_fn(&setup_rune),
					info.rune_name
				)
			} else {
				format!(
					"{} pub fn main(__fx) {{ let a = __fx[0]; let r = a.{}({args_r}); ((a, r), ()) }}",
					setup_fn(&setup_rune),
					info.rune_name
				)
			};
			// receiver state after the call and the return are compared together;
			// an error return keeps the receiver in the comparison as `ret=ERR:kind`
			let fmt = format!(
				"{{ let (a, r) = match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok(x) => x, Err(e) => return crate_oracle::Side::Broken(e.to_string()) }}; let recv = match rnx_polars::generated::fixtures::show_{}(&a) {{ Ok(s) => s.to_text(), Err(e) => return crate_oracle::Side::Broken(e) }}; let ret = {{ let v = r; {} }}; match ret {{ crate_oracle::Side::Value(s) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{}}\", s.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret=ERR:{{k}}\"))), other => other }} }}",
				w.rust.to_lowercase(),
				if info.fallible {
					value_side(&ret_fmt_script)
				} else {
					plain_side(&ret_fmt_script)
				}
			);
			let mut fx = vec![("__o".to_string(), recv_rust.clone().unwrap())];
			fx.extend(fixtures.iter().cloned());
			let oracle = staged(&fx, &format!("{{ let mut __o = __o; let __r = {}({}__o, {args_s}); let ret = {}; let recv = ({{ let v = &__o; {show} }}).to_text(); let ret = ret.replace(\"<<ERR:\", \"ERR:\").replace(\">>\", \"\"); crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"recv={{recv}};ret={{ret}}\"))) }}", info.callee, if info.deref { "&mut *" } else { "&mut " }, ret_fmt_rust).replace(", )", ")"));
			(script, fmt, oracle)
		} else {
			let script = match (&info.rune_owner, info.receiver.as_str()) {
				(Some(_), "none") => format!(
					"{} pub fn main(__fx) {{ let r = {}::{}({args_r}); (r, ()) }}",
					setup_fn(&setup_rune),
					info.rune_owner.as_ref().unwrap(),
					info.rune_name
				),
				// record 0109: a moved receiver takes one call; its reuse is an
				// access error, covered by tests/move_semantics.rs
				(Some(_), "self") if owner.is_some_and(|c| !world.clonable.contains(c)) => format!(
					"{} pub fn main(__fx) {{ let a = __fx[0]; let r = a.{}({args_r}); (r, ()) }}",
					setup_fn(&setup_rune),
					info.rune_name
				),
				(Some(_), _) => {
					let mut twice = setup_rune.clone();
					twice.extend(rune_args.iter().cloned());
					format!(
						"{} pub fn main(__fx) {{ let a = __fx[0]; let r = a.{}({args_r}); let r2 = a.{}({args_r2}); (r, r2) }}",
						setup_fn(&twice),
						info.rune_name,
						info.rune_name
					)
				}
				(None, _) => format!(
					"{} pub fn main(__fx) {{ let r = polars::{}({args_r}); (r, ()) }}",
					setup_fn(&setup_rune),
					info.rune_name
				),
			};
			let fmt = match &top {
				Some((_, _)) => wrapped_side(&ret_fmt_script, info.fallible),
				None => {
					if info.fallible {
						value_side(&ret_fmt_script)
					} else {
						plain_side(&ret_fmt_script)
					}
				}
			};
			let mut fx: Vec<(String, String)> = Vec::new();
			if info.receiver != "none" {
				fx.push(("__recv".to_string(), recv_rust.clone().unwrap()));
			}
			fx.extend(fixtures.iter().cloned());
			let call = match info.receiver.as_str() {
				"none" => format!("let __r = {}({args_s});", info.callee),
				"self" => format!("let __r = {}(__recv, {args_s});", info.callee),
				"&self" if info.deref => format!("let __r = {}(&*__recv, {args_s});", info.callee),
				"&self" => format!("let __r = {}(&__recv, {args_s});", info.callee),
				_ => {
					skip(e, "receiver form".into());
					continue;
				}
			}
			.replace(", )", ")");
			let oracle_body = match &top {
				Some((_, true)) => format!(
					"match __r {{ Ok(__r) => crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }}), Err(e) => crate_oracle::Side::Error(crate_oracle::error_kind(&e)) }}"
				),
				Some((_, false)) => {
					format!("crate_oracle::Side::Value({{ let v = &__r; {ret_fmt_rust} }})")
				}
				None => format!("crate_oracle::collapse({ret_fmt_rust})"),
			};
			// record 0078: a `From` constructor with a wrapped, clonable source
			// also compares the source after the call (the Rune value is
			// cloned out, so it must be unchanged); a scalar source is copied
			let from_source = if info.callee.contains(" as From<")
				&& info.receiver == "none"
				&& info.params.len() == 1
				&& !info.params[0].1.trim().starts_with('&')
			{
				let src = info.params[0].1.trim().to_string();
				match (
					world.wrappers.get(&src),
					o.show(&src),
					world.clonable.contains(&src),
				) {
					(Some(sw), Some(show), true) => Some((sw.rust.to_lowercase(), show, src)),
					_ => None,
				}
			} else {
				None
			};
			match from_source {
				Some((src_fn, src_show, src)) => {
					o.shown.borrow_mut().insert(src);
					let script = format!(
						"{} pub fn main(__fx) {{ let s = __fx[0]; let r = {}::{}(s); ((r, s), ()) }}",
						setup_fn(&setup_rune),
						info.rune_owner.as_ref().unwrap(),
						info.rune_name
					);
					let fmt = format!(
						"{{ let (v, s) = match rune::from_value::<(rune::Value, rune::Value)>(v) {{ Ok(x) => x, Err(e) => return crate_oracle::Side::Broken(e.to_string()) }}; let src = match rnx_polars::generated::fixtures::show_{src_fn}(&s) {{ Ok(x) => x.to_text(), Err(e) => return crate_oracle::Side::Broken(e) }}; let ret = {{ {fmt} }}; match ret {{ crate_oracle::Side::Value(r) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret={{}};src={{src}}\", r.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret=ERR:{{k}};src={{src}}\"))), other => other }} }}"
					);
					let call = call.replace("(__a0)", "(__a0.clone())");
					let body = format!(
						"{{ let __src = ({{ let v = &__a0; {src_show} }}).to_text(); {call} let ret = {oracle_body}; match ret {{ crate_oracle::Side::Value(r) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret={{}};src={{__src}}\", r.to_text()))), crate_oracle::Side::Error(k) => crate_oracle::Side::Value(crate_oracle::Repr::Text(format!(\"ret=ERR:{{k}};src={{__src}}\"))), other => other }} }}"
					);
					(script, fmt, staged(&fx, &body))
				}
				None => (
					script,
					fmt,
					staged(&fx, &format!("{{ {call} {oracle_body} }}")),
				),
			}
		};
		let d = format!(
			"case{}",
			o.recipe_note(&e.canonical_path, &[script.as_str()])
		);
		if let Some(b) = e.bindings.get_mut(bi) {
			b.disposition = Some(d.clone());
			b.case_id = Some(id.clone());
		}
		if single {
			e.execution = Some(d);
		}
		let moved = info.receiver == "self" && owner.is_some_and(|c| !world.clonable.contains(c));
		cases.push(OracleCase {
			id,
			path: e.canonical_path.clone(),
			script,
			has_receiver: !mutating
				&& !moved && (info.receiver == "self" || info.receiver == "&self"),
			fmt,
			oracle,
			unordered,
			policy,
		});
	}
	// callable-level disposition of a multi-binding entry: a case if any
	// binding has one, else the first binding's reason
	for e in entries.iter_mut() {
		if e.status == "generated" && e.bindings.len() > 1 {
			let n = e.bindings.iter().filter(|b| b.case_id.is_some()).count();
			e.execution = Some(if n > 0 {
				format!("case (on {n} of {} receivers)", e.bindings.len())
			} else {
				e.bindings[0]
					.disposition
					.clone()
					.unwrap_or_else(|| "no disposition".into())
			});
		}
	}
	// fixtures module inside the adapter
	let mut fixtures = String::from(
		"//! GENERATED by tools/polars-gen: fixtures for the generated oracle tests.\n//! Only built with the `test-support` feature.\n#![allow(dead_code, non_snake_case, unused_imports, clippy::all)]\nuse super::types::*;\nuse crate::oracle as crate_oracle;\nuse crate::{DataFrame, Expr, LazyFrame, LazyGroupBy};\nuse polars::prelude as p;\nuse polars::prelude::{IntoColumn, IntoLazy};\nuse rnx::rune;\nuse values::*;\n\n/// The Polars values the oracle tests use on both sides.\npub mod values {\n    use polars::prelude as p;\n    use polars::prelude::*;\n",
	);
	for (_, name, expr, _, _) in TYPED_FIXTURES {
		writeln!(fixtures, "    pub fn {name}() -> p::Series {{ {expr} }}").unwrap();
	}
	for (_, name, expr, _) in FIXTURES {
		let ret = match *name {
			"df" => "p::DataFrame",
			"lf" => "p::LazyFrame",
			"expr" => "p::Expr",
			"series" => "p::Series",
			"column" => "p::Column",
			"dtype" => "p::DataType",
			"field" => "p::Field",
			"group_by" => "p::LazyGroupBy",
			"null_chunked" => "p::NullChunked",
			"categories" => "polars_dtype::categorical::Categories",
			"frozen_categories" => "polars_dtype::categorical::FrozenCategories",
			"categorical_mapping" => "polars_dtype::categorical::CategoricalMapping",
			_ => unreachable!("core fixture {name} has no return type"),
		};
		writeln!(fixtures, "    pub fn {name}() -> {ret} {{ {expr} }}").unwrap();
	}
	fixtures.push_str("}\n\n");
	// a synthetic inventory (self-test) may lack the fixture types; the
	// drift test and the oracle build catch a real inventory missing one
	for (canonical, name, _, _) in FIXTURES {
		let Some(w) = world.wrappers.get(*canonical) else {
			continue;
		};
		writeln!(
			fixtures,
			"#[rune::function(path = {name})]\nfn fx_{name}() -> {} {{ {}(values::{name}()) }}",
			w.rust, w.rust
		)
		.unwrap();
	}
	for (canonical, name, _, _, _) in TYPED_FIXTURES {
		let Some(w) = world.wrappers.get(*canonical) else {
			continue;
		};
		writeln!(
			fixtures,
			"#[rune::function(path = {name})]\nfn fx_{name}() -> {} {{ {}(values::{name}()) }}",
			w.rust, w.rust
		)
		.unwrap();
	}
	let mut shown_structs: BTreeSet<String> = BTreeSet::new();
	for canonical in o.shown.borrow().iter() {
		let w = &world.wrappers[canonical];
		if !shown_structs.insert(w.rust.clone()) {
			continue; // one show function per wrapper struct, however many aliases share it
		}
		let show = o.show(canonical).unwrap();
		writeln!(fixtures, "/// Show a `{}` held in a Rune value, for the oracle tests.\npub fn show_{}(v: &rune::Value) -> Result<crate_oracle::Repr, String> {{ v.borrow_ref::<{}>().map_err(|e| e.to_string()).map(|w| {{ let v = &w.0; {show} }}) }}", canonical, w.rust.to_lowercase(), w.rust).unwrap();
	}
	fixtures
		.push_str("\npub fn install(m: &mut rune::Module) -> Result<(), rune::ContextError> {\n");
	for (_, name, _, _) in FIXTURES {
		writeln!(fixtures, "    m.function_meta(fx_{name})?;").unwrap();
	}
	for (_, name, _, _, _) in TYPED_FIXTURES {
		writeln!(fixtures, "    m.function_meta(fx_{name})?;").unwrap();
	}
	fixtures.push_str("    Ok(())\n}\n");
	// the test file
	let mut t = String::from(concat!(include_str!("harness_head.rs.in"), "\n"));
	for c in &cases {
		writeln!(
			t,
			"fn fmt_{}(v: Value) -> Side {{ {} }}\nfn oracle_{}() -> Staged {{ {} }}",
			c.id, c.fmt, c.id, c.oracle
		)
		.unwrap();
	}
	t.push_str("\nstatic CASES: &[Case] = &[\n");
	for c in &cases {
		writeln!(t, "    Case {{ id: {:?}, path: {:?}, script: {:?}, has_receiver: {}, unordered: {}, policy: {:?}, fmt: fmt_{}, oracle: oracle_{} }},", c.id, c.path, c.script, c.has_receiver, c.unordered, c.policy, c.id, c.id).unwrap();
	}
	t.push_str("];\n");
	t.push_str(include_str!("harness_runner.rs.in"));
	// the join controls' order policies come from the production rule
	let named = |v: &[(&str, &str)]| {
		v.iter()
			.map(|(a, b)| (a.to_string(), b.to_string()))
			.collect::<Vec<_>>()
	};
	let base = [
		("other", "fx::lf()"),
		("left_on", "[fx::expr()]"),
		("right_on", "[fx::expr()]"),
	];
	let j1 = world.release.policy(
		"polars_lazy::frame::LazyFrame::inner_join",
		&named(&[
			("other", "fx::lf()"),
			("left_on", "fx::expr()"),
			("right_on", "fx::expr()"),
		]),
	);
	let mut with_left = base.to_vec();
	with_left.push((
		"args",
		"polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::Left())",
	));
	let j2 = world
		.release
		.policy("polars_lazy::frame::LazyFrame::join", &named(&with_left));
	let mut with_none = base.to_vec();
	with_none.push((
		"args",
		"polars::JoinArgs::default_().with_maintain_order(polars::MaintainOrderJoin::LeftRight())",
	));
	let j4 = world
		.release
		.policy("polars_lazy::frame::LazyFrame::join", &named(&with_none));
	assert!(
		!j2.0,
		"the release policy must keep an explicit order ordered: {}",
		j2.1
	);
	assert!(
		!j4.0,
		"the release policy must keep an unrecognized configuration ordered: {}",
		j4.1
	);
	println!("join controls: j1 {} | j2 {} | j4 {}", j1.1, j2.1, j4.1);
	let t = t
		.replace("@J1@", &j1.0.to_string())
		.replace("@J1_POLICY@", &j1.1.replace('"', "'"))
		.replace("@J2@", &j2.0.to_string())
		.replace("@J2_POLICY@", &j2.1.replace('"', "'"))
		.replace("@J4@", &j4.0.to_string())
		.replace("@J4_POLICY@", &j4.1.replace('"', "'"));
	let recipes = serde_json::json!({
		"derived": o.recipes.iter().map(|(c, r)| serde_json::json!({"type": c, "recipe": r.kind, "rune": r.rune, "rust": r.rust})).collect::<Vec<_>>(),
		"none": o.no_recipe.iter().map(|(c, why)| serde_json::json!({"type": c, "reason": why})).collect::<Vec<_>>(),
	});
	(fixtures, t, skipped, recipes)
}
