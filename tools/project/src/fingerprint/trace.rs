#![allow(dead_code)]
use std::{io::Write, path::Path};
thread_local! {static EVENTS:std::cell::RefCell<Vec<serde_json::Value>>=const {std::cell::RefCell::new(vec![])};}
pub(crate) fn event(v: serde_json::Value) {
	EVENTS.with(|e| e.borrow_mut().push(v));
}
pub(crate) fn take() -> Vec<serde_json::Value> {
	EVENTS.with(|e| e.take())
}
pub(crate) fn between(root: &Path) -> Result<(), String> {
	if std::env::var_os("NESTED_PAUSE_ROOT").is_some_and(|p| Path::new(&p) == root) {
		let marker = std::env::var_os("NESTED_PAUSE").ok_or("missing marker")?;
		let marker = Path::new(&marker);
		std::fs::File::create(marker)
			.and_then(|mut f| f.write_all(b"ready"))
			.map_err(|e| e.to_string())?;
		let start = std::time::Instant::now();
		while !marker.with_extension("release").exists() {
			if start.elapsed().as_secs() > 15 {
				return Err("pause timed out".into());
			}
			std::thread::sleep(std::time::Duration::from_millis(5));
		}
	}
	Ok(())
}
