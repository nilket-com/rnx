//! The final's pool is the Rust library's; Cargo joins it as a client.
//! The stage programs' own client is controlled in tests/contracts.rs and
//! the budget crate (ADR 0289).
use job_budget::fifo::Pool;
use rnx_final::receipt::invocation_id;
use std::path::{Path, PathBuf};

struct Dir(PathBuf);
impl Dir {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!("rnx-final-interop-{}", invocation_id()));
		std::fs::create_dir(&path).unwrap();
		Self(path.canonicalize().unwrap())
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

fn workspace(root: &Path) {
	std::fs::write(
		root.join("Cargo.toml"),
		"[workspace]\nmembers=[\"a\",\"b\",\"c\",\"d\"]\nresolver=\"2\"\n",
	)
	.unwrap();
	for n in ["a", "b", "c", "d"] {
		std::fs::create_dir_all(root.join(n).join("src")).unwrap();
		std::fs::write(
			root.join(n).join("Cargo.toml"),
			format!("[package]\nname=\"{n}\"\nversion=\"0.1.0\"\nedition=\"2021\"\n"),
		)
		.unwrap();
		// Each build script records when it ran, so overlap is measured.
		std::fs::write(root.join(n).join("build.rs"), format!("use std::time::*;fn now()->u128{{SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros()}}fn main(){{let s=now();std::thread::sleep(Duration::from_millis(500));std::fs::write(std::path::Path::new(&std::env::var(\"STAMPS\").unwrap()).join(\"{n}\"),format!(\"{{}} {{}}\",s,now())).unwrap();}}\n")).unwrap();
		std::fs::write(root.join(n).join("src/lib.rs"), "").unwrap();
	}
}

fn spans(root: &Path, pool: &Pool, name: &str) -> Vec<(u128, u128)> {
	let stamps = root.join(format!("stamps-{name}"));
	std::fs::create_dir(&stamps).unwrap();
	let status = std::process::Command::new("cargo")
		.args(["+1.98.1", "build", "-q", "--offline"])
		.current_dir(root)
		.env("CARGO_TARGET_DIR", root.join(name))
		.env("CARGO_BUILD_JOBS", "16")
		.env("STAMPS", &stamps)
		.env("MAKEFLAGS", pool.flags())
		.env("CARGO_MAKEFLAGS", pool.flags())
		.status()
		.unwrap();
	assert!(status.success());
	let mut rows: Vec<(u128, u128)> = std::fs::read_dir(&stamps)
		.unwrap()
		.flatten()
		.map(|e| {
			let text = std::fs::read_to_string(e.path()).unwrap();
			let mut parts = text.split_whitespace().map(|p| p.parse().unwrap());
			(parts.next().unwrap(), parts.next().unwrap())
		})
		.collect();
	rows.sort();
	rows
}

#[test]
fn cargo_joins_the_final_pool() {
	let dir = Dir::new();
	workspace(&dir.0);
	let pool = Pool::create(&dir.0.join("jobserver.fifo"), 4).unwrap();
	let held = pool.take(4).unwrap().unwrap();
	assert!(pool.take(1).unwrap().is_none());
	let serial = spans(&dir.0, &pool, "held");
	pool.give(held).unwrap();
	let free = spans(&dir.0, &pool, "free");
	pool.close().unwrap();
	let overlaps = |s: &[(u128, u128)]| s.windows(2).any(|w| w[0].1 > w[1].0);
	// With every token held Cargo has only its implicit job: no overlap.
	assert_eq!(serial.len(), 4);
	assert!(!overlaps(&serial), "{serial:?}");
	assert!(overlaps(&free), "{free:?}");
}
