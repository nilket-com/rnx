//! Bounded rendering of values for the session. The formatter walks Rune's
//! value model directly and never invokes a formatting protocol through the
//! VM, so rendering a value runs no script code. Every limit is applied
//! before anything is copied or descended into, so the work done is bounded
//! by the output allowed, not by the size of the value.
use crate::session::Session;
use rune::runtime::{Function, Object, OwnedTuple, TypeValue, Value, Vec as RuneVec};

#[derive(Clone, Debug)]
pub struct Limits {
	pub depth: usize,
	pub length: usize,
	pub string_bytes: usize,
	pub total_bytes: usize,
}
impl Default for Limits {
	fn default() -> Self {
		Self {
			depth: 8,
			length: 64,
			string_bytes: 4096,
			total_bytes: 16 * 1024,
		}
	}
}

/// Render a value. `session` supplies field names for structs it declared.
pub fn render(value: &Value, session: Option<&Session>, limits: &Limits) -> String {
	render_with_work(value, session, limits).0
}
/// Render, and report the work done, for tests that bound it.
pub fn render_with_work(
	value: &Value,
	session: Option<&Session>,
	limits: &Limits,
) -> (String, Work) {
	let mut r = Renderer {
		out: String::new(),
		session,
		limits,
		path: Vec::new(),
		truncated: false,
		work: Work::default(),
	};
	r.value(value, 0);
	if r.truncated {
		r.out.push_str(&format!(
			" …(output truncated at {} bytes)",
			limits.total_bytes
		));
	}
	(r.out, r.work)
}

struct Renderer<'a> {
	out: String,
	session: Option<&'a Session>,
	limits: &'a Limits,
	/// Addresses of shared allocations on the current render path.
	path: Vec<usize>,
	truncated: bool,
	work: Work,
}
/// What a rendering examined and copied; tests bound it structurally.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
	pub entries_examined: usize,
	pub key_bytes_copied: usize,
	pub items_copied: usize,
}
/// The longest prefix of `s` within `bytes` on a char boundary.
fn prefix(s: &str, bytes: usize) -> &str {
	if s.len() <= bytes {
		return s;
	}
	let mut end = bytes;
	while !s.is_char_boundary(end) {
		end -= 1;
	}
	&s[..end]
}
impl Renderer<'_> {
	fn full(&self) -> bool {
		self.truncated
	}
	/// Append a piece, or refuse it when it would carry the output past the
	/// budget; once anything has been refused, everything after it is too, so
	/// the output never shows a later piece as if nothing were missing.
	fn push(&mut self, s: &str) {
		if self.truncated || self.out.len() + s.len() > self.limits.total_bytes {
			self.truncated = true;
			return;
		}
		self.out.push_str(s);
	}
	/// Whether to descend into a container at `address` and `depth`. Prints
	/// the cycle or depth marker and answers no when not; the caller has
	/// copied nothing yet.
	fn enter(&mut self, address: usize, depth: usize, cut: &str) -> bool {
		if self.path.contains(&address) {
			self.push("<cycle>");
			return false;
		}
		if depth >= self.limits.depth {
			self.push(cut);
			return false;
		}
		self.path.push(address);
		true
	}
	fn leave(&mut self) {
		self.path.pop();
	}
	fn value(&mut self, value: &Value, depth: usize) {
		if self.full() {
			return;
		}
		// Wrapper types descend too; past the depth nothing is opened.
		if depth > self.limits.depth {
			self.push("…");
			return;
		}
		if let Ok(s) = value.borrow_string_ref() {
			self.string(&s, s.len());
			return;
		}
		if let Ok(v) = value.borrow_ref::<RuneVec>() {
			let address = &*v as *const RuneVec as usize;
			let total = v.len();
			if !self.enter(address, depth, &format!("[…({total} items)]")) {
				return;
			}
			let items: Vec<Value> = v.iter().take(self.limits.length).cloned().collect();
			self.work.items_copied += items.len();
			drop(v);
			self.items("[", "]", &items, total, depth, false);
			self.leave();
			return;
		}
		if let Ok(f) = value.borrow_ref::<Function>() {
			let _ = &*f;
			// Rune 0.14.1 exposes no name or arity on a function value.
			self.push("<function>");
			return;
		}
		if let Ok(b) = value.borrow_ref::<rune::runtime::Bytes>() {
			self.push(&format!("b\"{} bytes\"", b.len()));
			return;
		}
		if let Ok(o) = value.borrow_ref::<Option<Value>>() {
			match &*o {
				Some(inner) => {
					let inner = inner.clone();
					drop(o);
					self.push("Some(");
					self.value(&inner, depth + 1);
					self.push(")");
				}
				None => self.push("None"),
			}
			return;
		}
		if let Ok(r) = value.borrow_ref::<Result<Value, Value>>() {
			let (label, inner) = match &*r {
				Ok(v) => ("Ok(", v.clone()),
				Err(e) => ("Err(", e.clone()),
			};
			drop(r);
			self.push(label);
			self.value(&inner, depth + 1);
			self.push(")");
			return;
		}
		match value.as_type_value() {
			Ok(TypeValue::Unit) => self.push("()"),
			Ok(TypeValue::Tuple(t)) => {
				let address = &*t as *const OwnedTuple as usize;
				let total = t.len();
				if !self.enter(address, depth, &format!("(…({total} items))")) {
					return;
				}
				let items: Vec<Value> = t.iter().take(self.limits.length).cloned().collect();
				drop(t);
				self.items("(", ")", &items, total, depth, true);
				self.leave();
			}
			Ok(TypeValue::Object(o)) => {
				let address = &*o as *const Object as usize;
				let total = o.len();
				if !self.enter(address, depth, &format!("{{…({total} fields)}}")) {
					return;
				}
				// The preview is the first `length` entries in the map's own order,
				// sorted among themselves for stable output. Choosing the globally
				// first keys would mean examining the whole object. Keys are copied
				// only up to the string limit. An object within the length limit
				// therefore renders fully sorted; a larger one shows a subset in
				// the map's order, which is not stable across processes.
				// Each entry carries the key's original byte length, so truncation
				// is known from metadata, never inferred from the copied prefix.
				let limit = self.limits.string_bytes;
				let mut entries: Vec<(String, usize, Value)> = o
					.iter()
					.take(self.limits.length)
					.map(|(k, v)| (prefix(k.as_ref(), limit).to_owned(), k.len(), v.clone()))
					.collect();
				self.work.entries_examined += entries.len();
				self.work.key_bytes_copied +=
					entries.iter().map(|(k, _, _)| k.len()).sum::<usize>();
				entries.sort_by(|a, b| a.0.cmp(&b.0));
				drop(o);
				self.object(&entries, total, depth, true);
				self.leave();
			}
			Ok(TypeValue::EmptyStruct(s)) => self.push(&type_name(s.rtti().item().to_string())),
			Ok(TypeValue::TupleStruct(s)) => {
				let name = type_name(s.rtti().item().to_string());
				let address = s.data().as_ptr() as usize;
				let total = s.data().len();
				self.push(&name);
				if !self.enter(address, depth, &format!("(…({total} items))")) {
					return;
				}
				let items: Vec<Value> = s.data().iter().take(self.limits.length).cloned().collect();
				drop(s);
				self.items("(", ")", &items, total, depth, false);
				self.leave();
			}
			Ok(TypeValue::Struct(s)) => {
				let name = type_name(s.rtti().item().to_string());
				let address = s.data().as_ptr() as usize;
				let total = s.data().len();
				self.push(&name);
				self.push(" ");
				if !self.enter(address, depth, &format!("{{…({total} fields)}}")) {
					return;
				}
				let fields = self
					.session
					.and_then(|session| session.struct_fields(&name).map(|f| f.to_vec()));
				let entries: Vec<(String, usize, Value)> = match fields {
					Some(fields) => fields
						.iter()
						.take(self.limits.length)
						.filter_map(|f| s.get(f.as_str()).map(|v| (f.clone(), f.len(), v.clone())))
						.collect(),
					None => s
						.data()
						.iter()
						.take(self.limits.length)
						.enumerate()
						.map(|(i, v)| (i.to_string(), i.to_string().len(), v.clone()))
						.collect(),
				};
				drop(s);
				self.object(&entries, total, depth, false);
				self.leave();
			}
			Ok(TypeValue::NotTypedInline(_)) => self.inline(value),
			Ok(TypeValue::NotTypedAnyObj(_)) => self.opaque(value),
			Ok(_) => self.opaque(value),
			Err(_) => self.opaque(value),
		}
	}
	fn inline(&mut self, value: &Value) {
		if let Ok(b) = rune::from_value::<bool>(value.clone()) {
			self.push(if b { "true" } else { "false" });
		} else if let Ok(c) = rune::from_value::<char>(value.clone()) {
			self.push(&format!("{c:?}"));
		} else if let Ok(i) = value.as_integer::<i64>() {
			self.push(&i.to_string());
		} else if let Ok(u) = value.as_integer::<u64>() {
			self.push(&u.to_string());
		} else if let Ok(f) = rune::from_value::<f64>(value.clone()) {
			let mut s = f.to_string();
			if !s.contains('.') && !s.contains('e') && f.is_finite() {
				s.push_str(".0");
			}
			self.push(&s);
		} else {
			self.opaque(value);
		}
	}
	fn opaque(&mut self, value: &Value) {
		self.push(&format!("<{}>", value.type_info()));
	}
	/// A string, quoted and escaped, cut at the string limit. `original_len`
	/// is the byte length of the whole string, which a caller knows without
	/// reading it; truncation and the omitted count come from that, so a
	/// key already cut to a prefix still reports exactly what was omitted.
	/// Object keys go through here too, so a key never reaches the terminal
	/// unescaped.
	fn string(&mut self, s: &str, original_len: usize) {
		let limit = self.limits.string_bytes;
		let cut = prefix(s, limit);
		if original_len > cut.len() {
			self.push(&format!("{cut:?}…(+{} bytes)", original_len - cut.len()));
		} else {
			self.push(&format!("{s:?}"));
		}
	}
	/// `items` holds at most `length` values of a container of `total`.
	fn items(
		&mut self,
		open: &str,
		close: &str,
		items: &[Value],
		total: usize,
		depth: usize,
		tuple: bool,
	) {
		self.push(open);
		for (i, item) in items.iter().enumerate() {
			if i > 0 {
				self.push(", ");
			}
			self.value(item, depth + 1);
			if self.full() {
				return;
			}
		}
		if total > items.len() {
			self.push(&format!(", …(+{} more)", total - items.len()));
		}
		if tuple && total == 1 {
			self.push(",");
		}
		self.push(close);
	}
	/// `entries` holds at most `length` entries of a container of `total`.
	fn object(
		&mut self,
		entries: &[(String, usize, Value)],
		total: usize,
		depth: usize,
		quote_keys: bool,
	) {
		self.push("{");
		for (i, (key, key_len, item)) in entries.iter().enumerate() {
			if i > 0 {
				self.push(", ");
			}
			if quote_keys {
				self.string(key, *key_len);
			} else {
				self.push(key);
			}
			self.push(": ");
			self.value(item, depth + 1);
			if self.full() {
				return;
			}
		}
		if total > entries.len() {
			self.push(&format!(", …(+{} more)", total - entries.len()));
		}
		self.push("}");
	}
}
/// Text made safe to print on a terminal: control characters are escaped,
/// escape included, while newlines and tabs are kept so source stays
/// readable. Stored text a command prints goes through here; values go
/// through the renderer, which quotes and escapes them.
///
/// Escape into `out`, stopping once `budget` bytes have been written, and
/// return how many bytes of `text` were consumed. Nothing beyond the budget
/// is examined or copied, so escaping a huge source for a small budget is
/// work proportional to the budget, not to the source.
pub fn terminal_safe_into(text: &str, budget: usize, out: &mut String) -> usize {
	let mut consumed = 0;
	let mut written = 0;
	let mut buffer = [0u8; 4];
	let mut escape = String::new();
	for c in text.chars() {
		let piece: &str = match c {
			'\n' | '\t' => c.encode_utf8(&mut buffer),
			c if c.is_control() => {
				escape.clear();
				use std::fmt::Write;
				let _ = write!(escape, "\\u{{{:x}}}", c as u32);
				escape.as_str()
			}
			c => c.encode_utf8(&mut buffer),
		};
		if written + piece.len() > budget {
			break;
		}
		out.push_str(piece);
		written += piece.len();
		consumed += c.len_utf8();
	}
	consumed
}

/// The last path component of an item, `Boxed` for `::Boxed`, and `Vec` for
/// `::std::vec::Vec`. Shared with the inspection commands so a type reads
/// the same wherever it is shown.
pub fn type_name(item: String) -> String {
	item.rsplit("::").next().unwrap_or(&item).to_owned()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::session::Session;
	use rune::Context;

	fn context() -> Context {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		context
	}
	fn eval(session: &mut Session, context: &Context, input: &str) -> String {
		session.set_budget(usize::MAX);
		let value = session.eval(context, input).unwrap();
		render(&value, Some(session), &Limits::default())
	}

	#[test]
	fn long_vector_is_cut_with_the_omitted_count() {
		let context = context();
		let mut session = Session::new();
		let out = eval(
			&mut session,
			&context,
			"let v = []; for i in 0..10000 { v.push(i) } v",
		);
		assert!(out.starts_with("[0, 1, 2, "), "{out}");
		assert!(out.ends_with("63, …(+9936 more)]"), "{out}");
	}

	#[test]
	fn deep_object_is_cut_at_the_depth_with_the_field_count() {
		let context = context();
		let mut session = Session::new();
		let out = eval(
			&mut session,
			&context,
			"let o = #{leaf: 1, other: 2}; for i in 0..20 { o = #{inner: o} } o",
		);
		assert_eq!(out.matches("\"inner\": ").count(), 8, "{out}");
		assert!(out.contains("{…(1 fields)}"), "{out}");
	}

	#[test]
	fn huge_string_is_cut_by_bytes() {
		let context = context();
		let mut session = Session::new();
		let out = eval(
			&mut session,
			&context,
			"let s = String::new(); let chunk = \"0123456789abcdef\"; for i in 0..655360 { s.push_str(chunk) } s",
		);
		assert!(
			out.ends_with("…(+10481664 bytes)"),
			"{}",
			&out[out.len() - 40..]
		);
		assert!(out.len() < 4200, "{}", out.len());
	}

	#[test]
	fn total_budget_truncates_the_whole_rendering_honestly() {
		let context = context();
		let mut session = Session::new();
		// 60 strings of 1000 bytes: each within the string limit, together over
		// the total budget. The count of what was not rendered is not known
		// without rendering it, so the marker names the budget, not a count.
		let out = eval(
			&mut session,
			&context,
			"let s = String::new(); for i in 0..1000 { s.push('x') } let v = []; for i in 0..60 { v.push(s) } v",
		);
		assert!(
			out.ends_with("…(output truncated at 16384 bytes)"),
			"{:?}",
			out.chars().rev().take(80).collect::<String>()
		);
		assert!(out.len() < 16384 + 64, "{}", out.len());
	}

	#[test]
	fn a_value_containing_itself_prints_a_cycle_marker() {
		let context = context();
		let mut session = Session::new();
		let out = eval(
			&mut session,
			&context,
			"let v = [1]; v.push(v); v.push([v]); v",
		);
		assert_eq!(out, "[1, <cycle>, [<cycle>]]");
	}

	#[test]
	fn unsupported_types_print_opaque_markers_with_the_type_name() {
		let context = context();
		let mut session = Session::new();
		let out = eval(
			&mut session,
			&context,
			"(0..3, |a| a, 'c', 2.5, -7, true, None, Some(\"s\"), (), b\"ab\")",
		);
		assert_eq!(
			out,
			"(<::std::ops::Range>, <function>, 'c', 2.5, -7, true, None, Some(\"s\"), (), b\"2 bytes\")"
		);
	}

	#[test]
	fn structs_print_by_declared_fields_and_enums_by_variant() {
		let context = context();
		let mut session = Session::new();
		// Rune 0.14.1 assigns struct literal values by position, not by name:
		// `struct P { y, x }` with `P { x: 1, y: 2 }` gives `p.x == 2`, and Rune's
		// own debug output agrees. The formatter reads slots by declared name,
		// which matches field access; the literal here keeps declaration order.
		session
			.eval(&context, "struct P { y, x } enum E { A, B(v), C { w } }")
			.unwrap();
		let out = eval(
			&mut session,
			&context,
			"(P { y: [2], x: 1 }, E::A, E::B(3), E::C { w: 4 })",
		);
		assert_eq!(out, "(P {y: [2], x: 1}, A, B(3), C {0: 4})");
	}

	/// A host type whose debug protocol records that it ran.
	#[derive(rune::Any)]
	#[rune(item = ::loud)]
	struct Loud {
		n: i64,
	}
	static PROTOCOL_RAN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
	impl Loud {
		#[rune::function(protocol = DEBUG_FMT)]
		fn debug_fmt(&self, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
			use rune::alloc::fmt::TryWrite;
			PROTOCOL_RAN.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
			rune::vm_write!(f, "Loud!{}", self.n)
		}
	}

	#[test]
	fn rendering_runs_no_script_code_even_when_a_debug_protocol_is_defined() {
		// In Rune 0.14.1 a script cannot define a debug protocol on its own
		// struct (an `impl P { fn debug_fmt }` is never consulted), so the live
		// protocol comes from a host type. Rendering must not invoke it: the
		// formatter holds no VM and cannot call any protocol.
		let mut context = context();
		let mut module = rune::Module::with_crate("loud").unwrap();
		module.ty::<Loud>().unwrap();
		module.function_meta(Loud::debug_fmt).unwrap();
		context.install(module).unwrap();
		let value = rune::to_value(Loud { n: 1 }).unwrap();
		let out = render(&value, None, &Limits::default());
		assert_eq!(out, "<::loud::Loud>");
		assert_eq!(
			PROTOCOL_RAN.load(std::sync::atomic::Ordering::SeqCst),
			0,
			"the debug protocol ran during rendering"
		);
		// Control: the protocol is live when the VM formats the value.
		let text: String = rune::from_value(
			crate::call(&context, "pub fn main(l) { format!(\"{:?}\", l) }", value).unwrap(),
		)
		.unwrap();
		assert_eq!(text, "Loud!1");
		assert_eq!(PROTOCOL_RAN.load(std::sync::atomic::Ordering::SeqCst), 1);
	}
}

#[cfg(test)]
mod boundary_tests {
	use super::*;
	use crate::session::Session;

	fn context() -> rune::Context {
		let mut context = rune::Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		context
	}

	#[test]
	fn object_keys_are_quoted_and_escaped() {
		let context = context();
		let mut session = Session::new();
		let value = session
			.eval(&context, "#{\"\\u{1b}[2J\": 1, \"plain\": 2}")
			.unwrap();
		let out = render(&value, Some(&session), &Limits::default());
		assert_eq!(out, "{\"\\u{1b}[2J\": 1, \"plain\": 2}");
		assert!(!out.contains('\x1b'));
	}

	#[test]
	fn wrapper_chains_stop_at_the_depth() {
		let context = context();
		let mut session = Session::new();
		let value = session
			.eval(
				&context,
				"let v = Some(1); for i in 0..100 { v = Some(v) } v",
			)
			.unwrap();
		let out = render(&value, Some(&session), &Limits::default());
		assert_eq!(out.matches("Some(").count(), 9, "{out}");
		assert!(out.contains("…"), "{out}");
	}

	#[test]
	fn limits_bound_the_work_not_only_the_output() {
		// A million-element vector: only 64 items are copied.
		let context = context();
		let mut session = Session::new();
		session.set_budget(usize::MAX);
		let value = session
			.eval(&context, "let v = []; for i in 0..1000000 { v.push(i) } v")
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session), &Limits::default());
		assert!(out.ends_with("63, …(+999936 more)]"), "{out}");
		assert_eq!(work.items_copied, 64);
		// A hundred-thousand-entry object: 64 entries examined, no more.
		let value = session
			.eval(
				&context,
				"let o = #{}; for i in 0..100000 { o[`k${i}`] = i } o",
			)
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session), &Limits::default());
		assert!(out.ends_with(", …(+99936 more)}"), "{out}");
		assert_eq!(work.entries_examined, 64);
		// A ten-megabyte key: at most the string limit is copied; the omitted
		// count comes from the key's byte length, not from the copy.
		let value = session
			.eval(
				&context,
				"let k = String::new(); for i in 0..655360 { k.push_str(\"0123456789abcdef\") } let o = #{}; o[k] = 1; o",
			)
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session), &Limits::default());
		assert!(out.ends_with("\"…(+10481664 bytes): 1}"), "{}", out.len());
		assert_eq!(work.key_bytes_copied, 4096);
		assert!(out.len() < 4200, "{}", out.len());
		// Multibyte boundaries: a key of 4,096 ASCII bytes then a four-byte
		// emoji and a tail is cut at 4,096 and reports the 8 omitted bytes; a
		// key of 4,095 ASCII bytes then a two-byte character is cut at 4,095,
		// short of the limit, and still reports its 2 omitted bytes.
		let value = session
			.eval(
				&context,
				"let a = String::new(); for i in 0..4096 { a.push('x') } let b = String::new(); for i in 0..4095 { b.push('x') } let o = #{}; o[a + \"\u{1F600}tail\"] = 1; o[b + \"\u{e9}\"] = 2; o",
			)
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session), &Limits::default());
		assert!(
			out.contains("\"…(+8 bytes): 1"),
			"{}",
			&out[out.len().saturating_sub(120)..]
		);
		assert!(
			out.contains("\"…(+2 bytes): 2"),
			"{}",
			&out[out.len().saturating_sub(120)..]
		);
		assert_eq!(work.key_bytes_copied, 4096 + 4095);
		assert!(!out.contains('\u{1F600}') && !out.contains('\u{e9}'));
	}
}
