//! `:vars` and `:help`: bounded, read-only descriptions of what the session
//! holds. Nothing here evaluates, compiles, or invokes a protocol, so both
//! commands answer even when the session is over its memory threshold and
//! Rune evaluation is refused. Every path checks the invocation budget
//! before it copies or escapes, so a command's work is bounded by what it
//! may print, not by the size of what it describes.
use crate::format::{Limits, render, terminal_safe_into, type_name};
use crate::host::HostFunction;
use crate::session::Session;

/// Bounds on one invocation, over and above the per-value display limits.
#[derive(Clone, Debug)]
pub struct InspectLimits {
	/// Total bytes one command may print, before its truncation marker.
	pub total_bytes: usize,
	/// Most bindings `:vars` may list.
	pub bindings: usize,
	/// Per-value display limits, shared with results.
	pub values: Limits,
}
impl Default for InspectLimits {
	fn default() -> Self {
		Self {
			total_bytes: 8 * 1024,
			bindings: 64,
			values: Limits {
				// A listing shows many values, so each is tighter than a result.
				total_bytes: 1024,
				..Limits::default()
			},
		}
	}
}

/// What an invocation copied, for tests that bound the work rather than
/// only the output.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Work {
	/// Bindings looked up in the session, whether or not they were shown.
	pub entries_visited: usize,
	/// Binding values cloned out of the session.
	pub values_cloned: usize,
	/// Values handed to the renderer. Zero when a budget stopped a command
	/// before it could print one.
	pub values_rendered: usize,
	/// Bytes of stored text read by the escaper.
	pub bytes_escaped: usize,
}

/// An output under a byte budget. Once the budget is reached nothing further
/// is written and the caller stops traversing; the marker names what was cut.
struct Budget<'a> {
	out: String,
	limits: &'a InspectLimits,
	full: bool,
	work: Work,
}
impl<'a> Budget<'a> {
	fn new(limits: &'a InspectLimits) -> Self {
		Self {
			out: String::new(),
			limits,
			full: false,
			work: Work::default(),
		}
	}
	fn remaining(&self) -> usize {
		self.limits.total_bytes.saturating_sub(self.out.len())
	}
	/// Append, or refuse and mark the output full.
	fn push(&mut self, text: &str) -> bool {
		if self.full || text.len() > self.remaining() {
			self.full = true;
			return false;
		}
		self.out.push_str(text);
		true
	}
	/// Append stored text, escaped for the terminal, reading only as much of
	/// it as the remaining budget allows.
	fn push_safe(&mut self, text: &str) -> bool {
		if self.full {
			return false;
		}
		let consumed = terminal_safe_into(text, self.remaining(), &mut self.out);
		self.work.bytes_escaped += consumed;
		if consumed < text.len() {
			self.full = true;
			return false;
		}
		true
	}
	fn line(&mut self, text: &str) -> bool {
		self.push(text) && self.push("\n")
	}
	fn finish(mut self, cut: String) -> (String, Work) {
		if self.full {
			self.out.push_str(&format!(
				"…({cut} not shown; {} byte budget reached)\n",
				self.limits.total_bytes
			));
		}
		(self.out, self.work)
	}
}

/// The runtime type of a value, as Rune reports it, by its last path
/// component. Structural: nothing runs.
fn type_of(value: &rune::runtime::Value) -> String {
	type_name(value.type_info().to_string())
}

/// `:vars`: every published binding, in name order, with type and value.
pub fn vars(session: &Session, limits: &InspectLimits) -> String {
	vars_with_work(session, limits).0
}
/// `:vars`, reporting the work it did.
pub fn vars_with_work(session: &Session, limits: &InspectLimits) -> (String, Work) {
	// The total is the count of published names, which costs nothing to ask.
	let total = session.binding_count();
	if total == 0 {
		return (
			"no bindings; :help lists the commands\n".to_owned(),
			Work::default(),
		);
	}
	let mut budget = Budget::new(limits);
	let mut shown = 0;
	// Bindings are visited lazily and the walk stops at whichever bound comes
	// first, the binding count or the byte budget, so nothing past the last
	// line printed is looked up.
	let visited = session.visit_bindings(|name, value| {
		if shown >= limits.bindings {
			return false;
		}
		budget.work.values_rendered += 1;
		let line = format!(
			"{name}: {} = {}",
			type_of(value),
			render(value, Some(session), &limits.values)
		);
		if !budget.line(&line) {
			return false;
		}
		shown += 1;
		true
	});
	budget.work.entries_visited = visited;
	let cut = total.saturating_sub(shown);
	if cut > 0 && !budget.full {
		// Stopped by the binding count rather than the byte budget.
		let marker = format!(
			"…({cut} more binding{} not shown; {} listed per invocation)\n",
			if cut == 1 { "" } else { "s" },
			limits.bindings
		);
		budget.out.push_str(&marker);
		let work = budget.work.clone();
		return (budget.out, work);
	}
	budget.finish(format!("{cut} of {total} bindings"))
}

/// `:help <name>`, or the command listing when `name` is `None`.
pub fn help(
	session: &Session,
	host: &[HostFunction],
	commands: &[(&str, &str)],
	name: Option<&str>,
	limits: &InspectLimits,
) -> String {
	help_with_work(session, host, commands, name, limits).0
}
/// `:help`, reporting the work it did.
pub fn help_with_work(
	session: &Session,
	host: &[HostFunction],
	commands: &[(&str, &str)],
	name: Option<&str>,
	limits: &InspectLimits,
) -> (String, Work) {
	let Some(name) = name.map(str::trim).filter(|n| !n.is_empty()) else {
		return command_listing(commands, limits);
	};
	let mut budget = Budget::new(limits);
	// A binding wins over a declaration; the help says when both exist, so
	// the precedence never silently hides one of them.
	if let Some(value) = session.binding(name) {
		budget.work.entries_visited = 1;
		budget.work.values_cloned = 1;
		// The value is rendered only if its header fitted, so a budget already
		// spent does not pay to render something that cannot be printed.
		if budget.line(&format!("{name}: binding"))
			&& budget.line(&format!("  type: {}", type_of(&value)))
		{
			budget.work.values_rendered += 1;
			budget.line(&format!(
				"  value: {}",
				render(&value, Some(session), &limits.values)
			));
			if session.declaration(name).is_some() {
				budget.line("  a declaration of this name also exists");
			}
		}
		return budget.finish("the rest of this description".to_owned());
	}
	if let Some((kind, source)) = session.declaration(name) {
		budget.line(&format!("{name}: {kind}"));
		// Escaped a line at a time into the remaining budget: a source longer
		// than the budget is never escaped whole.
		for line in source.lines() {
			if !budget.push("  ") || !budget.push_safe(line) || !budget.push("\n") {
				break;
			}
		}
		return budget.finish("the rest of this declaration".to_owned());
	}
	if let Some(function) = host.iter().find(|f| f.path == name) {
		budget.line(&format!("{}: host function", function.path));
		budget.push("  ");
		budget.push_safe(function.doc);
		budget.push("\n");
		return budget.finish("the rest of this description".to_owned());
	}
	if let Some((command, doc)) = commands.iter().find(|(c, _)| *c == name) {
		budget.line(&format!("{command}: session command"));
		budget.push("  ");
		budget.push_safe(doc);
		budget.push("\n");
		return budget.finish("the rest of this description".to_owned());
	}
	// The name is the person's own text and may be any length: it goes
	// through the same bounded, escaping writer as everything else.
	budget.push_safe(name);
	budget.push(": no binding, declaration, host function, or command of that name\n");
	budget.finish("the rest of this name".to_owned())
}

fn command_listing(commands: &[(&str, &str)], limits: &InspectLimits) -> (String, Work) {
	let mut budget = Budget::new(limits);
	let mut shown = 0;
	budget.line("session commands:");
	for (command, doc) in commands {
		if !budget.push("  ")
			|| !budget.push(command)
			|| !budget.push("  ")
			|| !budget.push_safe(doc)
			|| !budget.push("\n")
		{
			break;
		}
		shown += 1;
	}
	budget.line(":help <name> describes one binding, declaration, host function, or command");
	budget.line(":vars lists the bindings");
	budget.finish(format!(
		"{} of {} commands",
		commands.len() - shown,
		commands.len()
	))
}

#[cfg(test)]
mod tests {
	use super::*;
	use rune::Context;

	fn context() -> Context {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		context
	}
	fn host() -> Vec<HostFunction> {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap()
	}
	const COMMANDS: [(&str, &str); 6] = crate::repl::COMMANDS;

	#[test]
	fn vars_lists_bindings_in_name_order_with_type_and_value() {
		let mut session = Session::new(context()).unwrap();
		let empty = vars(&session, &InspectLimits::default());
		assert!(empty.starts_with("no bindings"), "{empty}");
		session
			.eval(
				"struct P { x } let zed = 1; let alpha = \"s\"; let mid = [1, 2]; let point = P { x: 3 };",
			)
			.unwrap();
		let out = vars(&session, &InspectLimits::default());
		let names: Vec<&str> = out.lines().map(|l| l.split(':').next().unwrap()).collect();
		assert_eq!(names, vec!["alpha", "mid", "point", "zed"]);
		assert!(out.contains("alpha: String = \"s\""), "{out}");
		assert!(out.contains("mid: Vec = [1, 2]"), "{out}");
		assert!(out.contains("point: P = P {x: 3}"), "{out}");
		assert!(out.contains("zed: i64 = 1"), "{out}");
	}

	#[test]
	fn help_answers_for_each_kind_and_states_the_precedence() {
		let host = host();
		let mut session = Session::new(context()).unwrap();
		session
			.eval("fn total(xs) {\n  xs.len()\n} struct P { x } let v = 7;")
			.unwrap();
		let limits = InspectLimits::default();
		let out = help(&session, &host, &COMMANDS, Some("v"), &limits);
		assert!(
			out.contains("v: binding") && out.contains("type: i64") && out.contains("value: 7"),
			"{out}"
		);
		let out = help(&session, &host, &COMMANDS, Some("total"), &limits);
		assert!(out.contains("total: function"), "{out}");
		assert!(
			out.contains("  fn total(xs) {") && out.contains("    xs.len()"),
			"{out}"
		);
		let out = help(&session, &host, &COMMANDS, Some("P"), &limits);
		assert!(
			out.contains("P: struct") && out.contains("struct P { x }"),
			"{out}"
		);
		let out = help(&session, &host, &COMMANDS, Some("host::write_new"), &limits);
		assert!(
			out.contains("host function") && out.contains("refusing to overwrite"),
			"{out}"
		);
		let out = help(&session, &host, &COMMANDS, Some(":memory"), &limits);
		assert!(out.contains("session command"), "{out}");
		let out = help(&session, &host, &COMMANDS, Some("nope"), &limits);
		assert!(
			out.contains("no binding, declaration, host function, or command"),
			"{out}"
		);
		let out = help(&session, &host, &COMMANDS, None, &limits);
		assert!(
			out.contains("session commands:")
				&& out.contains(":quit")
				&& out.contains(":vars lists"),
			"{out}"
		);
	}

	#[test]
	fn a_name_that_is_both_reports_the_binding_and_says_the_declaration_exists() {
		let host = host();
		let mut session = Session::new(context()).unwrap();
		session.eval("fn same(x) { x } let same = 5;").unwrap();
		let out = help(
			&session,
			&host,
			&COMMANDS,
			Some("same"),
			&InspectLimits::default(),
		);
		assert!(out.starts_with("same: binding"), "{out}");
		assert!(
			out.contains("a declaration of this name also exists"),
			"{out}"
		);
	}

	#[test]
	fn the_invocation_budget_bounds_every_command() {
		let host = host();
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		// Many small bindings: cut by the binding count, then by bytes.
		let mut input = String::new();
		for i in 0..200 {
			input.push_str(&format!("let name_{i:03} = {i}; "));
		}
		session.eval(&input).unwrap();
		let out = vars(&session, &InspectLimits::default());
		assert!(
			out.ends_with("…(136 more bindings not shown; 64 listed per invocation)\n"),
			"{out}"
		);
		let tight = InspectLimits {
			total_bytes: 200,
			..InspectLimits::default()
		};
		let out = vars(&session, &tight);
		assert!(
			out.contains("bindings not shown") && out.contains("byte budget reached"),
			"{out}"
		);
		assert!(out.len() < 200 + 96, "{}", out.len());
		// One binding whose rendering alone exceeds the budget.
		session
			.eval("let big = []; for i in 0..500 { big.push(i) }")
			.unwrap();
		let out = vars(&session, &tight);
		assert!(out.len() < 200 + 96, "{}", out.len());
		// A declaration longer than the budget.
		let mut long = String::from("fn long() {\n");
		for i in 0..200 {
			long.push_str(&format!("  let a_{i} = {i};\n"));
		}
		long.push('}');
		session.eval(&long).unwrap();
		let out = help(&session, &host, &COMMANDS, Some("long"), &tight);
		assert!(
			out.contains("this declaration") && out.contains("byte budget reached"),
			"{out}"
		);
		assert!(out.len() < 200 + 96, "{}", out.len());
		// The command listing obeys it too.
		let out = help(
			&session,
			&host,
			&COMMANDS,
			None,
			&InspectLimits {
				total_bytes: 60,
				..tight
			},
		);
		assert!(out.contains("commands not shown"), "{out}");
	}

	#[test]
	fn declaration_source_is_escaped_for_the_terminal() {
		// The declarations are never called; only their source is displayed.
		let host = host();
		let mut session = Session::new(context()).unwrap();
		session
			.eval("fn in_string() {\n  let clear = \"\u{1b}[2J\";\n  clear\n}")
			.unwrap();
		session
			.eval("fn in_comment() {\n  // \u{1b}[2J wipes the screen\n  1\n}")
			.unwrap();
		let limits = InspectLimits::default();
		for name in ["in_string", "in_comment"] {
			let out = help(&session, &host, &COMMANDS, Some(name), &limits);
			assert!(
				!out.contains('\u{1b}'),
				"{name}: an escape reached the output"
			);
			assert!(out.contains("\\u{1b}[2J"), "{name}: {out}");
			// Line breaks and indentation survive.
			assert!(out.lines().count() >= 4, "{name}: {out}");
			assert!(out.lines().any(|l| l.starts_with("    ")), "{name}: {out}");
		}
	}

	#[test]
	fn a_failed_input_leaves_its_mutation_visible_and_its_binding_absent() {
		let host = host();
		let mut session = Session::new(context()).unwrap();
		session.eval("let shared = [1];").unwrap();
		assert!(
			session
				.eval("shared.push(2); let fresh = 9; panic!(\"stop\");")
				.is_err()
		);
		let limits = InspectLimits::default();
		let out = vars(&session, &limits);
		// The first release record's failure rule, seen through these commands.
		assert!(out.contains("shared: Vec = [1, 2]"), "{out}");
		assert!(!out.contains("fresh"), "{out}");
		let out = help(&session, &host, &COMMANDS, Some("shared"), &limits);
		assert!(out.contains("[1, 2]"), "{out}");
		let out = help(&session, &host, &COMMANDS, Some("fresh"), &limits);
		assert!(out.contains("no binding, declaration"), "{out}");
	}

	#[test]
	fn both_commands_answer_when_the_session_is_over_its_ceiling() {
		let host = host();
		// A ceiling of one byte: any sample is at or above it, so the latch
		// trips without depending on what the process has allocated.
		let mut session = Session::with_ceiling(context(), 1).unwrap();
		session.eval("let kept = 1;").unwrap();
		session.sample();
		assert!(
			session.eval("kept").is_err(),
			"expected the ceiling to refuse evaluation"
		);
		let limits = InspectLimits::default();
		assert!(vars(&session, &limits).contains("kept: i64 = 1"));
		assert!(help(&session, &host, &COMMANDS, Some("kept"), &limits).contains("binding"));
	}

	#[test]
	fn every_registered_host_function_has_a_description() {
		let host = host();
		assert_eq!(host.len(), 7);
		for function in &host {
			assert!(
				!function.doc.trim().is_empty(),
				"{} has no description",
				function.path
			);
			assert!(
				function
					.doc
					.starts_with(function.path.trim_start_matches("host::")),
				"{}: description does not name the function: {}",
				function.path,
				function.doc
			);
		}
	}

	#[test]
	fn inspection_runs_no_code() {
		let host = host();
		let mut session = Session::new(context()).unwrap();
		let effect = std::env::temp_dir().join(format!("rnx-inspect-{}", std::process::id()));
		let _ = std::fs::remove_file(&effect);
		let quoted = serde_json::to_string(&effect.to_string_lossy()).unwrap();
		session
			.eval(&format!(
				"let boom = || host::write_new({quoted}, \"ran\");"
			))
			.unwrap();
		let limits = InspectLimits::default();
		let out = vars(&session, &limits);
		assert!(out.contains("boom: "), "{out}");
		let _ = help(&session, &host, &COMMANDS, Some("boom"), &limits);
		let _ = help(&session, &host, &COMMANDS, Some("host::write_new"), &limits);
		assert!(!effect.exists(), "inspection executed the closure");
	}
}

#[cfg(test)]
mod budget_tests {
	use super::*;
	use rune::Context;

	fn context() -> Context {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap();
		context
	}
	fn host() -> Vec<HostFunction> {
		let mut context = Context::with_default_modules().unwrap();
		crate::host::install(&mut context).unwrap()
	}
	const COMMANDS: [(&str, &str); 6] = crate::repl::COMMANDS;

	#[test]
	fn an_oversized_unknown_name_is_bounded_and_escaped() {
		let host = host();
		let session = Session::new(context()).unwrap();
		let limits = InspectLimits::default();
		// A 20,000-character unknown name, with an escape byte in it.
		let name = format!("{}\u{1b}[2J{}", "n".repeat(10_000), "m".repeat(10_000));
		let (out, work) = help_with_work(&session, &host, &COMMANDS, Some(&name), &limits);
		assert!(
			out.len() <= limits.total_bytes + 96,
			"output was {} bytes",
			out.len()
		);
		assert!(out.contains("byte budget reached"), "{}", &out[..80]);
		assert!(!out.contains('\u{1b}'));
		// The escaper read only what the budget allowed, not the whole name.
		assert!(
			work.bytes_escaped <= limits.total_bytes,
			"escaped {} bytes",
			work.bytes_escaped
		);
		assert!(work.bytes_escaped < name.len() / 2);
		let _ = context;
	}

	#[test]
	fn vars_visits_only_the_bindings_it_may_list() {
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let mut input = String::new();
		for i in 0..200 {
			input.push_str(&format!("let name_{i:03} = {i}; "));
		}
		session.eval(&input).unwrap();
		// The total costs nothing: it is the published-name count.
		assert_eq!(session.binding_count(), 200);
		// The listing visits one binding past the last it shows, which is the
		// one that tells it to stop, and never the whole session.
		let (out, work) = vars_with_work(&session, &InspectLimits::default());
		assert_eq!(work.entries_visited, 65);
		assert_eq!(work.values_cloned, 0);
		assert!(
			out.ends_with("…(136 more bindings not shown; 64 listed per invocation)\n"),
			"{out}"
		);
		// A tight byte budget stops the walk sooner still.
		let tight = InspectLimits {
			total_bytes: 100,
			bindings: 64,
			..InspectLimits::default()
		};
		let (out, work) = vars_with_work(&session, &tight);
		assert!(
			work.entries_visited < 12,
			"visited {}",
			work.entries_visited
		);
		assert!(out.len() <= tight.total_bytes + 96, "{}", out.len());
		// A stopping visitor is honoured by the session itself.
		let mut seen = 0;
		let visited = session.visit_bindings(|_, _| {
			seen += 1;
			seen < 3
		});
		assert_eq!((seen, visited), (3, 3));
	}

	#[test]
	fn declaration_help_escapes_only_what_fits() {
		let host = host();
		let mut session = Session::new(context()).unwrap();
		session.set_budget(usize::MAX);
		let mut long = String::from("fn long() {\n");
		for i in 0..1000 {
			long.push_str(&format!("  let a_{i} = {i};\n"));
		}
		long.push('}');
		let source_len = long.len();
		session.eval(&long).unwrap();
		let tight = InspectLimits {
			total_bytes: 300,
			..InspectLimits::default()
		};
		let (out, work) = help_with_work(&session, &host, &COMMANDS, Some("long"), &tight);
		assert!(out.len() <= tight.total_bytes + 96, "{}", out.len());
		assert!(out.contains("this declaration"), "{out}");
		// Only what fit was read, not the whole 30 KB declaration.
		assert!(
			work.bytes_escaped <= tight.total_bytes,
			"escaped {} of {source_len}",
			work.bytes_escaped
		);
	}

	#[test]
	fn the_escaper_stops_at_its_budget() {
		let mut out = String::new();
		let text = "x".repeat(1_000_000);
		let consumed = crate::format::terminal_safe_into(&text, 64, &mut out);
		assert_eq!((consumed, out.len()), (64, 64));
		// An escape expands to six bytes, so three of them fit in twenty.
		let mut out = String::new();
		let text = "\u{1b}".repeat(100);
		let consumed = crate::format::terminal_safe_into(&text, 20, &mut out);
		assert_eq!(consumed, 3);
		assert_eq!(out, "\\u{1b}\\u{1b}\\u{1b}");
	}
}

#[cfg(test)]
mod short_circuit_tests {
	use super::*;
	use rune::Context;

	#[test]
	fn binding_help_does_not_render_a_value_it_cannot_print() {
		let mut context = Context::with_default_modules().unwrap();
		let host = crate::host::install(&mut context).unwrap();
		let mut session = Session::new(context).unwrap();
		session.set_budget(usize::MAX);
		session
			.eval("let wide = []; for i in 0..100000 { wide.push(i) }")
			.unwrap();
		// A budget too small for even the header: the value is never rendered,
		// which a huge per-value limit would otherwise make expensive.
		let starved = InspectLimits {
			total_bytes: 4,
			values: Limits {
				total_bytes: 1024 * 1024,
				length: 100_000,
				..Limits::default()
			},
			..InspectLimits::default()
		};
		let (out, work) = help_with_work(
			&session,
			&host,
			&crate::repl::COMMANDS,
			Some("wide"),
			&starved,
		);
		assert!(out.contains("byte budget reached"), "{out}");
		assert!(!out.contains("value:"), "{out}");
		// The renderer was never called: that is the work the budget skipped.
		assert_eq!(work.values_rendered, 0);
		// The control: with room for the header, the value is rendered, so the
		// difference is the call itself, not a measured duration.
		let roomy = InspectLimits {
			total_bytes: 4 * 1024 * 1024,
			..starved
		};
		let (out, work) = help_with_work(
			&session,
			&host,
			&crate::repl::COMMANDS,
			Some("wide"),
			&roomy,
		);
		assert!(
			out.contains("value: [0, 1, 2"),
			"{}",
			&out[..60.min(out.len())]
		);
		assert_eq!(work.values_rendered, 1);
	}
}
