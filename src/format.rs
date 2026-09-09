//! Bounded rendering of values for the session. The formatter walks Rune's
//! value model directly and never invokes a formatting protocol through the
//! VM, so rendering a value runs no script code. Every limit is applied
//! before anything is copied or descended into, so the work done is bounded
//! by the output allowed, not by the size of the value.
use crate::declared::Fields;
use rune::runtime::{Function, Object, OwnedTuple, TypeValue, Value, Vec as RuneVec};

/// The deepest a value is opened where the output is the product rather than
/// a preview. Measured: the renderer recurses, and with the limits lifted a
/// debug build renders a value nested 2048 deep and dies by 3072, so this
/// leaves an eightfold margin under what works and is twice serde_json's own
/// default parse limit. A value that reaches it is pathological, not large.
/// The JSON walk shares this number rather than having one of its own.
pub const MAX_DEPTH: usize = 256;

/// What to do about a value too deep to open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OnExcess {
	/// Show a marker and carry on. A prompt is a preview, and a marked
	/// preview is honest about itself.
	Elide,
	/// Report a failure. At a shell entry point the value is the output, and
	/// a caller reading 64 of 4000 items has been told nothing true.
	Refuse,
}

#[derive(Clone, Debug)]
pub struct Limits {
	pub depth: usize,
	pub length: usize,
	pub string_bytes: usize,
	pub total_bytes: usize,
	pub on_excess: OnExcess,
}
impl Limits {
	/// What a person reading a prompt gets: bounded, and marked where it was
	/// cut. Record 0005's ceiling is what this protects.
	pub fn preview() -> Self {
		Self {
			depth: 8,
			length: 64,
			string_bytes: 4096,
			total_bytes: 16 * 1024,
			on_excess: OnExcess::Elide,
		}
	}
	/// What a shell entry point gets: the whole value, or a failure. No
	/// volume cap at all, and the depth bound is the recursion bound rather
	/// than a display choice, so the only way to exceed it is to be
	/// pathological.
	pub fn complete() -> Self {
		Self {
			depth: MAX_DEPTH,
			length: usize::MAX,
			string_bytes: usize::MAX,
			total_bytes: usize::MAX,
			on_excess: OnExcess::Refuse,
		}
	}
}
impl Default for Limits {
	fn default() -> Self {
		Self::preview()
	}
}

/// What a returned error looks like, from either entry point.
///
/// One policy in one place. A string prints bare — without quotes and without
/// a wrapper, as record 0018 decided — and anything else goes through the
/// bounded renderer. Both get the same budget, the preview's string budget,
/// because an error is a report of a failure and a report that fills a
/// terminal is not one: record 0019 decision 4. Escaping stops at the budget,
/// so the work is bounded by what is printed rather than by the size of what
/// was returned.
pub fn error_text(value: &Value, fields: Option<&Fields>) -> String {
	let limits = Limits::preview();
	match value.borrow_string_ref() {
		Ok(text) => {
			let mut out = String::new();
			let consumed = terminal_safe_into(&text, limits.string_bytes, &mut out);
			if consumed < text.len() {
				out.push_str(&format!("…(+{} bytes)", text.len() - consumed));
			}
			out
		}
		Err(_) => render(value, fields, &limits),
	}
}

/// Render a value whole, or say why it could not be.
///
/// The text is built before anything is printed, so a caller can put a
/// failure on standard error without having written half a value to standard
/// output. This is a claim about rendering, not about the stream: a write
/// that fails part-way is the caller's to report.
///
/// A cycle and an opaque value are **not** failures. `<cycle>`, `<function>`
/// and the like are what those values look like, and a value containing one
/// is rendered whole.
pub fn render_complete(value: &Value, fields: Option<&Fields>) -> Result<String, String> {
	let limits = Limits::complete();
	let mut r = Renderer {
		out: String::new(),
		fields,
		limits: &limits,
		path: Vec::new(),
		truncated: false,
		work: Work::default(),
		failure: None,
	};
	r.value(value, 0);
	match r.failure {
		Some(reason) => Err(reason),
		None => Ok(r.out),
	}
}

/// Render a value. `fields` supplies candidate field names for structs whose
/// declarations rnx compiled; each is verified against the value before use.
pub fn render(value: &Value, fields: Option<&Fields>, limits: &Limits) -> String {
	render_with_work(value, fields, limits).0
}
/// Render, and report the work done, for tests that bound it.
pub fn render_with_work(value: &Value, fields: Option<&Fields>, limits: &Limits) -> (String, Work) {
	let mut r = Renderer {
		out: String::new(),
		fields,
		limits,
		path: Vec::new(),
		truncated: false,
		work: Work::default(),
		failure: None,
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
	fields: Option<&'a Fields>,
	limits: &'a Limits,
	/// Addresses of shared allocations on the current render path.
	path: Vec<usize>,
	truncated: bool,
	work: Work,
	/// Why the value could not be rendered whole, in `Refuse` mode. The first
	/// reason is kept: it names the shallowest place the value outran the bound.
	failure: Option<String>,
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
		if self.truncated || self.out.len().saturating_add(s.len()) > self.limits.total_bytes {
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
			self.excess(cut);
			return false;
		}
		self.path.push(address);
		true
	}
	/// A value too deep to open: a marked preview where that is what the
	/// output is for, and a failure where the output is the product. The
	/// first reason is kept, because it names the shallowest place the value
	/// outran the bound.
	fn excess(&mut self, cut: &str) {
		match self.limits.on_excess {
			OnExcess::Elide => self.push(cut),
			OnExcess::Refuse => {
				if self.failure.is_none() {
					self.failure = Some(format!(
						"the value is nested deeper than {} levels",
						self.limits.depth
					));
				}
			}
		}
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
			self.excess("…");
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
			// Rune 0.14.1 exposes no name or arity on a function value, nor does
			// 0.14.2: `runtime/function.rs` is byte-identical between them.
			self.push("<function>");
			return;
		}
		if let Ok(b) = value.borrow_ref::<rune::runtime::Bytes>() {
			let bytes = b.as_slice().to_vec();
			drop(b);
			self.bytes(&bytes);
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
				// A candidate is used only once the value has answered to
				// every name in it and holds exactly that many fields, so the
				// names describe this shape whichever unit built it. Where
				// nothing fits, the names are unknown and say so: positions
				// printed in their place read as field names and are not.
				let fitting = self
					.fields
					.and_then(|known| known.fitting(&name, total, |field| s.get(field).is_some()))
					.map(|fields| fields.to_vec());
				match fitting {
					Some(fields) => {
						let entries: Vec<(String, usize, Value)> = fields
							.iter()
							.take(self.limits.length)
							.filter_map(|f| {
								s.get(f.as_str()).map(|v| (f.clone(), f.len(), v.clone()))
							})
							.collect();
						drop(s);
						self.object(&entries, total, depth, false);
					}
					None => {
						let values: Vec<Value> =
							s.data().iter().take(self.limits.length).cloned().collect();
						drop(s);
						self.items("{<names unknown> ", "}", &values, total, depth, false);
					}
				}
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
	/// A byte string, as its contents. Bytes are data a script means to look
	/// at — `b"abc"` and `b"xyz"` are two different values, and printing both
	/// as their length told a reader nothing. A byte that reads as itself
	/// does; every other is `\xNN`, so the text is unambiguous and safe to
	/// print whatever the bytes are, valid UTF-8 or not.
	///
	/// Cut at the string budget like any other string, with the number of
	/// bytes left out, so a prompt previews and a shell entry point does not.
	fn bytes(&mut self, bytes: &[u8]) {
		let shown = bytes.len().min(self.limits.string_bytes);
		let mut out = String::with_capacity(shown + 3);
		out.push_str("b\"");
		for byte in &bytes[..shown] {
			match byte {
				b'"' => out.push_str("\\\""),
				b'\\' => out.push_str("\\\\"),
				0x20..=0x7e => out.push(*byte as char),
				_ => out.push_str(&format!("\\x{byte:02x}")),
			}
		}
		out.push('"');
		if shown < bytes.len() {
			out.push_str(&format!("…(+{} bytes)", bytes.len() - shown));
		}
		self.push(&out);
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
			// A newline is kept: it cannot put the cursor anywhere a reader
			// does not expect, and stored text reads better with it. A tab is
			// not, because a caret is placed under what was printed and a tab
			// would make that a question about the terminal's tab stops.
			'\n' => c.encode_utf8(&mut buffer),
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

/// Text made safe to print, whole. Everything rnx writes on its own behalf
/// goes through here: a diagnostic message, a file path, a source excerpt.
/// Values go through the renderer, which quotes and escapes them.
pub fn terminal_safe(text: &str) -> String {
	let mut out = String::new();
	terminal_safe_into(text, usize::MAX, &mut out);
	out
}

/// How many terminal columns a piece of already-escaped text occupies. A
/// caret is placed with this rather than by counting characters, because an
/// East Asian wide character occupies two columns and a combining mark none.
/// The one place the crate answers this question.
pub fn display_width(text: &str) -> usize {
	unicode_width::UnicodeWidthStr::width(text)
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
	fn eval(session: &mut Session, input: &str) -> String {
		session.set_budget(usize::MAX);
		let value = session.eval(input).unwrap();
		render(&value, Some(&session.fields()), &Limits::default())
	}

	#[test]
	fn a_candidate_that_does_not_fit_the_value_is_not_used() {
		// The heart of decision 5: a name is verified, never assumed. A table
		// holding the wrong field list for this short name must not be
		// believed, and the fields say they are unknown rather than printing
		// positions that read as names.
		let mut session = Session::new(context()).unwrap();
		session.eval("struct P { code }").unwrap();
		let value = session.eval("P { code: 7 }").unwrap();

		let mut wrong = Fields::default();
		wrong.add("P", vec!["elsewhere".to_owned()]);
		assert_eq!(
			render(&value, Some(&wrong), &Limits::preview()),
			"P {<names unknown> 7}",
			"a candidate with the wrong name was used"
		);

		let mut short = Fields::default();
		short.add("P", vec!["code".to_owned(), "extra".to_owned()]);
		assert_eq!(
			render(&value, Some(&short), &Limits::preview()),
			"P {<names unknown> 7}",
			"a candidate of the wrong arity was used"
		);

		// And with nothing known at all, which is what `run` had before this
		// cut and what any host value has now.
		assert_eq!(
			render(&value, None, &Limits::preview()),
			"P {<names unknown> 7}"
		);

		// The right candidate is used, so the test above is not passing for
		// want of a working path.
		assert_eq!(
			render(&value, Some(&session.fields()), &Limits::preview()),
			"P {code: 7}"
		);
	}

	#[test]
	fn a_retained_value_keeps_its_field_names_across_later_inputs() {
		// A value made by an earlier input renders by the declaration it was
		// made with, after other inputs have come and gone. Records 0006 and
		// 0007 keep that unit alive weakly; what this asserts is that the
		// names still resolve.
		let mut session = Session::new(context()).unwrap();
		session.eval("struct P { code }").unwrap();
		let value = session.eval("P { code: 7 }").unwrap();
		session.eval("let unrelated = 1;").unwrap();
		session.eval("fn helper() { 2 }").unwrap();
		session.eval("struct Other { a, b }").unwrap();
		assert_eq!(
			render(&value, Some(&session.fields()), &Limits::preview()),
			"P {code: 7}",
			"a retained value lost its field names"
		);
	}

	#[test]
	fn byte_strings_show_their_contents() {
		// `b"abc"` and `b"xyz"` are two different values. Printing both as
		// their length said nothing about either, and exited 0 while doing it.
		let mut session = Session::new(context()).unwrap();
		let abc = session.eval("b\"abc\"").unwrap();
		let xyz = session.eval("b\"xyz\"").unwrap();
		let complete = |v: &Value| render_complete(v, None).unwrap();
		assert_eq!(complete(&abc), "b\"abc\"");
		assert_eq!(complete(&xyz), "b\"xyz\"");
		assert_ne!(
			complete(&abc),
			complete(&xyz),
			"two byte strings of one length are not one value"
		);

		// Bytes that are not text at all, and the two delimiters that would
		// otherwise end the literal.
		let raw = session.eval("b\"\\xff\\xfe\\x00ok\"").unwrap();
		assert_eq!(complete(&raw), "b\"\\xff\\xfe\\x00ok\"");
		let quoted = session.eval("b\"a\\\"b\\\\c\"").unwrap();
		assert_eq!(complete(&quoted), "b\"a\\\"b\\\\c\"");
	}

	#[test]
	fn a_large_byte_string_is_complete_at_an_entry_point_and_previewed_at_a_prompt() {
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let value = session
			.eval("let v = []; for i in 0..9000 { v.push(65) } Bytes::from_vec(v)")
			.unwrap();
		let complete = render_complete(&value, None).unwrap();
		assert_eq!(complete.matches('A').count(), 9_000, "bytes went missing");
		assert!(!complete.contains('…'), "a complete rendering elided");

		let preview = render(&value, None, &Limits::preview());
		assert!(
			preview.contains("…(+4904 bytes)"),
			"a preview should say what it left out: {}",
			&preview[preview.len().saturating_sub(40)..]
		);
		assert!(preview.len() < 4_200, "preview is {} bytes", preview.len());
	}

	#[test]
	fn an_error_is_bounded_by_the_same_budget_whatever_its_shape() {
		// A string error used to skip the budget entirely: 20,000 characters
		// printed 20,008 bytes while the same text inside an object printed
		// 4,130. One policy now, and the budget is the preview's string
		// budget, so the two cannot diverge again.
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		session
			.eval("let s = \"\"; for i in 0..2000 { s += \"0123456789\" } s")
			.unwrap();
		let bare = session.eval("Err(s)").unwrap();
		let wrapped = session.eval("Err(#{message: s})").unwrap();
		let budget = Limits::preview().string_bytes;

		let bare = error_text(&bare, None);
		let wrapped = error_text(&wrapped, None);
		assert!(
			bare.len() < budget + 64,
			"a string error is {} bytes",
			bare.len()
		);
		assert!(
			bare.contains("…(+15904 bytes)"),
			"the omitted count is missing"
		);
		assert!(
			wrapped.len() < budget + 64,
			"a wrapped error is {} bytes",
			wrapped.len()
		);

		// Bounded work, not merely bounded output: escaping stops at the
		// budget, so nothing past it is examined or copied. Every character
		// here escapes to six bytes, which a length check alone would miss.
		let mut out = String::new();
		let escapes = "\u{1b}".repeat(200_000);
		let consumed = terminal_safe_into(&escapes, budget, &mut out);
		assert!(consumed <= budget, "{consumed} bytes of input were read");
		assert!(out.len() <= budget, "{} bytes were written", out.len());
	}

	#[test]
	fn a_preview_elides_and_a_complete_rendering_does_not() {
		// The split decision 1 of record 0019 makes, asserted where both limit
		// sets are defined so the two halves cannot drift apart: the same value
		// previews short for a person at a prompt and renders whole where the
		// output is the product a caller reads.
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let value = session
			.eval("let v = []; for i in 0..4000 { v.push(1) } v")
			.unwrap();
		let preview = render(&value, Some(&session.fields()), &Limits::preview());
		assert!(preview.contains("…(+3936 more)"), "{preview}");
		assert!(
			preview.len() < 1_000,
			"a preview should stay short, got {} bytes",
			preview.len()
		);
		let complete = render_complete(&value, Some(&session.fields())).unwrap();
		assert_eq!(complete.matches('1').count(), 4_000, "items are missing");
		assert!(!complete.contains('…'), "a complete rendering elided");
	}

	#[test]
	fn a_cycle_and_an_opaque_value_are_complete_renderings() {
		// Not failures: `<cycle>` and `<function>` are what those values look
		// like, so a value containing one renders whole and reports nothing.
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let value = session.eval("let v = [1]; v.push(v); v").unwrap();
		assert_eq!(
			render_complete(&value, Some(&session.fields())).unwrap(),
			"[1, <cycle>]"
		);
		let value = session.eval("[|| 1]").unwrap();
		assert_eq!(
			render_complete(&value, Some(&session.fields())).unwrap(),
			"[<function>]"
		);
	}

	#[test]
	fn repeated_shared_data_is_not_a_cycle() {
		// One allocation referenced twice is repeated data, and a renderer
		// that tracked values seen rather than the active path would call it a
		// cycle. The guard pushes on descent and pops on return.
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let value = session.eval("let a = [1]; [a, a]").unwrap();
		assert_eq!(
			render_complete(&value, Some(&session.fields())).unwrap(),
			"[[1], [1]]"
		);
	}
	#[test]
	fn long_vector_is_cut_with_the_omitted_count() {
		let mut session = Session::new(context()).unwrap();
		let out = eval(
			&mut session,
			"let v = []; for i in 0..10000 { v.push(i) } v",
		);
		assert!(out.starts_with("[0, 1, 2, "), "{out}");
		assert!(out.ends_with("63, …(+9936 more)]"), "{out}");
	}

	#[test]
	fn deep_object_is_cut_at_the_depth_with_the_field_count() {
		let mut session = Session::new(context()).unwrap();
		let out = eval(
			&mut session,
			"let o = #{leaf: 1, other: 2}; for i in 0..20 { o = #{inner: o} } o",
		);
		assert_eq!(out.matches("\"inner\": ").count(), 8, "{out}");
		assert!(out.contains("{…(1 fields)}"), "{out}");
	}

	#[test]
	fn huge_string_is_cut_by_bytes() {
		let mut session = Session::new(context()).unwrap();
		let out = eval(
			&mut session,
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
		let mut session = Session::new(context()).unwrap();
		// 60 strings of 1000 bytes: each within the string limit, together over
		// the total budget. The count of what was not rendered is not known
		// without rendering it, so the marker names the budget, not a count.
		let out = eval(
			&mut session,
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
		let mut session = Session::new(context()).unwrap();
		let out = eval(&mut session, "let v = [1]; v.push(v); v.push([v]); v");
		assert_eq!(out, "[1, <cycle>, [<cycle>]]");
	}

	#[test]
	fn unsupported_types_print_opaque_markers_with_the_type_name() {
		let mut session = Session::new(context()).unwrap();
		let out = eval(
			&mut session,
			"(0..3, |a| a, 'c', 2.5, -7, true, None, Some(\"s\"), (), b\"ab\")",
		);
		assert_eq!(
			out,
			// Bytes are not in this company any more: they are data a script
			// means to look at, and record 0019 renders their contents. A
			// range and a function have no contents to show.
			"(<::std::ops::Range>, <function>, 'c', 2.5, -7, true, None, Some(\"s\"), (), b\"ab\")"
		);
	}

	#[test]
	fn structs_print_by_declared_fields_and_enums_by_variant() {
		let mut session = Session::new(context()).unwrap();
		// Rune 0.14.1 assigns struct literal values by position, not by name, and
		// 0.14.2 still does — measured by the matrix below rather than by reading
		// the compiler, which did change in that release:
		// `struct P { y, x }` with `P { x: 1, y: 2 }` gives `p.x == 2`, and Rune's
		// own debug output agrees. The formatter reads slots by declared name,
		// which matches field access; the literal here keeps declaration order.
		session
			.eval("struct P { y, x } enum E { A, B(v), C { w } }")
			.unwrap();
		let out = eval(
			&mut session,
			"(P { y: [2], x: 1 }, E::A, E::B(3), E::C { w: 4 })",
		);
		// Record 0019 decision 5: a struct variant's field names come from its
		// declaration like any other, verified against the value. This used to
		// print `C {0: 4}`, a position dressed as a field name.
		assert_eq!(out, "(P {y: [2], x: 1}, A, B(3), C {w: 4})");
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
		// In Rune 0.14.1 a script cannot define a debug protocol on its own, and
		// nor in 0.14.2, whose `runtime/protocol.rs` is byte-identical:
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
		let mut session = Session::new(context()).unwrap();
		let value = session.eval("#{\"\\u{1b}[2J\": 1, \"plain\": 2}").unwrap();
		let out = render(&value, Some(&session.fields()), &Limits::default());
		assert_eq!(out, "{\"\\u{1b}[2J\": 1, \"plain\": 2}");
		assert!(!out.contains('\x1b'));
	}

	#[test]
	fn wrapper_chains_stop_at_the_depth() {
		let mut session = Session::new(context()).unwrap();
		let value = session
			.eval("let v = Some(1); for i in 0..100 { v = Some(v) } v")
			.unwrap();
		let out = render(&value, Some(&session.fields()), &Limits::default());
		assert_eq!(out.matches("Some(").count(), 9, "{out}");
		assert!(out.contains("…"), "{out}");
	}

	#[test]
	fn limits_bound_the_work_not_only_the_output() {
		// A million-element vector: only 64 items are copied.
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let value = session
			.eval("let v = []; for i in 0..1000000 { v.push(i) } v")
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session.fields()), &Limits::default());
		assert!(out.ends_with("63, …(+999936 more)]"), "{out}");
		assert_eq!(work.items_copied, 64);
		// A hundred-thousand-entry object: 64 entries examined, no more.
		let value = session
			.eval("let o = #{}; for i in 0..100000 { o[`k${i}`] = i } o")
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session.fields()), &Limits::default());
		assert!(out.ends_with(", …(+99936 more)}"), "{out}");
		assert_eq!(work.entries_examined, 64);
		// A ten-megabyte key: at most the string limit is copied; the omitted
		// count comes from the key's byte length, not from the copy.
		let value = session
			.eval(
				"let k = String::new(); for i in 0..655360 { k.push_str(\"0123456789abcdef\") } let o = #{}; o[k] = 1; o",
			)
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session.fields()), &Limits::default());
		assert!(out.ends_with("\"…(+10481664 bytes): 1}"), "{}", out.len());
		assert_eq!(work.key_bytes_copied, 4096);
		assert!(out.len() < 4200, "{}", out.len());
		// Multibyte boundaries: a key of 4,096 ASCII bytes then a four-byte
		// emoji and a tail is cut at 4,096 and reports the 8 omitted bytes; a
		// key of 4,095 ASCII bytes then a two-byte character is cut at 4,095,
		// short of the limit, and still reports its 2 omitted bytes.
		let value = session
			.eval(
				"let a = String::new(); for i in 0..4096 { a.push('x') } let b = String::new(); for i in 0..4095 { b.push('x') } let o = #{}; o[a + \"\u{1F600}tail\"] = 1; o[b + \"\u{e9}\"] = 2; o",
			)
			.unwrap();
		let (out, work) = render_with_work(&value, Some(&session.fields()), &Limits::default());
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
