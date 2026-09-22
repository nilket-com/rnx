//! Record 0079 gate 2: runs one probe script in its own process, so the
//! controls that may hang or must catch a signal run under `run.sh`'s watchdog.
//!
//!   callback-probe <script.rn> [--watch-interrupt]
//!
//! Prints the script's `main()` result as JSON on stdout. With
//! `--watch-interrupt` a SIGINT sets a flag (the session's handler) instead
//! of killing the process, and the exit line reports whether it was seen.
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);
/// Milliseconds since the call started when the signal arrived (0 = before it).
static INTERRUPTED_AT_MS: AtomicU64 = AtomicU64::new(0);
static START_MS: AtomicU64 = AtomicU64::new(0);
fn now_ms() -> u64 {
	std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

#[cfg(unix)]
extern "C" fn on_interrupt(_: libc::c_int) {
	INTERRUPTED.store(true, Ordering::Relaxed);
	INTERRUPTED_AT_MS.store(now_ms().saturating_sub(START_MS.load(Ordering::Relaxed)), Ordering::Relaxed);
}

fn main() {
	let args: Vec<String> = std::env::args().collect();
	let path = args.get(1).expect("script path");
	if args.iter().any(|a| a == "--watch-interrupt") {
		#[cfg(unix)]
		unsafe {
			libc::signal(libc::SIGINT, on_interrupt as *const () as libc::sighandler_t);
		}
	}
	if let Ok(pidfile) = std::env::var("PROBE_PIDFILE") {
		std::fs::write(pidfile, std::process::id().to_string()).expect("pidfile");
	}
	let source = std::fs::read_to_string(path).expect("script");
	let script = match callback_probe::Script::compile(&source) {
		Ok(s) => s,
		Err(e) => {
			println!("{}", serde_json::json!({"compile_error": e}));
			std::process::exit(2);
		}
	};
	if args.iter().any(|a| a == "--fork-survivor") {
		// the watchdog's negative control: a child of this process group outlives it
		let _ = std::process::Command::new("sleep").arg("30").stdout(std::process::Stdio::null()).spawn().expect("sleep");
	}
	START_MS.store(now_ms(), Ordering::Relaxed);
	let started = std::time::Instant::now();
	let result = script.call("main", ());
	let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
	let text = match result {
		Ok(v) => match rune::from_value::<String>(v) {
			Ok(s) => s,
			Err(e) => format!("<non-string result: {e}>"),
		},
		Err(e) => format!("<error: {e}>"),
	};
	println!("{}", serde_json::json!({"result": text, "elapsed_ms": elapsed_ms, "interrupted": INTERRUPTED.load(Ordering::Relaxed), "interrupted_at_ms": INTERRUPTED_AT_MS.load(Ordering::Relaxed), "pool_threads": polars_core::runtime::RAYON.current_num_threads()}));
}
