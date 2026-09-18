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

// Opt-in, test-support-only exclusive phase accounting. An ordinary build has
// no clocks or profiling output. Nested spans charge only their own interval.
#[derive(Default)]
struct Profile {
	path: Option<std::path::PathBuf>,
	start: Option<std::time::Instant>,
	last: Option<std::time::Instant>,
	stack: Vec<&'static str>,
	ns: std::collections::BTreeMap<&'static str, u128>,
	calls: std::collections::BTreeMap<&'static str, usize>,
}
thread_local! {static PROFILE:std::cell::RefCell<Profile>=std::cell::RefCell::new(Profile::default());}
impl Profile {
	fn tick(&mut self) {
		let now = std::time::Instant::now();
		if let (Some(last), Some(name)) = (self.last, self.stack.last()) {
			*self.ns.entry(name).or_default() += now.duration_since(last).as_nanos();
		}
		self.last = Some(now);
	}
}
pub(crate) struct InventoryProfile;
pub(crate) fn profile() -> InventoryProfile {
	PROFILE.with(|p| {
		let mut p = p.borrow_mut();
		assert!(p.path.is_none(), "nested inventory profiling");
		if let Some(path) = std::env::var_os("RNX_INVENTORY_PROFILE") {
			let now = std::time::Instant::now();
			*p = Profile {
				path: Some(path.into()),
				start: Some(now),
				last: Some(now),
				stack: vec!["other"],
				..Profile::default()
			};
		}
	});
	InventoryProfile
}
impl Drop for InventoryProfile {
	fn drop(&mut self) {
		PROFILE.with(|p| {
			let mut p = p.take();
			if p.path.is_none() {
				return;
			}
			p.tick();
			let wall = p.last.unwrap().duration_since(p.start.unwrap()).as_nanos();
			assert_eq!(p.ns.values().sum::<u128>(), wall);
			let mut file = std::fs::OpenOptions::new().create(true).append(true)
				.open(p.path.unwrap()).expect("inventory profile output");
			writeln!(file, "{}", serde_json::json!({"pid":std::process::id(),"wall_ns":wall,"phase_ns":p.ns,"phase_calls":p.calls}))
				.expect("inventory profile write");
		});
	}
}
pub(crate) struct Phase(bool);
pub(crate) fn phase(name: &'static str) -> Phase {
	Phase(PROFILE.with(|p| {
		let mut p = p.borrow_mut();
		if p.path.is_none() {
			return false;
		}
		p.tick();
		p.stack.push(name);
		*p.calls.entry(name).or_default() += 1;
		true
	}))
}
impl Drop for Phase {
	fn drop(&mut self) {
		if self.0 {
			PROFILE.with(|p| {
				let mut p = p.borrow_mut();
				p.tick();
				p.stack.pop();
			});
		}
	}
}
