//! The one JSON serializer rnx offers a script, and the walk that bounds it.
//!
//! Rune's `Serialize` for `Value` recurses with no cycle guard and no depth
//! bound, so a cyclic value aborts the process with a stack overflow rather
//! than returning an error. Bounding it is this module's job: a value is
//! walked here first and refused before serde is ever handed it, so nothing
//! reaches serde that this walk has not already accepted.
//!
//! The walk is not a guess at what serde does. It is read off
//! `rune-0.14.1/src/runtime/value/serde.rs`, arm for arm: four shapes descend
//! (an option, a sequence, a tuple, an object) and everything else is a leaf
//! that is either serialized as it stands or refused. A `Result` is refused,
//! which is easy to get wrong by assuming it behaves like an option.
//!
//! Nothing here runs script code: no formatting protocol is invoked through
//! the virtual machine, which is the rule `format.rs` has followed since it
//! was written.
use crate::format::MAX_DEPTH;
use rune::runtime::{Function, Object, OwnedTuple, TypeValue, Value, Vec as RuneVec};

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
	walk(value, 0, &mut Vec::new(), &mut Vec::new())?;
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
	// The bound is enforced on entry to every value, not only where a
	// container is descended. An option is a step of recursion that adds no
	// container of its own, and a chain of them reached serde unbounded until
	// this check existed: 257 of them serialized, and 8,192 aborted the
	// process. This is the check the renderer makes in `value`, at the same
	// threshold, so the two agree about what is too deep.
	if depth > MAX_DEPTH {
		return Err(too_deep(path));
	}
	if let Ok(_s) = value.borrow_string_ref() {
		return Ok(());
	}
	if let Ok(v) = value.borrow_ref::<RuneVec>() {
		let address = &*v as *const RuneVec as usize;
		let items: Vec<Value> = v.iter().cloned().collect();
		drop(v);
		return descend(&items, address, depth, path, active, None);
	}
	if let Ok(b) = value.borrow_ref::<rune::runtime::Bytes>() {
		// serde writes these as an array of numbers.
		let _ = &*b;
		return Ok(());
	}
	if let Ok(o) = value.borrow_ref::<Option<Value>>() {
		let inner: Option<Value> = (*o).clone();
		drop(o);
		// The serializer serializes the inside, so an option adds a level to
		// the walk but no step to the path: there is nothing to name.
		return match inner {
			Some(inner) => walk(&inner, depth + 1, path, active),
			None => Ok(()),
		};
	}
	if value.borrow_ref::<Result<Value, Value>>().is_ok() {
		// Measured: `host::json_stringify(Ok(1))` refuses. A result is not in
		// the serializer's list of known types, so it is an external
		// reference like any other.
		return Err(refuse("a result", path));
	}
	if value.borrow_ref::<Function>().is_ok() {
		return Err(refuse("a function", path));
	}
	match value.as_type_value() {
		Ok(TypeValue::Unit) => Ok(()),
		Ok(TypeValue::Tuple(t)) => {
			let address = &*t as *const OwnedTuple as usize;
			let items: Vec<Value> = t.iter().cloned().collect();
			drop(t);
			descend(&items, address, depth, path, active, None)
		}
		Ok(TypeValue::Object(o)) => {
			let address = &*o as *const Object as usize;
			let entries: Vec<(String, Value)> = o
				.iter()
				.map(|(k, v)| (k.as_str().to_owned(), v.clone()))
				.collect();
			drop(o);
			let items: Vec<Value> = entries.iter().map(|(_, v)| v.clone()).collect();
			let keys: Vec<String> = entries.into_iter().map(|(k, _)| k).collect();
			descend(&items, address, depth, path, active, Some(&keys))
		}
		// Every `RttiKind` refuses, and none of them descends: the serializer
		// gives up on a struct before looking inside it, so the walk does too
		// and the path names the struct rather than something within it.
		Ok(TypeValue::EmptyStruct(s)) => {
			Err(refuse(&format!("empty struct {}", s.rtti().item()), path))
		}
		Ok(TypeValue::TupleStruct(s)) => {
			Err(refuse(&format!("tuple struct {}", s.rtti().item()), path))
		}
		Ok(TypeValue::Struct(s)) => Err(refuse(&format!("struct {}", s.rtti().item()), path)),
		Ok(TypeValue::NotTypedInline(_)) => inline(value, path),
		// An unrecognised type is what the serializer calls an external
		// reference. It is refused rather than passed through, because the
		// walk's promise is that serde sees nothing it has not accepted.
		Ok(_) | Err(_) => Err(refuse(
			&format!("{} (an external reference)", value.type_info()),
			path,
		)),
	}
}

/// An inline value: those the serializer writes, and those it refuses.
fn inline(value: &Value, path: &[Step]) -> Result<(), String> {
	let serializable = rune::from_value::<bool>(value.clone()).is_ok()
		|| rune::from_value::<char>(value.clone()).is_ok()
		|| value.as_integer::<i64>().is_ok()
		|| value.as_integer::<u64>().is_ok()
		|| rune::from_value::<f64>(value.clone()).is_ok();
	if serializable {
		return Ok(());
	}
	// What is left is an empty value, a type, an ordering, or a type hash,
	// each of which the serializer refuses by name.
	Err(refuse(&format!("{}", value.type_info()), path))
}

/// Descend into a container: the depth bound, the cycle guard, and one step
/// of path per child. `keys`, when present, names the children.
fn descend(
	items: &[Value],
	address: usize,
	depth: usize,
	path: &mut Vec<Step>,
	active: &mut Vec<usize>,
	keys: Option<&[String]>,
) -> Result<(), String> {
	if active.contains(&address) {
		return Err(refuse("a cycle", path));
	}
	if depth >= MAX_DEPTH {
		return Err(too_deep(path));
	}
	active.push(address);
	for (index, item) in items.iter().enumerate() {
		path.push(match keys {
			Some(keys) => Step::Key(keys[index].clone()),
			None => Step::Index(index),
		});
		let outcome = walk(item, depth + 1, path, active);
		path.pop();
		if let Err(reason) = outcome {
			active.pop();
			return Err(reason);
		}
	}
	active.pop();
	Ok(())
}
