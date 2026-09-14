//! Terminal controls are independent of colour and never sent to a pipe.
use std::io::{IsTerminal, Write};
use std::sync::atomic::{AtomicBool, Ordering};
static TITLE: AtomicBool = AtomicBool::new(false);
fn enabled() -> bool {
	let unsupported = std::env::var_os("TERM").is_some_and(|term| {
		["dumb", "cons25", "emacs"]
			.iter()
			.any(|s| term.to_string_lossy().eq_ignore_ascii_case(s))
	});
	!unsupported && crate::presentation::console(std::io::stdout().is_terminal(), false, true)
}
fn emit(text: &str) {
	let mut out = std::io::stdout().lock();
	let _ = out.write_all(text.as_bytes());
	let _ = out.flush();
}
pub fn clear() {
	if enabled() {
		emit("\x1b[2J\x1b[H");
	}
}
pub struct Title;
impl Title {
	pub fn new(title: &str) -> Self {
		if enabled() {
			let title = crate::format::terminal_safe(title);
			emit(&format!("\x1b[22;2t\x1b]2;{title}\x07"));
			TITLE.store(true, Ordering::Relaxed);
		}
		Self
	}
}
impl Drop for Title {
	fn drop(&mut self) {
		restore();
	}
}
pub fn restore() {
	if TITLE.swap(false, Ordering::Relaxed) {
		emit("\x1b[23;2t");
	}
}
/// Explicit exits bypass Drop; normal returns use the title guard.
pub fn exit(code: i32) -> ! {
	restore();
	std::process::exit(code)
}
