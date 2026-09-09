//! The persistent session: an AST-based adapter over upstream Rune.
//!
//! Each input is compiled into a fresh unit that receives the published
//! bindings, runs once, and returns the new bindings. Declarations are
//! retained as source and recompiled into every later unit. The session owns
//! a source map per unit so diagnostics point at the input a person typed.
use super::Result;
use rune::ast::Spanned;
use rune::runtime::{GeneratorState, Unit, VmError, budget};
use rune::{Context, Source, SourceId, Sources, Vm, ast, runtime::Value};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};

/// Instructions per slice between interrupt checks.
pub const SLICE: usize = 10_000;
/// Instructions per input before the session halts it: a safety net of about
/// ten seconds of pure Rune execution, not the way to stop an input; Ctrl-C
/// is, at every slice boundary.
pub const BUDGET: usize = 2_000_000_000;
/// The ceiling on tracked live allocation request bytes, unless configured
/// otherwise. Generous enough that ordinary work never meets it, small
/// enough that a runaway session stops before the machine notices.
pub const DEFAULT_CEILING: usize = 512 * 1024 * 1024;
/// Longest accepted input.
const INPUT_CAP: usize = 32 * 1024;

/// A position in an input a person typed: input number (1-based), line and
/// column (1-based, in characters).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Origin {
	pub input: usize,
	pub line: usize,
	pub column: usize,
	/// The text of that line, for the diagnostic.
	pub text: String,
}

#[derive(Debug)]
pub enum Failure {
	/// The input was refused by the adapter before compilation.
	Refused(String),
	/// A compile error. The origin is `None` only when the position could not
	/// be mapped to user text; the message says so.
	Compile {
		message: String,
		origin: Option<Origin>,
	},
	/// A runtime error, positioned at the instruction that raised it.
	Runtime {
		message: String,
		origin: Option<Origin>,
	},
	/// Ctrl-C was observed at a slice boundary.
	Interrupted,
	/// The instruction budget for one input ran out.
	Budget(usize),
	/// Tracked live allocation request bytes are at or above the ceiling;
	/// only `:reset` and inspection work until a reset samples below it.
	OverCeiling { live: usize, ceiling: usize },
}
impl std::fmt::Display for Failure {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			Failure::Refused(m) => write!(f, "refused: {m}"),
			Failure::Compile { message, origin } => located(f, "error", message, origin),
			Failure::Runtime { message, origin } => located(f, "runtime error", message, origin),
			Failure::Interrupted => write!(f, "interrupted"),
			Failure::Budget(n) => write!(f, "halted: {n} instructions exceeded"),
			Failure::OverCeiling { live, ceiling } => write!(
				f,
				"tracked live allocation request bytes {live} are at or above the ceiling of {ceiling}; :reset to continue"
			),
		}
	}
}
impl std::error::Error for Failure {}
fn located(
	f: &mut std::fmt::Formatter<'_>,
	kind: &str,
	message: &str,
	origin: &Option<Origin>,
) -> std::fmt::Result {
	// Everything printed here is escaped: the message, which may quote a
	// script's own text, and the excerpt, which is the input as written.
	let message = crate::format::terminal_safe(message);
	match origin {
		Some(o) => {
			// The reported column is a position in the source, counted in
			// characters. The caret is placed separately, by the display width
			// of the escaped prefix actually printed, which is the same rule
			// the file runner uses.
			writeln!(
				f,
				"{kind} at input {}, line {}, column {}: {message}",
				o.input, o.line, o.column
			)?;
			writeln!(f, "  {}", crate::format::terminal_safe(&o.text))?;
			let prefix: String = o.text.chars().take(o.column.saturating_sub(1)).collect();
			let columns = crate::format::display_width(&crate::format::terminal_safe(&prefix));
			write!(f, "  {}^", " ".repeat(columns))
		}
		None => write!(
			f,
			"{kind} (position not in your input; :debug shows the generated source): {message}"
		),
	}
}

/// Whether an input is finished. Decided on the original text: parsing
/// stops at the input's end for an incomplete input, whatever the error's
/// span starts at (an unterminated string spans from its opening quote).
#[derive(Debug, PartialEq, Eq)]
pub enum Completeness {
	Complete,
	Incomplete,
}
/// The closed list of incomplete inputs, each recognised by the shape of the
/// first parse error on the original text, never by its message:
/// - an expected-token error whose actual token is end of input, and a bare
///   unexpected end of input: a zero-width span at the input's end;
/// - an unterminated string, template, or block comment: a span that reaches
///   the input's end and keeps reaching it when the input grows, because the
///   lexer consumes to the end of whatever it is given;
/// - an open bracket, brace, or parenthesis: the same expected-token error at
///   the end, since the original text has no wrapper to supply a closer.
/// A real token at the end, such as the `;` of `let x = ;`, keeps its span
/// when the input grows, so it is a diagnostic.
pub fn completeness(input: &str) -> Completeness {
	let Some(span) = first_error_span(input) else {
		return Completeness::Complete;
	};
	let end = span.range().end;
	if end < input.len() {
		return Completeness::Complete;
	}
	if span.range().is_empty() {
		return Completeness::Incomplete;
	}
	let grown = format!("{input}\n");
	match first_error_span(&grown) {
		Some(s) if s.range().start == span.range().start && s.range().end >= grown.len() => {
			Completeness::Incomplete
		}
		_ => Completeness::Complete,
	}
}
fn first_error_span(input: &str) -> Option<ast::Span> {
	let mut parser = rune::parse::Parser::new(input, SourceId::empty(), false);
	loop {
		match parser.is_eof() {
			Ok(true) => return None,
			Ok(false) => {}
			Err(e) => return Some(e.span()),
		}
		if let Err(e) = parser.parse::<ast::Stmt>() {
			return Some(e.span());
		}
	}
}

struct Declaration {
	is_type: bool,
	source: String,
	/// Input number the declaration was last entered in, and its offset there.
	input: usize,
	offset: usize,
	/// Field names for a struct, in declaration order; used by the formatter.
	fields: Vec<String>,
}

/// One contiguous piece of generated source with a known origin.
#[derive(Clone, Debug)]
struct Segment {
	start: usize,
	end: usize,
	input: usize,
	offset: usize,
}
#[derive(Clone, Debug, Default)]
pub struct SourceMap {
	segments: Vec<Segment>,
}
impl SourceMap {
	fn locate(&self, generated: usize) -> Option<(usize, usize)> {
		self.segments
			.iter()
			.find(|s| s.start <= generated && generated < s.end)
			.map(|s| (s.input, s.offset + (generated - s.start)))
	}
	fn bytes(&self) -> usize {
		self.segments.len() * std::mem::size_of::<Segment>()
	}
}

/// A generated source under construction, recording origins as it grows.
struct Generated {
	text: String,
	map: SourceMap,
}
impl Generated {
	fn new() -> Self {
		Self {
			text: String::new(),
			map: SourceMap::default(),
		}
	}
	fn raw(&mut self, s: &str) {
		self.text.push_str(s);
	}
	fn user(&mut self, s: &str, input: usize, offset: usize) {
		let start = self.text.len();
		self.text.push_str(s);
		self.map.segments.push(Segment {
			start,
			end: self.text.len(),
			input,
			offset,
		});
	}
}

/// One input's bookkeeping. The unit is held weakly, so it lives exactly as
/// long as something can still call into it; the map is small and goes when
/// the entry is pruned, because a map whose unit nothing can call can never
/// be consulted.
struct Retained {
	unit: Weak<Unit>,
	map: SourceMap,
}

pub struct Session {
	/// The compile-time context, owned, so an input cannot be compiled
	/// against one context and run against another, and so no caller can
	/// modify it between inputs. The runtime is built from it per input:
	/// sharing one is refused by Rune 0.14.1 and by 0.14.2, for the reason
	/// recorded in
	/// `plans/0006_what_a_retained_value_pins.md`.
	context: Context,
	declarations: BTreeMap<String, Declaration>,
	names: BTreeSet<String>,
	/// The published bindings, name to value, held on this side rather than
	/// inside a Rune object. Publishing a delta into a `BTreeMap` cannot
	/// fail, so an input either publishes all of its delta or none of it;
	/// every fallible step happens before anything is committed.
	state: BTreeMap<String, Value>,
	inputs: Vec<String>,
	units: Vec<Retained>,
	last_generated: String,
	ceiling: usize,
	/// Latched once a sample finds the figure at or above the ceiling. Only a
	/// fresh session, from `:reset`, can start unlatched, and its first sample
	/// latches it again if the figure is still there.
	over_ceiling: bool,
	budget: usize,
}
impl Session {
	pub fn new(context: Context) -> Result<Self> {
		Self::with_ceiling(context, DEFAULT_CEILING)
	}
	pub fn with_ceiling(context: Context, ceiling: usize) -> Result<Self> {
		Ok(Self {
			context,
			declarations: BTreeMap::new(),
			names: BTreeSet::new(),
			state: BTreeMap::new(),
			inputs: Vec::new(),
			units: Vec::new(),
			last_generated: String::new(),
			ceiling,
			over_ceiling: false,
			budget: BUDGET,
		})
	}
	/// The generated source of the most recent input, for `:debug`.
	pub fn last_generated(&self) -> &str {
		&self.last_generated
	}
	/// Bytes of the text and maps the session retains: the inputs, the
	/// declaration sources, and the source map of each entry. The units
	/// themselves are held weakly and are not counted here; the values a
	/// binding holds are not counted either. Record 0005's figure is the
	/// measured one, and this is a named component of it.
	pub fn retained_bytes(&self) -> usize {
		self.inputs.iter().map(String::len).sum::<usize>()
			+ self
				.declarations
				.values()
				.map(|d| d.source.len())
				.sum::<usize>()
			+ self.units.iter().map(|u| u.map.bytes()).sum::<usize>()
	}
	/// Instructions one input may spend before it is halted.
	pub fn set_budget(&mut self, budget: usize) {
		self.budget = budget;
	}
	/// Empty the session, keeping the compile-time context: a reset clears
	/// what the session holds, not the interpreter it runs on. The startup
	/// reference point is the process's, never re-recorded.
	pub fn reset(&mut self) {
		self.declarations.clear();
		self.names.clear();
		self.state.clear();
		self.inputs.clear();
		self.units.clear();
		self.last_generated.clear();
		self.over_ceiling = false;
	}
	pub fn ceiling(&self) -> usize {
		self.ceiling
	}
	/// Whether evaluation is refused because a sample found the figure at or
	/// above the ceiling.
	pub fn over_ceiling(&self) -> bool {
		self.over_ceiling
	}
	/// Take one sample of tracked live allocation request bytes and latch the
	/// refusal if it is at or above the ceiling. Returns the figure, or `None`
	/// in a build with accounting compiled out, where no ceiling is enforced.
	pub fn sample(&mut self) -> Option<usize> {
		let live = super::memory::live()?;
		if live >= self.ceiling {
			self.over_ceiling = true;
		}
		Some(live)
	}
	/// How many entries the session keeps. After pruning this is the number
	/// of units something can still call, plus the one just registered.
	pub fn retained_units(&self) -> usize {
		self.units.len()
	}
	/// How many of those entries still have a live unit, for the gate that
	/// units are released rather than merely held weakly.
	#[cfg(test)]
	pub fn live_units(&self) -> usize {
		self.units
			.iter()
			.filter(|entry| entry.unit.strong_count() > 0)
			.count()
	}
	/// How many bindings are published. The count of published names, which
	/// the session already keeps, so asking costs nothing.
	pub fn binding_count(&self) -> usize {
		self.names.len()
	}
	/// Visit published bindings in name order until `visit` returns false,
	/// and report how many were visited. Nothing is cloned and no binding
	/// past the caller's stopping point is looked up, so a caller that shows
	/// a few does work for a few. Reading a value is structural: nothing is
	/// evaluated.
	pub fn visit_bindings(&self, mut visit: impl FnMut(&str, &Value) -> bool) -> usize {
		let mut visited = 0;
		for (name, value) in &self.state {
			visited += 1;
			if !visit(name, value) {
				break;
			}
		}
		visited
	}
	/// One published binding's value, or `None` if the session has no such
	/// binding.
	pub fn binding(&self, name: &str) -> Option<Value> {
		self.state.get(name).cloned()
	}
	/// One retained declaration: its kind and the source last entered for it.
	pub fn declaration(&self, name: &str) -> Option<(&'static str, &str)> {
		let declaration = self.declarations.get(name)?;
		let kind = if !declaration.is_type {
			"function"
		} else if declaration.source.starts_with("struct") {
			"struct"
		} else {
			"enum"
		};
		Some((kind, declaration.source.as_str()))
	}
	/// Published binding names, for completion.
	pub fn binding_names(&self) -> Vec<String> {
		self.names.iter().cloned().collect()
	}
	/// Retained declaration names, for completion.
	pub fn declaration_names(&self) -> Vec<String> {
		self.declarations.keys().cloned().collect()
	}
	/// The session's declarations as field-name candidates for the renderer.
	/// Built fresh: a session holds few declarations, and a stale copy is a
	/// defect waiting to happen.
	pub fn fields(&self) -> crate::declared::Fields {
		let mut fields = crate::declared::Fields::default();
		for declaration in self.declarations.values() {
			if declaration.is_type {
				// Read from the declaration's own text by the one walker, so a
				// struct, a struct inside it, and an enum's variants are all
				// found the same way here as in a file.
				crate::declared::into_fields(&declaration.source, &mut fields);
			}
		}
		fields
	}

	pub fn eval(&mut self, input: &str) -> std::result::Result<Value, Failure> {
		if self.over_ceiling {
			return Err(Failure::OverCeiling {
				live: super::memory::live().unwrap_or(0),
				ceiling: self.ceiling,
			});
		}
		if input.len() > INPUT_CAP {
			return Err(Failure::Refused(format!("input exceeds {INPUT_CAP} bytes")));
		}
		super::host::clear_interrupt();
		self.inputs.push(input.to_owned());
		let number = self.inputs.len();
		let result = self.eval_inner(input, number);
		if result.is_err() {
			// A failed input publishes nothing, but its text stays so an origin
			// recorded against it (there is none) could never dangle.
		}
		result
	}

	fn eval_inner(&mut self, input: &str, number: usize) -> std::result::Result<Value, Failure> {
		// The wrapper is one byte of `{` before the input; wrapped offsets map
		// to input offsets by subtracting it.
		let wrapped = format!("{{{input}\n}}");
		let block: ast::Block = rune::parse::parse_all(&wrapped, SourceId::empty(), false)
			.map_err(|e| {
				self.compile_failure(
					e.to_string(),
					Some((number, e.span().range().start.saturating_sub(1))),
				)
			})?;
		let slice = |span: ast::Span| &wrapped[span.range()];
		let mut declarations: BTreeMap<String, Declaration> = self
			.declarations
			.iter()
			.map(|(k, d)| {
				(
					k.clone(),
					Declaration {
						is_type: d.is_type,
						source: d.source.clone(),
						input: d.input,
						offset: d.offset,
						fields: d.fields.clone(),
					},
				)
			})
			.collect();
		let mut names = self.names.clone();
		// Statements: (text, offset in input).
		let mut statements: Vec<(String, usize)> = Vec::new();
		let mut result: (String, usize) = ("()".to_owned(), 0);
		let mut result_is_user = false;
		for (index, statement) in block.statements.iter().enumerate() {
			// ItemStruct's derived span omits its closing delimiter in 0.14.1, and
			// in 0.14.2, whose `ast/item_struct.rs` is byte-identical.
			// Use parsed statement boundaries, never a textual brace scanner.
			let start = statement.span().range().start;
			let end = block
				.statements
				.get(index + 1)
				.map(|s| s.span().range().start)
				.unwrap_or(block.close.span().range().start);
			let statement_source = &wrapped[start..end];
			match statement {
				ast::Stmt::Item(item, _) => {
					let (name, is_type, fields) = match item {
						ast::Item::Fn(f) => (slice(f.name.span()), false, Vec::new()),
						ast::Item::Struct(s) => {
							(slice(s.ident.span()), true, struct_fields(s, &wrapped))
						}
						ast::Item::Enum(e) => (slice(e.name.span()), true, Vec::new()),
						_ => {
							return Err(self.refused_at(
								"a session accepts fn, struct, and enum declarations; modules, imports, macro declarations, and impl blocks work in files",
								number,
								start - 1,
							));
						}
					};
					valid_name(name).map_err(Failure::Refused)?;
					let trimmed = statement_source.trim_end();
					let leading = statement_source.len() - statement_source.trim_start().len();
					let source = trimmed.trim_start().to_owned();
					if let Some(old) = declarations.get(name) {
						if (old.is_type || is_type) && old.source != source {
							return Err(self.refused_at(
								&format!(
									"`{name}` is a type whose shape changed; :reset before redeclaring it"
								),
								number,
								start - 1,
							));
						}
					}
					declarations.insert(
						name.to_owned(),
						Declaration {
							is_type,
							source,
							input: number,
							offset: start + leading - 1,
							fields,
						},
					);
				}
				ast::Stmt::Local(local) => {
					bindings(&local.pat, &wrapped, &mut names).map_err(Failure::Refused)?;
					statements.push((statement_source.to_owned(), start - 1));
				}
				ast::Stmt::Expr(expr) if index + 1 == block.statements.len() => {
					result = (slice(expr.span()).to_owned(), expr.span().range().start - 1);
					result_is_user = true;
				}
				ast::Stmt::Semi(_) | ast::Stmt::Expr(_) => {
					let mut text = statement_source.to_owned();
					if matches!(statement, ast::Stmt::Expr(_)) {
						text.push(';');
					}
					statements.push((text, start - 1));
				}
				_ => return Err(self.refused_at("unsupported statement", number, start - 1)),
			}
		}
		let mut generated = Generated::new();
		for declaration in declarations.values() {
			generated.user(&declaration.source, declaration.input, declaration.offset);
			generated.raw("\n");
		}
		generated.raw("pub fn main(__rnx_state) {\n");
		// Restore only what this input may reference, so the prelude follows
		// the input rather than the session.
		let referenced = mentioned(input, &self.names);
		for name in &referenced {
			generated.raw(&format!("let {name} = __rnx_state[\"{name}\"];\n"));
		}
		let restored = self.restored(&referenced)?;
		for (text, offset) in &statements {
			generated.user(text, number, *offset);
			generated.raw("\n");
		}
		generated.raw("let __rnx_result = (");
		if result_is_user {
			generated.user(&result.0, number, result.1);
		} else {
			generated.raw(&result.0);
		}
		// Publish a delta: the names this input restored, which it may have
		// reassigned, and the names it declared. A name it never mentioned is
		// absent, and the session leaves its entry, and its identity, alone.
		let declared: Vec<&str> = names
			.iter()
			.filter(|name| !self.names.contains(*name))
			.map(String::as_str)
			.collect();
		let delta: BTreeSet<&str> = referenced.into_iter().chain(declared).collect();
		generated.raw(");\n(#{");
		generated.raw(&delta.into_iter().collect::<Vec<_>>().join(","));
		generated.raw("}, __rnx_result)\n}");
		self.last_generated = generated.text.clone();

		let unit = Arc::new(self.compile(&generated)?);
		// Entries whose unit nothing can call any more go before this input's
		// entry is registered, so the bookkeeping stays proportional to the
		// units still reachable rather than to the number of inputs.
		self.units.retain(|entry| entry.unit.strong_count() > 0);
		// The entry is registered before the unit runs: a failed or interrupted
		// input can still leave a closure over this unit reachable through a
		// shared handle, and an error inside it must map to this input.
		self.units.push(Retained {
			unit: Arc::downgrade(&unit),
			map: generated.map,
		});
		let output = self.execute(&unit, restored)?;
		let (delta, value): (Value, Value) =
			rune::from_value(output).map_err(|e| Failure::Runtime {
				message: e.to_string(),
				origin: None,
			})?;
		// Publish bindings and declarations only after a successful run. Values
		// are shared handles: mutations on a failed input remain visible.
		// The delta is merged entry by entry, so a binding this input never
		// mentioned keeps its value and its identity untouched.
		self.merge(delta)?;
		self.declarations = declarations;
		self.names = names;
		Ok(value)
	}

	/// Merge one input's published delta into the session's state. Entries the
	/// delta does not name are left exactly as they were, handle included.
	/// The reading half is fallible and happens first; the writing half is a
	/// `BTreeMap` insert, which cannot fail, so the delta is published whole
	/// or not at all.
	fn merge(&mut self, delta: Value) -> std::result::Result<(), Failure> {
		let entries: Vec<(String, Value)> = {
			let object =
				delta
					.borrow_ref::<rune::runtime::Object>()
					.map_err(|e| Failure::Runtime {
						message: e.to_string(),
						origin: None,
					})?;
			object
				.iter()
				.map(|(name, value)| (name.to_string(), value.clone()))
				.collect()
		};
		for (name, value) in entries {
			self.state.insert(name, value);
		}
		Ok(())
	}
	fn compile(&self, generated: &Generated) -> std::result::Result<Unit, Failure> {
		let mut sources = Sources::new();
		sources
			.insert(Source::memory(&generated.text).map_err(|e| Failure::Refused(e.to_string()))?)
			.map_err(|e| Failure::Refused(e.to_string()))?;
		let mut diagnostics = rune::Diagnostics::new();
		let built = rune::prepare(&mut sources)
			.with_context(&self.context)
			.with_diagnostics(&mut diagnostics)
			.build();
		match built {
			Ok(unit) => Ok(unit),
			Err(error) => {
				let first = diagnostics.diagnostics().iter().find_map(|d| match d {
					rune::diagnostics::Diagnostic::Fatal(fatal) => match fatal.kind() {
						rune::diagnostics::FatalDiagnosticKind::CompileError(e) => {
							Some((e.to_string(), e.span().range().start))
						}
						_ => None,
					},
					_ => None,
				});
				match first {
					Some((message, offset)) => {
						let origin = generated
							.map
							.locate(offset)
							.and_then(|(i, o)| self.origin(i, o));
						Err(Failure::Compile { message, origin })
					}
					None => Err(Failure::Compile {
						message: error.to_string(),
						origin: None,
					}),
				}
			}
		}
	}

	/// The object an input receives: only the bindings it may reference,
	/// built before anything runs, so a failure here publishes nothing.
	fn restored(&self, referenced: &BTreeSet<&str>) -> std::result::Result<Value, Failure> {
		let fault = |message: String| Failure::Refused(message);
		let mut object = rune::runtime::Object::with_capacity(referenced.len())
			.map_err(|e| fault(e.to_string()))?;
		for name in referenced {
			let Some(value) = self.state.get(*name) else {
				continue;
			};
			let key = rune::alloc::String::try_from(*name).map_err(|e| fault(e.to_string()))?;
			object
				.insert(key, value.clone())
				.map_err(|e| fault(e.to_string()))?;
		}
		rune::to_value(object).map_err(|e| fault(e.to_string()))
	}
	fn execute(&self, unit: &Arc<Unit>, state: Value) -> std::result::Result<Value, Failure> {
		// A runtime per input. Sharing one across units is what this cut set
		// out to do and cannot: see the record.
		let runtime = Arc::new(
			self.context
				.runtime()
				.map_err(|e| Failure::Refused(e.to_string()))?,
		);
		let mut vm = Vm::new(runtime, unit.clone());
		let mut execution = vm
			.execute(["main"], (state,))
			.map_err(|e| self.runtime_failure(e))?;
		let mut spent = 0usize;
		loop {
			// A slice: resume under a budget. A budget halt leaves the frozen
			// instruction pointer in place and the execution resumable; it is
			// recognised by two structural facts, no location on the error and
			// an exhausted budget guard, never by the error's text.
			let (outcome, exhausted) = budget::with(SLICE, || {
				let outcome = execution.resume().into_result();
				let exhausted = !budget::acquire().take();
				(outcome, exhausted)
			})
			.call();
			match outcome {
				// Completion is a slice boundary too: an interrupt that arrived during
				// a host call the input was waiting on is observed here.
				Ok(GeneratorState::Complete(_)) if super::host::interrupted() => {
					return Err(Failure::Interrupted);
				}
				Ok(GeneratorState::Complete(value)) => return Ok(value),
				Ok(GeneratorState::Yielded(_)) => {
					return Err(Failure::Runtime {
						message: "unexpected yield".into(),
						origin: None,
					});
				}
				Err(error) if error.first_location().is_none() && exhausted => {
					spent += SLICE;
					if super::host::interrupted() {
						return Err(Failure::Interrupted);
					}
					if spent >= self.budget {
						return Err(Failure::Budget(self.budget));
					}
				}
				Err(error) => return Err(self.runtime_failure(error)),
			}
		}
	}

	/// Every unit that ever ran is retained with its map, so the unit an
	/// error names is found by identity whichever input produced it.
	fn runtime_failure(&self, error: VmError) -> Failure {
		let message = error.to_string();
		let origin = error.first_location().and_then(|location| {
			// The unit that raised the error is alive, because the error holds
			// it; an entry whose weak reference no longer upgrades belongs to a
			// unit nothing can call and so cannot be this one.
			let map = self
				.units
				.iter()
				.find(|r| {
					r.unit
						.upgrade()
						.is_some_and(|unit| Arc::ptr_eq(&unit, &location.unit))
				})
				.map(|r| &r.map)?;
			let inst = location.unit.debug_info()?.instruction_at(location.ip)?;
			let (input, offset) = map.locate(inst.span.range().start)?;
			self.origin(input, offset)
		});
		Failure::Runtime { message, origin }
	}

	fn compile_failure(&self, message: String, at: Option<(usize, usize)>) -> Failure {
		let origin = at.and_then(|(i, o)| self.origin(i, o));
		Failure::Compile { message, origin }
	}
	fn refused_at(&self, message: &str, input: usize, offset: usize) -> Failure {
		match self.origin(input, offset) {
			Some(o) => Failure::Refused(format!(
				"{message} (input {}, line {}, column {})",
				o.input, o.line, o.column
			)),
			None => Failure::Refused(message.to_owned()),
		}
	}
	/// Line and column of a byte offset in a retained input.
	fn origin(&self, input: usize, offset: usize) -> Option<Origin> {
		let text = self.inputs.get(input.checked_sub(1)?)?;
		let (line, column, line_text) = position(text, offset);
		Some(Origin {
			input,
			line,
			column,
			text: line_text,
		})
	}
}
fn struct_fields(item: &ast::ItemStruct, source: &str) -> Vec<String> {
	match &item.body {
		ast::Fields::Named(braced) => braced
			.iter()
			.map(|(field, _)| source[field.name.span().range()].to_owned())
			.collect(),
		_ => Vec::new(),
	}
}
fn valid_name(name: &str) -> std::result::Result<(), String> {
	if name == "main" || name.starts_with("__rnx_") {
		return Err(format!("`{name}` is reserved by the session"));
	}
	Ok(())
}
fn bindings(
	pat: &ast::Pat,
	source: &str,
	names: &mut BTreeSet<String>,
) -> std::result::Result<(), String> {
	match pat {
		ast::Pat::Path(p)
			if p.path.global.is_none() && p.path.rest.is_empty() && p.path.trailing.is_none() =>
		{
			if let ast::PathSegment::Ident(ident) = &p.path.first {
				let name = &source[ident.span().range()];
				valid_name(name)?;
				names.insert(name.into());
			} else {
				return Err("unsupported binding path".into());
			}
		}
		ast::Pat::Tuple(p) => {
			for (pat, _) in p.items.iter() {
				bindings(pat, source, names)?;
			}
		}
		ast::Pat::Vec(p) => {
			for (pat, _) in p.items.iter() {
				bindings(pat, source, names)?;
			}
		}
		ast::Pat::Object(p) => {
			for (pat, _) in p.items.iter() {
				bindings(pat, source, names)?;
			}
		}
		ast::Pat::Binding(p) => bindings(&p.pat, source, names)?,
		ast::Pat::Ignore(_) | ast::Pat::Rest(_) | ast::Pat::Lit(_) => (),
		_ => return Err("unsupported binding pattern".into()),
	}
	Ok(())
}

/// The spike's session checks, kept as the regression floor for the
/// session rules. `render` is the session formatter.
pub fn checks() -> Result<()> {
	let render = |v: &Value| super::format::render(v, None, &super::format::Limits::default());
	let mut context = Context::with_default_modules()?;
	super::host::install(&mut context)?;
	let mut session = Session::new(context)?;
	for (input, expected) in [
		(
			"let (x, y) = (2, 3); let shared = [1]; let effects = []; fn f() { 10 } struct Boxed { value } let old = f; let c = || f() + shared[0]; let b = Boxed { value: 7 }; x + y",
			"5",
		),
		(
			"effects.push(1); fn f() { 20 } (old(), f(), c(), b.value, b is Boxed)",
			"(10, 20, 11, 7, true)",
		),
		(
			"let x = x + 5; let [left, right] = [8,9]; let #{a: renamed} = #{a: 4}; (x,y,left,right,renamed,effects.len())",
			"(7, 3, 8, 9, 4, 1)",
		),
	] {
		let value = session.eval(input)?;
		let actual = render(&value);
		assert_eq!(actual, expected);
		println!("session: {actual}");
	}
	assert!(session.eval("fn f() { 99 } let broken = ;").is_err());
	assert!(
		session
			.eval("shared.push(2); let x = 99; let new_name = 1; panic!(\"partial\");")
			.is_err()
	);
	assert!(session.eval("new_name").is_err());
	let value = session.eval("(x, shared, f(), effects.len())")?;
	assert_eq!(render(&value), "(7, [1, 2], 20, 1)");
	println!("after compile/runtime failures: {}", render(&value));
	assert!(session.eval("struct Boxed { other }").is_err());
	println!("changed type declaration: refused until reset");
	session.set_budget(2_000_000);
	let error = session.eval("while true {} ").unwrap_err();
	assert!(matches!(error, Failure::Budget(_)));
	println!("infinite loop: {error}");
	assert_eq!(render(&session.eval("x")?), "7");
	let effect_path = std::env::temp_dir().join(format!(
		"rnx-effect-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)?
			.as_nanos()
	));
	let quoted = serde_json::to_string(&effect_path.to_string_lossy())?;
	assert!(
		session
			.eval(&format!(
				"host::write_new({quoted}, \"once\")?; panic!(\"after external write\");"
			))
			.is_err()
	);
	assert_eq!(std::fs::read_to_string(&effect_path)?, "once");
	assert_eq!(render(&session.eval("x")?), "7");
	assert_eq!(std::fs::read_to_string(&effect_path)?, "once");
	std::fs::remove_file(effect_path)?;
	println!("external write before runtime failure survives; later input does not replay it");
	let mut context = Context::with_default_modules()?;
	super::host::install(&mut context)?;
	let mut reset = Session::new(context)?;
	assert!(reset.eval("x").is_err());
	let start = std::time::Instant::now();
	for _ in 0..100 {
		session.eval("let x = x + 1; x")?;
	}
	assert_eq!(render(&session.eval("x")?), "107");
	println!(
		"100 incremental inputs (compile + execute): {:?}",
		start.elapsed()
	);
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	fn context() -> Context {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		context
	}

	#[test]
	fn incomplete_inputs_are_the_closed_list() {
		for input in [
			"fn f() {",
			"fn f() {\n  let a = 1;",
			"let xs = [",
			"let x =",
			"let x = \"unterminated",
			"let x = 1; /* open comment",
			"foo(",
			"if x {",
			"let t = (1,",
			"let o = #{a: 1,",
			// A `let` needs its `;` in Rune; without it the parser wants more.
			"let s = \"closed\"",
		] {
			assert_eq!(completeness(input), Completeness::Incomplete, "{input:?}");
		}
	}

	#[test]
	fn complete_inputs_include_invalid_ones() {
		for input in [
			"let x = ;",
			"foo())",
			"foo \"abc\"",
			"let a = 1; let b = 2;",
			"fn f() { 1 }",
			"x + 1",
			"struct S { a }",
			"[1, 2, 3]",
			"let = 5;",
			"1 +* 2;",
		] {
			assert_eq!(completeness(input), Completeness::Complete, "{input:?}");
		}
	}

	#[test]
	fn compile_error_maps_to_the_typed_line_and_column() {
		let mut session = Session::new(context()).unwrap();
		let failure = session
			.eval("let a = 1;\nlet b = +* 2;\nlet c = 3;")
			.unwrap_err();
		match failure {
			Failure::Compile {
				origin: Some(o), ..
			} => {
				assert_eq!((o.input, o.line, o.column), (1, 2, 9));
				assert_eq!(o.text, "let b = +* 2;");
			}
			other => panic!("{other:?}"),
		}
	}

	#[test]
	fn runtime_error_in_an_older_definition_reports_that_input() {
		let mut session = Session::new(context()).unwrap();
		session
			.eval("fn boom(x) {\n  panic!(\"bad {}\", x)\n}")
			.unwrap();
		session.eval("let y = 2;").unwrap();
		let failure = session.eval("boom(y)").unwrap_err();
		match failure {
			Failure::Runtime {
				message,
				origin: Some(o),
			} => {
				assert!(message.contains("bad 2"), "{message}");
				assert_eq!((o.input, o.line, o.column), (1, 2, 3));
				assert_eq!(o.text, "  panic!(\"bad {}\", x)");
			}
			other => panic!("{other:?}"),
		}
	}

	#[test]
	fn runtime_error_through_a_retained_closure_reports_the_closure_input() {
		let mut session = Session::new(context()).unwrap();
		session.eval("let c = |v| v.missing_method();").unwrap();
		session.eval("let z = 1;").unwrap();
		let failure = session.eval("c(z)").unwrap_err();
		match failure {
			Failure::Runtime {
				origin: Some(o), ..
			} => assert_eq!((o.input, o.line), (1, 1)),
			other => panic!("{other:?}"),
		}
	}

	#[test]
	fn every_refusal_and_failure_reports_user_text_never_generated_text() {
		let mut session = Session::new(context()).unwrap();
		session.eval("let keep = 1;").unwrap();
		for input in [
			"let x = ;",
			"use std::fmt;",
			"impl Foo { fn f() {} }",
			"macro_rules! m { () => {} }",
			"fn main() { 1 }",
			"let __rnx_x = 1;",
			"let q = 1 / 0;",
			"nonexistent_fn()",
			"let v = [1]; v[9]",
			"panic!(\"p\")",
		] {
			let failure = session.eval(input).unwrap_err();
			let text = failure.to_string();
			assert!(!text.contains("__rnx_state"), "{input}: {text}");
			assert!(!text.contains("not in your input"), "{input}: {text}");
			if let Failure::Compile { origin, .. } | Failure::Runtime { origin, .. } = &failure {
				assert!(origin.is_some(), "{input}: {text}");
			}
		}
		assert_eq!(
			rune::from_value::<i64>(session.eval("keep").unwrap()).unwrap(),
			1
		);
	}

	#[test]
	fn a_closure_retained_by_a_failed_input_still_maps_to_that_input() {
		let mut session = Session::new(context()).unwrap();
		session.eval("let saved = [];").unwrap();
		assert!(
			session
				.eval("saved.push(|| panic!(\"retained failure\")); panic!(\"outer\");")
				.is_err()
		);
		let failure = session.eval("let f = saved[0]; f()").unwrap_err();
		match failure {
			Failure::Runtime {
				message,
				origin: Some(o),
			} => {
				assert!(message.contains("retained failure"), "{message}");
				assert_eq!((o.input, o.line), (2, 1));
			}
			other => panic!("{other:?}"),
		}
	}

	#[test]
	fn over_the_ceiling_refuses_evaluation_but_not_inspection() {
		// A ceiling of one byte: any sample is at or above it, so the latch
		// trips without depending on what the rest of the process allocated.
		let mut session = Session::with_ceiling(context(), 1).unwrap();
		session.eval("let a = 1;").unwrap();
		// Unlatched until a sample says so: the ceiling is not checked during
		// an input, only after one.
		assert!(!session.over_ceiling());
		assert!(session.sample().is_some());
		assert!(session.over_ceiling());
		let failure = session.eval("a").unwrap_err();
		assert!(
			matches!(failure, Failure::OverCeiling { .. }),
			"{failure:?}"
		);
		// The latch holds without re-testing, and inspection still answers.
		assert!(session.eval("a").is_err());
		assert!(session.retained_bytes() > 0);
		assert!(!session.last_generated().is_empty());
	}
}

#[cfg(test)]
mod retention_tests {
	use super::*;

	fn session() -> Session {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		let mut session = Session::new(context).unwrap();
		session.set_budget(usize::MAX);
		session
	}

	#[test]
	fn a_unit_nothing_can_call_is_released_and_its_entry_pruned() {
		let mut session = session();
		for i in 0..100 {
			session.eval(&format!("let x = {i};")).unwrap();
		}
		// Every unit has been released, the last one included: the session
		// holds only weak references, so a unit dies with the evaluation that
		// made it unless a value keeps it alive. One entry remains, the one
		// the last input registered, and the next input prunes it.
		assert_eq!(session.live_units(), 0, "live units");
		assert_eq!(session.retained_units(), 1, "entries");
	}

	#[test]
	fn a_unit_something_can_still_call_is_kept() {
		let mut session = session();
		for i in 0..20 {
			session.eval(&format!("let c{i} = || {i};")).unwrap();
		}
		// Each closure keeps its own unit reachable, so nothing is pruned.
		assert_eq!(session.live_units(), 20, "live units");
		assert_eq!(session.retained_units(), 20, "entries");
		// Dropping the closures by rebinding releases them again.
		for i in 0..20 {
			session.eval(&format!("let c{i} = 0;")).unwrap();
		}
		assert_eq!(
			session.live_units(),
			0,
			"live units after the closures went"
		);
	}

	#[test]
	fn a_closure_kept_by_a_failed_input_keeps_its_unit_and_its_position() {
		let mut session = session();
		session.eval("let saved = [];").unwrap();
		// The input registers its entry, stores a closure through a shared
		// handle, and then fails. The closure is still callable.
		assert!(
			session
				.eval("saved.push(|| panic!(\"from the failed input\")); panic!(\"outer\");")
				.is_err()
		);
		assert_eq!(
			session.live_units(),
			1,
			"the stored closure should keep the failed input's unit alive"
		);
		let failure = session.eval("let f = saved[0]; f()").unwrap_err();
		match failure {
			Failure::Runtime {
				message,
				origin: Some(origin),
			} => {
				assert!(message.contains("from the failed input"), "{message}");
				assert_eq!(origin.input, 2, "the closure's defining input");
			}
			other => panic!("{other:?}"),
		}
	}

	#[test]
	fn every_ordinary_runtime_error_finds_its_position() {
		let mut session = session();
		session
			.eval("fn thrower() {\n  panic!(\"deep\")\n}")
			.unwrap();
		session.eval("let held = || thrower();").unwrap();
		for input in [
			"panic!(\"here\")",
			"thrower()",
			"held()",
			"let v = []; v[3]",
		] {
			let failure = session.eval(input).unwrap_err();
			match failure {
				Failure::Runtime {
					origin: Some(_), ..
				} => {}
				other => panic!("{input}: no position: {other:?}"),
			}
		}
	}

	#[test]
	fn a_missing_entry_reports_the_position_as_unavailable() {
		let mut session = session();
		session.eval("fn thrower() { panic!(\"x\") }").unwrap();
		session.eval("let held = || thrower();").unwrap();
		// The defensive path: the entry is gone although the unit lives. This
		// cannot happen in ordinary execution, which the test above covers.
		session.units.clear();
		let failure = session.eval("held()").unwrap_err();
		match failure {
			Failure::Runtime { origin: None, .. } => {}
			other => panic!("expected an unavailable position, got {other:?}"),
		}
		assert!(failure_text(&failure).contains("position not in your input"));
	}

	fn failure_text(failure: &Failure) -> String {
		failure.to_string()
	}

	#[test]
	fn a_reset_keeps_the_context_and_empties_the_session() {
		let mut session = session();
		session.eval("let c = || 1;").unwrap();
		session.eval("let d = || 2;").unwrap();
		assert_eq!(session.binding_count(), 2);
		session.reset();
		assert_eq!(session.binding_count(), 0);
		assert_eq!(session.retained_units(), 0);
		// The context survives, so the session still evaluates afterwards.
		assert_eq!(
			rune::from_value::<i64>(session.eval("1 + 1").unwrap()).unwrap(),
			2
		);
	}
}

/// The line and column of a byte offset in `text`, both counted from one,
/// with that line's text. The column counts characters, not bytes, so a
/// line with characters outside ASCII before the offset still reports the
/// column a person would count. Shared with the file runner so the session
/// and `run` report the same place for the same source.
pub fn position(text: &str, offset: usize) -> (usize, usize, String) {
	let offset = offset.min(text.len());
	let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
	let line = text[..line_start].matches('\n').count() + 1;
	let column = text[line_start..offset].chars().count() + 1;
	let line_text = text[line_start..].lines().next().unwrap_or("").to_owned();
	(line, column, line_text)
}

/// Every published name an input may reference. Sound over-approximation:
/// identifier tokens wherever they appear, plus the leading identifier of
/// each `{...}` group inside a string literal, because `format!("{x}")`
/// reads `x` without `x` ever being an identifier token. Deliberately not
/// completion's scan, which suppresses strings, comments, and template
/// interpolations; every one of those suppressions would be a defect here.
fn mentioned<'a>(input: &str, published: &'a BTreeSet<String>) -> BTreeSet<&'a str> {
	let mut found = BTreeSet::new();
	if published.is_empty() {
		return found;
	}
	let everything = || {
		published
			.iter()
			.map(String::as_str)
			.collect::<BTreeSet<_>>()
	};
	let mut parser = rune::parse::Parser::new(input, SourceId::empty(), false);
	loop {
		match parser.parse::<ast::Token>() {
			Ok(token) if matches!(token.kind, ast::Kind::Eof) => break,
			Ok(token) => {
				let text = &input[token.span.range()];
				match token.kind {
					ast::Kind::Ident(_) => {
						if let Some(name) = published.get(text) {
							found.insert(name.as_str());
						}
					}
					// A string literal's own text, braces and all. Templates
					// arrive as identifier tokens instead, already covered.
					ast::Kind::Str(_) => {
						// The formatter reads the decoded string, so the scan must
						// too. An escape this decoder does not recognise means the
						// contents are unknown, and an unknown string could name
						// anything: restore everything rather than miss a capture.
						let Some(decoded) = decode(text) else {
							return everything();
						};
						for group in format_captures(&decoded) {
							if let Some(name) = published.get(group) {
								found.insert(name.as_str());
							}
						}
					}
					_ => {}
				}
			}
			// A partial lex still yields what it read; the caller compiles
			// next and reports any real error with a position.
			Err(_) => break,
		}
	}
	found
}
/// A string literal decoded far enough to find its format captures, or
/// `None` when an escape appears that this decoder does not recognise. Rune
/// decodes the literal before the formatter reads it, so `"\\u{7b}x}"` is
/// `{x}` and captures `x`; scanning the raw text would miss it. Rune's own
/// decoder is crate-private, so this one mirrors the escapes its lexer
/// accepts and refuses to guess at anything else.
fn decode(text: &str) -> Option<String> {
	let mut out = String::with_capacity(text.len());
	let mut chars = text.chars();
	while let Some(c) = chars.next() {
		if c != '\\' {
			out.push(c);
			continue;
		}
		match chars.next()? {
			'n' => out.push('\n'),
			'r' => out.push('\r'),
			't' => out.push('\t'),
			'0' => out.push('\0'),
			'\\' => out.push('\\'),
			'\'' => out.push('\''),
			'"' => out.push('"'),
			// Two hex digits, at most 0x7f, as the lexer requires.
			'x' => {
				let mut value = 0u32;
				for _ in 0..2 {
					value = value * 16 + chars.next()?.to_digit(16)?;
				}
				out.push(char::from_u32(value)?);
			}
			// `u{HEX}`, the form that can hide an opening brace.
			'u' => {
				if chars.next()? != '{' {
					return None;
				}
				let mut value = 0u32;
				loop {
					match chars.next()? {
						'}' => break,
						digit => value = value.checked_mul(16)?.checked_add(digit.to_digit(16)?)?,
					}
				}
				out.push(char::from_u32(value)?);
			}
			// An escape this decoder does not know. The caller restores
			// everything rather than miss a capture.
			_ => return None,
		}
	}
	Some(out)
}
/// The name part of each `{...}` group in decoded text: everything from the
/// brace to the first `}` or `:`, which are what end a format argument's
/// name. The caller looks the result up among the published names, so this
/// needs no view on what characters an identifier may contain, and a name
/// like `café` is found without knowing Rune's identifier rules.
fn format_captures(text: &str) -> Vec<&str> {
	let mut names = Vec::new();
	let mut rest = text;
	while let Some(open) = rest.find('{') {
		let after = &rest[open + 1..];
		// `{{` is a literal brace and captures nothing; skipping both is what
		// lets `{{{name}}}`, a literal brace beside a capture, still be seen.
		if after.starts_with('{') {
			rest = &after[1..];
			continue;
		}
		let end = after.find(|c| c == '}' || c == ':').unwrap_or(after.len());
		if end > 0 {
			names.push(&after[..end]);
		}
		rest = &after[end.min(after.len())..];
		if rest.is_empty() {
			break;
		}
		rest = &rest[rest.char_indices().nth(1).map_or(rest.len(), |(i, _)| i)..];
	}
	names
}

#[cfg(test)]
mod prelude_tests {
	use super::*;

	fn session() -> Session {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		let mut session = Session::new(context).unwrap();
		session.set_budget(usize::MAX);
		session
	}
	fn shown(session: &mut Session, input: &str) -> String {
		let value = session.eval(input).unwrap();
		crate::format::render(
			&value,
			Some(&session.fields()),
			&crate::format::Limits::default(),
		)
	}

	#[test]
	fn the_generated_source_follows_the_input_not_the_session() {
		let mut wide = session();
		for i in 0..100 {
			wide.eval(&format!("let n{i:03} = {i};")).unwrap();
		}
		wide.eval("n000 + n099").unwrap();
		let with_hundred = wide.last_generated().len();

		let mut narrow = session();
		narrow.eval("let n000 = 0;").unwrap();
		narrow.eval("let n099 = 99;").unwrap();
		narrow.eval("n000 + n099").unwrap();
		let with_two = narrow.last_generated().len();

		assert_eq!(
			with_hundred, with_two,
			"the same input generated {with_hundred} bytes against a hundred names \
			 and {with_two} against two"
		);
		// Two restoring lines and a delta of two, whatever the session holds.
		assert_eq!(wide.last_generated().matches("__rnx_state[").count(), 2);
	}

	#[test]
	fn assignment_shadowing_and_destructuring_publish_correctly() {
		let mut session = session();
		session.eval("let a = 1; let b = 2; let c = 3;").unwrap();
		// Assignment to a restored name.
		session.eval("a = 10;").unwrap();
		assert_eq!(shown(&mut session, "a"), "10");
		// Shadowing a restored name.
		session.eval("let b = b + 40;").unwrap();
		assert_eq!(shown(&mut session, "b"), "42");
		// Destructuring, publishing new names.
		session.eval("let [d, e] = [4, 5];").unwrap();
		assert_eq!(shown(&mut session, "(c, d, e)"), "(3, 4, 5)");
		// A name no input has mentioned for many inputs is untouched.
		for _ in 0..20 {
			session.eval("let filler = 0;").unwrap();
		}
		assert_eq!(shown(&mut session, "(a, b, c, d, e)"), "(10, 42, 3, 4, 5)");
	}

	#[test]
	fn aliases_keep_one_shared_handle_across_a_narrowed_prelude() {
		let mut session = session();
		session.eval("let original = [1];").unwrap();
		session.eval("let alias = original;").unwrap();
		// Many inputs that mention neither, so neither is restored.
		for _ in 0..10 {
			session.eval("let other = 0;").unwrap();
		}
		// Mutate through one name, read through the other.
		session.eval("alias.push(2);").unwrap();
		assert_eq!(shown(&mut session, "original"), "[1, 2]");
		session.eval("original.push(3);").unwrap();
		assert_eq!(shown(&mut session, "alias"), "[1, 2, 3]");
	}

	#[test]
	fn a_closure_captures_a_restored_binding_and_keeps_seeing_it() {
		let mut session = session();
		session.eval("let counter = [0];").unwrap();
		session.eval("let bump = || counter.push(1);").unwrap();
		for _ in 0..10 {
			session.eval("let unrelated = 0;").unwrap();
		}
		session.eval("bump(); bump();").unwrap();
		assert_eq!(shown(&mut session, "counter"), "[0, 1, 1]");
	}

	#[test]
	fn a_failed_input_publishes_no_rebinding_and_keeps_its_mutation() {
		let mut session = session();
		session.eval("let shared = [1]; let scalar = 1;").unwrap();
		assert!(
			session
				.eval("shared.push(2); scalar = 99; let fresh = 5; panic!(\"stop\");")
				.is_err()
		);
		// The mutation through the shared handle stands; the rebinding does not.
		assert_eq!(shown(&mut session, "shared"), "[1, 2]");
		assert_eq!(shown(&mut session, "scalar"), "1");
		assert!(session.eval("fresh").is_err());
	}

	#[test]
	fn a_format_capture_restores_the_binding_it_names() {
		let mut session = session();
		session.eval("let captured = 7;").unwrap();
		for _ in 0..10 {
			session.eval("let noise = 0;").unwrap();
		}
		// `captured` appears only inside a string literal, never as an
		// identifier token, and must still be restored.
		assert_eq!(shown(&mut session, "format!(\"{captured}\")"), "\"7\"");
		assert_eq!(
			shown(&mut session, "format!(\"{captured:?} and {captured}\")"),
			"\"7 and 7\""
		);
		session.eval("println!(\"{captured}\");").unwrap();
	}

	#[test]
	fn a_template_interpolation_restores_the_binding_it_names() {
		let mut session = session();
		session.eval("let inside = 3;").unwrap();
		for _ in 0..5 {
			session.eval("let noise = 0;").unwrap();
		}
		assert_eq!(shown(&mut session, "`v ${inside}`"), "\"v 3\"");
	}

	#[test]
	fn the_scan_over_approximates_rather_than_missing_a_name() {
		let published: BTreeSet<String> =
			["a", "b", "unused"].iter().map(|s| s.to_string()).collect();
		// Identifier tokens.
		assert_eq!(mentioned("a + 1", &published), ["a"].into_iter().collect());
		// A format capture inside a string.
		assert_eq!(
			mentioned("format!(\"{b}\")", &published),
			["b"].into_iter().collect()
		);
		// A plain string that merely looks like one: over-approximated, which
		// costs a restoring line and nothing else.
		assert_eq!(
			mentioned("\"{a}\"", &published),
			["a"].into_iter().collect()
		);
		// A comment mentions nothing, because it is not a token.
		assert!(mentioned("// a b unused\n1", &published).is_empty());
		// Nothing published, nothing scanned.
		assert!(mentioned("a + b", &BTreeSet::new()).is_empty());
	}
}

#[cfg(test)]
mod escape_tests {
	use super::*;

	fn published() -> BTreeSet<String> {
		["x", "y"].iter().map(|s| s.to_string()).collect()
	}

	#[test]
	fn a_capture_hidden_behind_an_escape_is_still_found() {
		let published = published();
		// `\u{7b}` decodes to `{`, so the formatter reads `{x}`. Scanning the
		// raw text would see `{7b}` and miss `x`.
		assert_eq!(
			mentioned("format!(\"\\u{7b}x}\")", &published),
			["x"].into_iter().collect()
		);
		// The same through a hex escape.
		assert_eq!(
			mentioned("format!(\"\\x7bx}\")", &published),
			["x"].into_iter().collect()
		);
		// `{{x}}` is a literal `{x}`, not a capture, so nothing is restored.
		assert!(mentioned("format!(\"{{x}}\")", &published).is_empty());
		// `{{{x}}}` is a literal brace beside a real capture of `x`, which the
		// scan must still see.
		assert_eq!(
			mentioned("format!(\"{{{x}}}\")", &published),
			["x"].into_iter().collect()
		);
	}

	#[test]
	fn an_unrecognised_escape_restores_everything() {
		let published = published();
		// `\q` is not an escape Rune's lexer accepts, so what the string holds
		// is unknown; the scan restores every published name rather than risk
		// missing a capture.
		assert_eq!(
			mentioned("format!(\"\\q\")", &published),
			["x", "y"].into_iter().collect()
		);
	}

	#[test]
	fn ordinary_escapes_decode_without_widening() {
		let published = published();
		// A newline escape decodes and names nothing.
		assert!(mentioned("format!(\"a\\nb\")", &published).is_empty());
		// And still finds a real capture beside it.
		assert_eq!(
			mentioned("format!(\"a\\nb {y}\")", &published),
			["y"].into_iter().collect()
		);
	}

	#[test]
	fn the_decoder_matches_the_escapes_the_lexer_accepts() {
		assert_eq!(decode("a\\nb").as_deref(), Some("a\nb"));
		assert_eq!(decode("\\t\\r\\0").as_deref(), Some("\t\r\0"));
		assert_eq!(decode("\\\\").as_deref(), Some("\\"));
		assert_eq!(decode("\\\"").as_deref(), Some("\""));
		assert_eq!(decode("\\x7b").as_deref(), Some("{"));
		assert_eq!(decode("\\u{7b}").as_deref(), Some("{"));
		assert_eq!(decode("\\u{1F4AF}").as_deref(), Some("\u{1F4AF}"));
		// Unknown or malformed: the caller widens rather than guesses.
		assert_eq!(decode("\\q"), None);
		assert_eq!(decode("\\u7b"), None);
		assert_eq!(decode("\\x7"), None);
		assert_eq!(decode("trailing\\"), None);
	}
}

#[cfg(test)]
mod unicode_capture_tests {
	use super::*;

	fn published() -> BTreeSet<String> {
		["café", "a", "ab", "ünïcødé"]
			.iter()
			.map(|s| s.to_string())
			.collect()
	}

	#[test]
	fn a_capture_naming_a_unicode_binding_is_found() {
		let published = published();
		// Written literally.
		assert_eq!(
			mentioned("format!(\"{café}\")", &published),
			["café"].into_iter().collect()
		);
		// Written as an escape, which decodes to the same name.
		assert_eq!(
			mentioned("format!(\"{caf\\u{e9}}\")", &published),
			["café"].into_iter().collect()
		);
		// With a format spec after the name.
		assert_eq!(
			mentioned("format!(\"{café:?}\")", &published),
			["café"].into_iter().collect()
		);
		// A name of nothing but non-ASCII letters.
		assert_eq!(
			mentioned("format!(\"{ünïcødé}\")", &published),
			["ünïcødé"].into_iter().collect()
		);
	}

	#[test]
	fn a_longer_name_is_not_shadowed_by_its_own_prefix() {
		let published = published();
		// `a` is published and is a prefix of `ab`; the capture is `ab`.
		assert_eq!(
			mentioned("format!(\"{ab}\")", &published),
			["ab"].into_iter().collect()
		);
		assert_eq!(
			mentioned("format!(\"{a}\")", &published),
			["a"].into_iter().collect()
		);
	}

	#[test]
	fn a_group_naming_nothing_published_restores_nothing() {
		let published = published();
		assert!(mentioned("format!(\"{0}\")", &published).is_empty());
		assert!(mentioned("format!(\"{}\", 1)", &published).is_empty());
		assert!(mentioned("format!(\"{unknown}\")", &published).is_empty());
	}
}

#[cfg(test)]
mod position_tests {
	use super::position;

	#[test]
	fn a_column_counts_characters_not_bytes() {
		// A tab is one character.
		let text = "fn main() {\n\t\tlet x = ;\n}";
		let offset = text.find(';').unwrap();
		assert_eq!(position(text, offset), (2, 11, "\t\tlet x = ;".to_owned()));
		// Characters outside ASCII count as one each, not as their bytes.
		let text = "fn main() {\n\tlet café = ;\n}";
		let offset = text.find(';').unwrap();
		assert_eq!(position(text, offset), (2, 13, "\tlet café = ;".to_owned()));
	}

	#[test]
	fn an_offset_past_the_end_lands_on_the_last_line() {
		let text = "one\ntwo";
		assert_eq!(position(text, 9999), (2, 4, "two".to_owned()));
	}

	#[test]
	fn the_first_character_is_line_one_column_one() {
		assert_eq!(position("abc", 0), (1, 1, "abc".to_owned()));
	}
}
