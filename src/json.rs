//! The JSON reader, guarded serializer, and the walk that bounds writing.
//!
//! Rune bounds recursive serialization with its value-walk limit. This module
//! first walks the same graph to give cycle and depth errors a useful path,
//! using the same bound and counting the root as level one. Arrays, tuples,
//! objects and options descend; unsupported values are refused before serde.
//!
//! Nothing here runs script code: no formatting protocol is invoked through
//! the virtual machine, which is the rule `format.rs` has followed since it
//! was written.
use crate::format::MAX_DEPTH;
use rune::runtime::{Function, Object, OwnedTuple, TypeValue, Value, Vec as RuneVec};

/// Read with serde_json's recursion guard intact, then preserve its numeric
/// representation. Rune 0.14.2's Deserialize casts u64 to i64, wrapping large
/// positive integers; its unsigned Value variant does not have that defect.
pub fn parse(text: &str) -> Result<Value, String> {
	let document: serde_json::Value = serde_json::from_str(text).map_err(|error| {
		let reason = format!("cannot parse JSON document: {error}");
		// serde_json exposes the error category, but not this specific code.
		// Boundary gates pin both the counter and this diagnostic spelling.
		if error.to_string().starts_with("recursion limit exceeded") {
			format!(
				"{reason}; the reader allows at most 127 nested arrays or objects (recursion counter 128)"
			)
		} else {
			reason
		}
	})?;
	from_document(document).map_err(|error| format!("cannot convert parsed JSON document: {error}"))
}

/// Only called on a tree already bounded by the parser. Consuming the tree
/// avoids retaining a second complete copy during conversion. Allocation
/// failures in Rune's containers remain catchable errors.
fn from_document(document: serde_json::Value) -> Result<Value, rune::alloc::Error> {
	use serde_json::Value as Json;
	Ok(match document {
		Json::Null => Value::from(()),
		Json::Bool(value) => Value::from(value),
		Json::Number(value) => {
			if let Some(value) = value.as_i64() {
				Value::from(value)
			} else if let Some(value) = value.as_u64() {
				Value::from(value)
			} else {
				// With arbitrary_precision off every remaining Number is f64.
				Value::from(
					value
						.as_f64()
						.expect("serde_json number is i64, u64, or f64"),
				)
			}
		}
		Json::String(value) => Value::try_from(rune::alloc::String::try_from(value.as_str())?)?,
		Json::Array(values) => {
			let mut array = RuneVec::with_capacity(values.len())?;
			for value in values {
				array.push(from_document(value)?)?;
			}
			Value::try_from(array)?
		}
		Json::Object(values) => {
			let mut object = Object::with_capacity(values.len())?;
			for (key, value) in values {
				object.insert(
					rune::alloc::String::try_from(key.as_str())?,
					from_document(value)?,
				)?;
			}
			Value::try_from(object)?
		}
	})
}

/// One step of the way into a value, for naming where a refusal happened.
enum Step {
	Key(String),
	Index(usize),
}

/// Whether a key can be written as `.name` without ambiguity.
fn plain(key: &str) -> bool {
	!key.is_empty()
		&& !key.starts_with(|c: char| c.is_ascii_digit())
		&& key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Where in a value the walk stopped. A key that is a plain identifier reads
/// as `.name`; anything else is quoted and escaped, so a key containing a
/// dot, a bracket, a quote, or a control character cannot be confused with
/// the path it would otherwise imitate: the key `a.b` is `["a.b"]` while the
/// nested keys `a` then `b` are `.a.b`, and the key `3` is `["3"]` while the
/// fourth element of a sequence is `[3]`.
fn where_it_is(path: &[Step]) -> String {
	if path.is_empty() {
		return "the value itself".to_owned();
	}
	let mut out = String::new();
	for step in path {
		match step {
			Step::Key(key) if plain(key) => {
				out.push('.');
				out.push_str(key);
			}
			// `{:?}` quotes and escapes, control characters included.
			Step::Key(key) => out.push_str(&format!("[{key:?}]")),
			Step::Index(index) => out.push_str(&format!("[{index}]")),
		}
	}
	out
}

/// One sentence for a value too deep, wherever the bound was reached.
fn too_deep(path: &[Step]) -> String {
	format!(
		"cannot serialize a value nested deeper than {MAX_DEPTH} levels, reached at {}",
		where_it_is(path)
	)
}

fn refuse(what: &str, path: &[Step]) -> String {
	format!("cannot serialize {what} at {}", where_it_is(path))
}

/// Serialize a value to JSON, or say what could not be serialized and where.
pub fn stringify(value: &Value) -> Result<String, String> {
	walk(value, 1, &mut Vec::new(), &mut Vec::new())?;
	// The walk has accepted every leaf serde will reach, so a failure here
	// would mean the walk and the serializer disagree. Report it as itself
	// rather than pretending it was refused on purpose.
	serde_json::to_string(value).map_err(|e| format!("the JSON writer refused a walked value: {e}"))
}

/// Walk a value the way Rune's serializer will, refusing what it would refuse
/// and bounding what it would not bound.
///
/// `active` holds the addresses of the containers currently being descended,
/// pushed on the way in and popped on the way out. That is what makes a cycle
/// a repeat on the active path rather than a value seen twice: one allocation
/// referenced twice by the same sequence is repeated data and serializes.
fn walk(
	value: &Value,
	depth: usize,
	path: &mut Vec<Step>,
	active: &mut Vec<usize>,
) -> Result<(), String> {
	validate(&View::Legacy(value.clone()), depth, path, active)
}

// Both representations expose one node at a time. Children are never eagerly
// cloned into a second tree and opaque leaves are never traversed.
enum View<'a> {
	Legacy(Value),
	Owned(&'a rune::host::OwnedTreeInput, usize),
}
enum Kind {
	Leaf,
	Sequence {
		object: bool,
		len: usize,
		identity: usize,
		cycle: bool,
		frontier: bool,
	},
	Option(bool),
	Refused(String),
}
impl<'a> View<'a> {
	fn kind(&self) -> Result<Kind, String> {
		if let Self::Owned(tree, index) = self {
			use rune::host::TreeKind as K;
			let node = tree.node(*index).ok_or("broken owned JSON node")?;
			return Ok(match node.kind() {
				K::DepthCut => {
					return Err("owned JSON depth frontier reached without depth check".into());
				}
				K::Scalar(value) => View::Legacy(value.clone()).kind()?,
				K::String(_) => Kind::Leaf,
				K::Vec | K::Tuple | K::Object => Kind::Sequence {
					object: matches!(node.kind(), K::Object),
					len: node.len(),
					identity: *index,
					cycle: node.cycle(),
					frontier: node.frontier(),
				},
				K::Option(some) => Kind::Option(*some),
				K::Unsupported(what) => Kind::Refused(what.as_str().to_owned()),
			});
		}
		let Self::Legacy(value) = self else {
			unreachable!()
		};
		if value.borrow_string_ref().is_ok() {
			return Ok(Kind::Leaf);
		}
		if let Ok(v) = value.borrow_ref::<RuneVec>() {
			return Ok(Kind::Sequence {
				object: false,
				len: v.len(),
				identity: &*v as *const RuneVec as usize,
				cycle: false,
				frontier: false,
			});
		}
		if value.borrow_ref::<rune::runtime::Bytes>().is_ok() {
			return Ok(Kind::Leaf);
		}
		if let Ok(v) = value.borrow_ref::<Option<Value>>() {
			return Ok(Kind::Option(v.is_some()));
		}
		if value.borrow_ref::<Result<Value, Value>>().is_ok() {
			return Ok(Kind::Refused("a result".into()));
		}
		if value.borrow_ref::<Function>().is_ok() {
			return Ok(Kind::Refused("a function".into()));
		}
		Ok(match value.as_type_value() {
			Ok(TypeValue::Unit) => Kind::Leaf,
			Ok(TypeValue::Tuple(v)) => Kind::Sequence {
				object: false,
				len: v.len(),
				identity: &*v as *const OwnedTuple as usize,
				cycle: false,
				frontier: false,
			},
			Ok(TypeValue::Object(v)) => Kind::Sequence {
				object: true,
				len: v.len(),
				identity: &*v as *const Object as usize,
				cycle: false,
				frontier: false,
			},
			Ok(TypeValue::EmptyStruct(s)) => {
				Kind::Refused(format!("empty struct {}", s.rtti().item()))
			}
			Ok(TypeValue::TupleStruct(s)) => {
				Kind::Refused(format!("tuple struct {}", s.rtti().item()))
			}
			Ok(TypeValue::Struct(s)) => Kind::Refused(format!("struct {}", s.rtti().item())),
			Ok(TypeValue::NotTypedInline(_)) => {
				if rune::from_value::<bool>(value.clone()).is_ok()
					|| rune::from_value::<char>(value.clone()).is_ok()
					|| value.as_integer::<i64>().is_ok()
					|| value.as_integer::<u64>().is_ok()
					|| rune::from_value::<f64>(value.clone()).is_ok()
				{
					Kind::Leaf
				} else {
					Kind::Refused(format!("{}", value.type_info()))
				}
			}
			_ => Kind::Refused(format!("{} (an external reference)", value.type_info())),
		})
	}
	fn option_child(&self) -> Result<Self, String> {
		match self {
			Self::Owned(tree, at) => {
				let (_, child) = tree
					.node(*at)
					.and_then(|n| n.child(0))
					.ok_or("broken owned JSON option")?;
				Ok(Self::Owned(tree, child))
			}
			Self::Legacy(value) => {
				let v = value
					.borrow_ref::<Option<Value>>()
					.map_err(|e| e.to_string())?;
				Ok(Self::Legacy(
					v.as_ref().as_ref().ok_or("missing option value")?.clone(),
				))
			}
		}
	}
	// Borrow each legacy container once and consume its iterator once. Packet
	// edges are also visited once, without copying a wide intermediate list.
	fn visit_children(
		&self,
		mut visit: impl FnMut(Option<&str>, Self) -> Result<(), String>,
	) -> Result<(), String> {
		match self {
			Self::Owned(tree, at) => {
				let node = tree.node(*at).ok_or("broken owned JSON node")?;
				for index in 0..node.len() {
					let (key, child) = node.child(index).ok_or("broken owned JSON child")?;
					visit(key, Self::Owned(tree, child))?;
				}
				Ok(())
			}
			Self::Legacy(value) => {
				if let Ok(v) = value.borrow_ref::<RuneVec>() {
					return visit_once(v.iter().map(|v| (None, Self::Legacy(v.clone()))), visit);
				}
				match value.as_type_value().map_err(|e| e.to_string())? {
					TypeValue::Tuple(v) => {
						visit_once(v.iter().map(|v| (None, Self::Legacy(v.clone()))), visit)
					}
					TypeValue::Object(v) => visit_once(
						v.iter()
							.map(|(k, v)| (Some(k.as_str()), Self::Legacy(v.clone()))),
						visit,
					),
					_ => Err("invalid JSON children".into()),
				}
			}
		}
	}
}
fn visit_once<'k, 'v>(
	entries: impl Iterator<Item = (Option<&'k str>, View<'v>)>,
	mut visit: impl FnMut(Option<&'k str>, View<'v>) -> Result<(), String>,
) -> Result<(), String> {
	#[cfg(test)]
	WALK_COUNTS.with(|c| {
		let (opens, steps) = c.get();
		c.set((opens + 1, steps));
	});
	for (key, value) in entries {
		#[cfg(test)]
		WALK_COUNTS.with(|c| {
			let (opens, steps) = c.get();
			c.set((opens, steps + 1));
		});
		visit(key, value)?;
	}
	Ok(())
}
#[cfg(test)]
std::thread_local! { static WALK_COUNTS: std::cell::Cell<(usize,usize)> = const { std::cell::Cell::new((0,0)) }; }

fn validate(
	value: &View<'_>,
	depth: usize,
	path: &mut Vec<Step>,
	active: &mut Vec<usize>,
) -> Result<(), String> {
	// Option-only chains also advance depth: omitting this check allowed an
	// 8,192-Option chain to reach serde and abort the process.
	if depth > MAX_DEPTH {
		return Err(too_deep(path));
	}
	match value.kind()? {
		Kind::Leaf => Ok(()),
		Kind::Refused(what) => Err(refuse(&what, path)),
		Kind::Option(false) => Ok(()),
		Kind::Option(true) => validate(&value.option_child()?, depth + 1, path, active),
		Kind::Sequence {
			object,
			len,
			identity,
			cycle,
			frontier,
		} => {
			if cycle || active.contains(&identity) {
				return Err(refuse("a cycle", path));
			}
			if depth >= MAX_DEPTH && (len > 0 || frontier) {
				return Err(too_deep(path));
			}
			active.push(identity);
			let mut index = 0;
			let result = value.visit_children(|key, child| {
				path.push(if object {
					Step::Key(key.ok_or("missing JSON field name")?.to_owned())
				} else {
					Step::Index(index)
				});
				index += 1;
				let result = validate(&child, depth + 1, path, active);
				path.pop();
				result
			});
			active.pop();
			result
		}
	}
}

struct Serialized<'a>(&'a rune::host::OwnedTreeInput, usize);
impl serde::Serialize for Serialized<'_> {
	fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
		use rune::host::TreeKind as K;
		use serde::ser::{Error, SerializeMap, SerializeSeq};
		let n = self
			.0
			.node(self.1)
			.ok_or_else(|| S::Error::custom("invalid owned JSON node"))?;
		match n.kind() {
			// Delegate every primitive to the SAME Rune Value Serialize implementation.
			K::Scalar(v) => v.serialize(s),
			K::String(v) => s.serialize_str(v.as_str()),
			K::Option(false) => s.serialize_none(),
			K::Option(true) => s.serialize_some(&Serialized(
				self.0,
				n.child(0)
					.ok_or_else(|| S::Error::custom("invalid option"))?
					.1,
			)),
			K::Vec | K::Tuple => {
				let mut seq = s.serialize_seq(Some(n.len()))?;
				for i in 0..n.len() {
					seq.serialize_element(&Serialized(self.0, n.child(i).unwrap().1))?;
				}
				seq.end()
			}
			K::Object => {
				let mut map = s.serialize_map(Some(n.len()))?;
				for i in 0..n.len() {
					let (k, c) = n.child(i).unwrap();
					map.serialize_entry(k.unwrap(), &Serialized(self.0, c))?;
				}
				map.end()
			}
			K::DepthCut | K::Unsupported(_) => {
				Err(S::Error::custom("unvalidated owned JSON value"))
			}
		}
	}
}
fn stringify_owned(value: rune::host::OwnedTreeInput) -> Result<String, String> {
	if let Some(leaf) = value.leaf() {
		return stringify(leaf);
	}
	validate(&View::Owned(&value, 0), 1, &mut Vec::new(), &mut Vec::new())?;
	serde_json::to_string(&Serialized(&value, 0))
		.map_err(|e| format!("the JSON writer refused a walked value: {e}"))
}

/// The only script-visible JSON reader and guarded writer.
pub(crate) fn install(
	context: &mut rune::Context,
) -> crate::Result<Vec<crate::host::HostFunction>> {
	let mut module = rune::Module::with_crate("json")?;
	module.host_function_with_output("parse", parse, rune::host::OutputCertificate::JSON_RESULT)?;
	module.host_function_tree_input(
		"stringify",
		|value: Value| stringify(&value),
		stringify_owned,
	)?;
	context.install(module)?;
	Ok(vec![
		crate::host::HostFunction {
			path: "json::parse".into(),
			doc: "parse(text) -> Result<value>: read JSON with exact signed/unsigned integers in range and document-position refusals",
		},
		crate::host::HostFunction {
			path: "json::stringify".into(),
			doc: "stringify(value) -> Result<String>: render JSON with cycle and depth guards; refuse unrepresentable values",
		},
	])
}

#[cfg(test)]
mod traversal_controls {
	use super::*;
	#[test]
	fn wide_legacy_containers_visit_once() {
		const N: usize = 100_000;
		let mut object = Object::with_capacity(N).unwrap();
		let mut vector = RuneVec::with_capacity(N).unwrap();
		for n in 0..N {
			object
				.insert(
					rune::alloc::String::try_from(n.to_string().as_str()).unwrap(),
					Value::from(n as i64),
				)
				.unwrap();
			vector.push(Value::from(n as i64)).unwrap();
		}
		let tuple = OwnedTuple::try_from(
			(0..N)
				.map(|n| Value::from(n as i64))
				.collect::<Vec<_>>()
				.into_boxed_slice(),
		)
		.unwrap();
		for value in [
			Value::try_from(object).unwrap(),
			Value::try_from(vector).unwrap(),
			Value::try_from(tuple).unwrap(),
		] {
			WALK_COUNTS.with(|c| c.set((0, 0)));
			walk(&value, 1, &mut Vec::new(), &mut Vec::new()).unwrap();
			assert_eq!(WALK_COUNTS.with(|c| c.get()), (1, N));
		}
	}
}
