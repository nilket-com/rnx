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
use std::sync::Arc;

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
	match origin {
		Some(o) => {
			writeln!(
				f,
				"{kind} at input {}, line {}, column {}: {message}",
				o.input, o.line, o.column
			)?;
			writeln!(f, "  {}", o.text)?;
			write!(f, "  {}^", " ".repeat(o.column.saturating_sub(1)))
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

struct Retained {
	unit: Arc<Unit>,
	generated: String,
	map: SourceMap,
}

pub struct Session {
	declarations: BTreeMap<String, Declaration>,
	names: BTreeSet<String>,
	state: Value,
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
	pub fn new() -> Self {
		Self::with_ceiling(DEFAULT_CEILING)
	}
	pub fn with_ceiling(ceiling: usize) -> Self {
		Self {
			declarations: BTreeMap::new(),
			names: BTreeSet::new(),
			state: serde_json::from_str("{}").unwrap(),
			inputs: Vec::new(),
			units: Vec::new(),
			last_generated: String::new(),
			ceiling,
			over_ceiling: false,
			budget: BUDGET,
		}
	}
	/// The generated source of the most recent input, for `:debug`.
	pub fn last_generated(&self) -> &str {
		&self.last_generated
	}
	/// Bytes the session retains: inputs, declaration sources, generated
	/// sources of retained units, and their source maps. Values held by
	/// bindings are not measured; see the first release record.
	pub fn retained_bytes(&self) -> usize {
		self.inputs.iter().map(String::len).sum::<usize>()
			+ self
				.declarations
				.values()
				.map(|d| d.source.len())
				.sum::<usize>()
			+ self
				.units
				.iter()
				.map(|u| u.generated.len() + u.map.bytes())
				.sum::<usize>()
	}
	/// Instructions one input may spend before it is halted.
	pub fn set_budget(&mut self, budget: usize) {
		self.budget = budget;
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
	pub fn retained_units(&self) -> usize {
		self.units.len()
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
		let Ok(object) = self.state.borrow_ref::<rune::runtime::Object>() else {
			return 0;
		};
		let mut visited = 0;
		for name in &self.names {
			let Some(value) = object.get(name.as_str()) else {
				continue;
			};
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
		if !self.names.contains(name) {
			return None;
		}
		let object = self.state.borrow_ref::<rune::runtime::Object>().ok()?;
		object.get(name).cloned()
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
	/// Field names of a struct the session declared, in declaration order.
	pub fn struct_fields(&self, name: &str) -> Option<&[String]> {
		self.declarations
			.get(name)
			.filter(|d| d.is_type)
			.map(|d| d.fields.as_slice())
	}

	pub fn eval(&mut self, context: &Context, input: &str) -> std::result::Result<Value, Failure> {
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
		let result = self.eval_inner(context, input, number);
		if result.is_err() {
			// A failed input publishes nothing, but its text stays so an origin
			// recorded against it (there is none) could never dangle.
		}
		result
	}

	fn eval_inner(
		&mut self,
		context: &Context,
		input: &str,
		number: usize,
	) -> std::result::Result<Value, Failure> {
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
			// ItemStruct's derived span omits its closing delimiter in 0.14.1.
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
		for name in &self.names {
			generated.raw(&format!("let {name} = __rnx_state[\"{name}\"];\n"));
		}
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
		generated.raw(");\n(#{");
		generated.raw(&names.iter().cloned().collect::<Vec<_>>().join(","));
		generated.raw("}, __rnx_result)\n}");
		self.last_generated = generated.text.clone();

		let unit = self.compile(context, &generated)?;
		let unit = Arc::new(unit);
		// The unit and its map are retained before it runs: a failed or
		// interrupted input can still leave closures over this unit reachable
		// through shared handles, and their errors must map to this input.
		self.units.push(Retained {
			unit: unit.clone(),
			generated: generated.text,
			map: generated.map,
		});
		let output = self.execute(context, &unit)?;
		let (state, value): (Value, Value) =
			rune::from_value(output).map_err(|e| Failure::Runtime {
				message: e.to_string(),
				origin: None,
			})?;
		// Publish bindings and declarations only after a successful run. Values
		// are shared handles: mutations on a failed input remain visible.
		self.declarations = declarations;
		self.names = names;
		self.state = state;
		Ok(value)
	}

	fn compile(
		&self,
		context: &Context,
		generated: &Generated,
	) -> std::result::Result<Unit, Failure> {
		let mut sources = Sources::new();
		sources
			.insert(Source::memory(&generated.text).map_err(|e| Failure::Refused(e.to_string()))?)
			.map_err(|e| Failure::Refused(e.to_string()))?;
		let mut diagnostics = rune::Diagnostics::new();
		let built = rune::prepare(&mut sources)
			.with_context(context)
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

	fn execute(&self, context: &Context, unit: &Arc<Unit>) -> std::result::Result<Value, Failure> {
		let runtime = Arc::new(
			context
				.runtime()
				.map_err(|e| Failure::Refused(e.to_string()))?,
		);
		let mut vm = Vm::new(runtime, unit.clone());
		let mut execution = vm
			.execute(["main"], (self.state.clone(),))
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
			let map = self
				.units
				.iter()
				.find(|r| Arc::ptr_eq(&r.unit, &location.unit))
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
		let offset = offset.min(text.len());
		let line_start = text[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
		let line = text[..line_start].matches('\n').count() + 1;
		let column = text[line_start..offset].chars().count() + 1;
		let line_text = text[line_start..].lines().next().unwrap_or("").to_owned();
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
pub fn checks(context: &Context) -> Result<()> {
	let render = |v: &Value| super::format::render(v, None, &super::format::Limits::default());
	let mut session = Session::new();
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
		let value = session.eval(context, input)?;
		let actual = render(&value);
		assert_eq!(actual, expected);
		println!("session: {actual}");
	}
	assert!(
		session
			.eval(context, "fn f() { 99 } let broken = ;")
			.is_err()
	);
	assert!(
		session
			.eval(
				context,
				"shared.push(2); let x = 99; let new_name = 1; panic!(\"partial\");"
			)
			.is_err()
	);
	assert!(session.eval(context, "new_name").is_err());
	let value = session.eval(context, "(x, shared, f(), effects.len())")?;
	assert_eq!(render(&value), "(7, [1, 2], 20, 1)");
	println!("after compile/runtime failures: {}", render(&value));
	assert!(session.eval(context, "struct Boxed { other }").is_err());
	println!("changed type declaration: refused until reset");
	session.set_budget(2_000_000);
	let error = session.eval(context, "while true {} ").unwrap_err();
	assert!(matches!(error, Failure::Budget(_)));
	println!("infinite loop: {error}");
	assert_eq!(render(&session.eval(context, "x")?), "7");
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
			.eval(
				context,
				&format!("host::write_new({quoted}, \"once\")?; panic!(\"after external write\");")
			)
			.is_err()
	);
	assert_eq!(std::fs::read_to_string(&effect_path)?, "once");
	assert_eq!(render(&session.eval(context, "x")?), "7");
	assert_eq!(std::fs::read_to_string(&effect_path)?, "once");
	std::fs::remove_file(effect_path)?;
	println!("external write before runtime failure survives; later input does not replay it");
	let mut reset = Session::new();
	assert!(reset.eval(context, "x").is_err());
	let start = std::time::Instant::now();
	for _ in 0..100 {
		session.eval(context, "let x = x + 1; x")?;
	}
	assert_eq!(render(&session.eval(context, "x")?), "107");
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
		let context = context();
		let mut session = Session::new();
		let failure = session
			.eval(&context, "let a = 1;\nlet b = +* 2;\nlet c = 3;")
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
		let context = context();
		let mut session = Session::new();
		session
			.eval(&context, "fn boom(x) {\n  panic!(\"bad {}\", x)\n}")
			.unwrap();
		session.eval(&context, "let y = 2;").unwrap();
		let failure = session.eval(&context, "boom(y)").unwrap_err();
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
		let context = context();
		let mut session = Session::new();
		session
			.eval(&context, "let c = |v| v.missing_method();")
			.unwrap();
		session.eval(&context, "let z = 1;").unwrap();
		let failure = session.eval(&context, "c(z)").unwrap_err();
		match failure {
			Failure::Runtime {
				origin: Some(o), ..
			} => assert_eq!((o.input, o.line), (1, 1)),
			other => panic!("{other:?}"),
		}
	}

	#[test]
	fn every_refusal_and_failure_reports_user_text_never_generated_text() {
		let context = context();
		let mut session = Session::new();
		session.eval(&context, "let keep = 1;").unwrap();
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
			let failure = session.eval(&context, input).unwrap_err();
			let text = failure.to_string();
			assert!(!text.contains("__rnx_state"), "{input}: {text}");
			assert!(!text.contains("not in your input"), "{input}: {text}");
			if let Failure::Compile { origin, .. } | Failure::Runtime { origin, .. } = &failure {
				assert!(origin.is_some(), "{input}: {text}");
			}
		}
		assert_eq!(
			rune::from_value::<i64>(session.eval(&context, "keep").unwrap()).unwrap(),
			1
		);
	}

	#[test]
	fn a_closure_retained_by_a_failed_input_still_maps_to_that_input() {
		let context = context();
		let mut session = Session::new();
		session.eval(&context, "let saved = [];").unwrap();
		assert!(
			session
				.eval(
					&context,
					"saved.push(|| panic!(\"retained failure\")); panic!(\"outer\");"
				)
				.is_err()
		);
		let failure = session.eval(&context, "let f = saved[0]; f()").unwrap_err();
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
		let context = context();
		// A ceiling of one byte: any sample is at or above it, so the latch
		// trips without depending on what the rest of the process allocated.
		let mut session = Session::with_ceiling(1);
		session.eval(&context, "let a = 1;").unwrap();
		// Unlatched until a sample says so: the ceiling is not checked during
		// an input, only after one.
		assert!(!session.over_ceiling());
		assert!(session.sample().is_some());
		assert!(session.over_ceiling());
		let failure = session.eval(&context, "a").unwrap_err();
		assert!(
			matches!(failure, Failure::OverCeiling { .. }),
			"{failure:?}"
		);
		// The latch holds without re-testing, and inspection still answers.
		assert!(session.eval(&context, "a").is_err());
		assert!(session.retained_bytes() > 0);
		assert!(!session.last_generated().is_empty());
	}
}
