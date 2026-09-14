//! Record 0039: styling is metadata over text, never part of its byte budget.
use std::borrow::Cow;
use std::io::IsTerminal;
use std::ops::Range;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
	Auto,
	Always,
	Never,
}
impl Mode {
	pub fn parse(value: &str) -> Option<Self> {
		match value {
			"auto" => Some(Self::Auto),
			"always" => Some(Self::Always),
			"never" => Some(Self::Never),
			_ => None,
		}
	}
}
/// Parsed colours carry no arbitrary terminal text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
	Ansi(u8),
	Rgb(u8, u8, u8),
}
impl Colour {
	pub fn parse(text: &str) -> Option<Self> {
		if text.len() == 7
			&& text.starts_with('#')
			&& text.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
		{
			return Some(Self::Rgb(
				u8::from_str_radix(&text[1..3], 16).ok()?,
				u8::from_str_radix(&text[3..5], 16).ok()?,
				u8::from_str_radix(&text[5..7], 16).ok()?,
			));
		}
		let (bright, name) = text
			.strip_prefix("bright-")
			.map_or((false, text), |name| (true, name));
		[
			"black", "red", "green", "yellow", "blue", "magenta", "cyan", "white",
		]
		.iter()
		.position(|&n| n == name)
		.map(|n| Self::Ansi(n as u8 + if bright { 90 } else { 30 }))
	}
	fn foreground(self) -> String {
		match self {
			Self::Ansi(code) => code.to_string(),
			Self::Rgb(r, g, b) => format!("38;2;{r};{g};{b}"),
		}
	}
}
static PALETTE: OnceLock<[Option<String>; 8]> = OnceLock::new();
pub fn set_palette(colours: [Option<Colour>; 6]) {
	let mut styles: [Option<String>; 8] = Default::default();
	for (role, style) in [
		(0, Style::Keyword),
		(1, Style::Literal),
		(2, Style::Number),
		(3, Style::Comment),
		(4, Style::PromptNumber),
		(5, Style::Error),
		(5, Style::Caret),
	] {
		if let Some(colour) = colours[role] {
			let weight = match style {
				Style::Error | Style::PromptNumber => "1;",
				Style::Comment => "2;",
				_ => "",
			};
			styles[style as usize] = Some(format!("\x1b[{weight}{}m", colour.foreground()));
		}
	}
	let _ = PALETTE.set(styles);
}
#[derive(Clone, Copy, Debug)]
struct Policy {
	out: bool,
	err: bool,
}
static POLICY: OnceLock<Policy> = OnceLock::new();

pub fn initialize(mode: Mode) {
	let automatic = std::env::var_os("TERM").is_none_or(|v| v != "dumb")
		&& std::env::var_os("NO_COLOR").is_none_or(|v| v.is_empty());
	let enabled = |terminal| match mode {
		Mode::Always => true,
		Mode::Never => false,
		Mode::Auto => automatic && terminal,
	};
	let out = console(std::io::stdout().is_terminal(), false, mode != Mode::Never);
	let err = console(std::io::stderr().is_terminal(), true, mode != Mode::Never);
	let _ = POLICY.set(Policy {
		out: enabled(out),
		err: enabled(err),
	});
}
pub fn stdout() -> bool {
	POLICY.get().is_some_and(|p| p.out)
}
pub fn stderr() -> bool {
	POLICY.get().is_some_and(|p| p.err)
}
pub fn editor_mode() -> rustyline::ColorMode {
	// Cache the complete per-stream policy, including NO_COLOR and TERM.
	// Forced does not change rustyline's independent reader selection.
	if stdout() {
		rustyline::ColorMode::Forced
	} else {
		rustyline::ColorMode::Disabled
	}
}
#[cfg(not(windows))]
pub(crate) fn console(terminal: bool, _: bool, _: bool) -> bool {
	terminal
}
#[cfg(windows)]
pub(crate) fn console(terminal: bool, stderr: bool, enable: bool) -> bool {
	use windows_sys::Win32::System::Console::*;
	if !terminal || !enable {
		return terminal;
	}
	unsafe {
		let handle = GetStdHandle(if stderr {
			STD_ERROR_HANDLE
		} else {
			STD_OUTPUT_HANDLE
		});
		let mut mode = 0;
		GetConsoleMode(handle, &mut mode) != 0
			&& SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_PROCESSING) != 0
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Style {
	Bold,
	Keyword,
	Literal,
	Number,
	Comment,
	Error,
	Caret,
	PromptNumber,
}
impl Style {
	pub fn sgr(self) -> &'static str {
		if let Some(style) = PALETTE.get().and_then(|styles| styles[self as usize].as_deref()) {
			return style;
		}
		match self {
			Self::Bold => "\x1b[1m",
			Self::Keyword => "\x1b[35m",
			Self::Literal => "\x1b[32m",
			Self::Number => "\x1b[36m",
			Self::Comment => "\x1b[2m",
			Self::Error => "\x1b[1;31m",
			Self::Caret => "\x1b[31m",
			Self::PromptNumber => "\x1b[1;32m",
		}
	}
}
pub const RESET: &str = "\x1b[0m";
#[derive(Debug)]
pub struct Span {
	pub range: Range<usize>,
	pub style: Style,
}
/// Spans are nonoverlapping UTF-8 byte ranges into already-built text.
pub fn paint<'a>(text: &'a str, spans: &[Span], enabled: bool) -> Cow<'a, str> {
	if !enabled || spans.is_empty() {
		return Cow::Borrowed(text);
	}
	let mut out = String::with_capacity(text.len());
	let mut at = 0;
	for span in spans {
		out.push_str(&text[at..span.range.start]);
		out.push_str(span.style.sgr());
		out.push_str(&text[span.range.clone()]);
		out.push_str(RESET);
		at = span.range.end;
	}
	out.push_str(&text[at..]);
	Cow::Owned(out)
}
pub fn styled(text: &str, style: Style, enabled: bool) -> Cow<'_, str> {
	paint(
		text,
		&[Span {
			range: 0..text.len(),
			style,
		}],
		enabled,
	)
}
pub fn error(text: &str) -> Cow<'_, str> {
	styled(text, Style::Error, stderr())
}
pub fn caret(text: &str) -> Cow<'_, str> {
	styled(text, Style::Caret, stderr())
}
/// Inspection has already bounded and escaped its text. Highlight its heading
/// without reparsing descriptions or decorating a script's quoted examples.
pub fn help(text: &str) -> Cow<'_, str> {
	let end = text.find('\n').unwrap_or(text.len());
	paint(
		text,
		&[Span {
			range: 0..end,
			style: Style::Bold,
		}],
		stdout(),
	)
}

/// Tolerant input tokenizer. Only slicing at character boundaries; every pass
/// advances and styles the original bytes, including incomplete literals.
pub fn highlight(text: &str) -> String {
	let mut spans = Vec::new();
	let mut at = 0;
	while at < text.len() {
		let start = at;
		let rest = &text[at..];
		let c = rest.chars().next().unwrap();
		let style;
		if rest.starts_with("//") {
			at += rest.find('\n').unwrap_or(rest.len());
			style = Some(Style::Comment);
		} else if rest.starts_with("/*") {
			at += 2;
			let mut depth = 1usize;
			while at < text.len() && depth != 0 {
				if text[at..].starts_with("/*") {
					depth += 1;
					at += 2;
				} else if text[at..].starts_with("*/") {
					depth -= 1;
					at += 2;
				} else {
					at += text[at..].chars().next().unwrap().len_utf8();
				}
			}
			style = Some(Style::Comment);
		} else if matches!(c, '"' | '\'' | '`') || rest.starts_with("b\"") || rest.starts_with("b'")
		{
			let quote = if c == 'b' {
				at += 1;
				text.as_bytes()[at] as char
			} else {
				c
			};
			at += 1;
			while at < text.len() {
				let ch = text[at..].chars().next().unwrap();
				at += ch.len_utf8();
				if ch == '\\' {
					if let Some(next) = text[at..].chars().next() {
						at += next.len_utf8();
					}
				} else if ch == quote {
					break;
				}
			}
			style = Some(Style::Literal);
		} else if c.is_ascii_digit() {
			at += 1;
			while at < text.len() {
				let ch = text.as_bytes()[at];
				if ch.is_ascii_alphanumeric() || ch == b'_' {
					at += 1;
				} else if ch == b'.' && text.as_bytes().get(at + 1).is_some_and(u8::is_ascii_digit)
				{
					at += 1;
				} else if matches!(ch, b'+' | b'-')
					&& matches!(text.as_bytes()[at - 1], b'e' | b'E')
				{
					at += 1;
				} else {
					break;
				}
			}
			style = Some(Style::Number);
		} else if unicode_ident::is_xid_start(c) || c == '_' {
			at += c.len_utf8();
			while let Some(ch) = text[at..].chars().next() {
				if unicode_ident::is_xid_continue(ch) {
					at += ch.len_utf8();
				} else {
					break;
				}
			}
			style = match &text[start..at] {
				"true" | "false" | "None" | "Some" => Some(Style::Number),
				"abstract" | "alignof" | "become" | "crate" | "default" | "do" | "extern"
				| "final" | "macro" | "mut" | "not" | "offsetof" | "override" | "priv" | "proc"
				| "pure" | "ref" | "sizeof" | "static" | "super" | "typeof" | "unsafe"
				| "virtual" | "as" | "async" | "await" | "break" | "const" | "continue"
				| "else" | "enum" | "fn" | "for" | "if" | "impl" | "in" | "is" | "let" | "loop"
				| "match" | "mod" | "move" | "pub" | "return" | "select" | "self" | "Self"
				| "struct" | "use" | "while" | "yield" => Some(Style::Keyword),
				_ => None,
			};
		} else {
			at += c.len_utf8();
			style = None;
		}
		if let Some(style) = style {
			spans.push(Span {
				range: start..at,
				style,
			});
		}
	}
	paint(text, &spans, true).into_owned()
}

#[cfg(test)]
mod tests {
	use super::*;
	fn unstyle(text: &str) -> String {
		let mut out = text.to_owned();
		for code in [
			RESET,
			"\x1b[1m",
			"\x1b[35m",
			"\x1b[32m",
			"\x1b[36m",
			"\x1b[2m",
			"\x1b[1;31m",
			"\x1b[31m",
		] {
			out = out.replace(code, "");
		}
		out
	}
	#[test]
	fn every_utf8_prefix_keeps_its_bytes_and_finishes_open_tokens() {
		let block =
			"let 界 = [12.5e-3, b\"é\\t\", 'a', `hello ${x}`]; /* outer /* inner */ */ // text\n"
				.repeat(150);
		for end in (0..=block.len()).filter(|&i| block.is_char_boundary(i)) {
			assert_eq!(unstyle(&highlight(&block[..end])), &block[..end]);
		}
		for text in ["\"open\\", "'é", "`open", "/* open /*", "// open"] {
			assert!(highlight(text).ends_with(&format!("{text}{RESET}")));
		}
		for text in ["\t界e\u{301}\n", "\x1b[2J", "println!(\"not executed\")"] {
			assert_eq!(unstyle(&highlight(text)), text);
		}
	}
	#[test]
	fn edits_change_token_classes_without_rewriting_text() {
		assert_eq!(highlight("le"), "le");
		assert_eq!(highlight("let\u{301}"), "let\u{301}");
		assert_eq!(highlight("let"), "\x1b[35mlet\x1b[0m");
		assert_eq!(
			highlight("\"x\" + 1"),
			"\x1b[32m\"x\"\x1b[0m + \x1b[36m1\x1b[0m"
		);
		assert_eq!(highlight("\"x + 1"), "\x1b[32m\"x + 1\x1b[0m");
		assert_eq!(highlight("1..3"), "\x1b[36m1\x1b[0m..\x1b[36m3\x1b[0m");
	}
}
