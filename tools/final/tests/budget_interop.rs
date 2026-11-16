//! The final's pool is the Rust library's; its clients are Cargo and the
//! unchanged Python stage programs. Process controls that they share it.
use job_budget::fifo::Pool;
use rnx_final::receipt::invocation_id;
use std::path::{Path, PathBuf};
use std::time::Duration;

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

fn column() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../../tools/column")
		.canonicalize()
		.unwrap()
}

/// A Python program against the pool, bounded so a deadlock is a failure.
fn python(pool: &Pool, held: usize, script: &str) -> (bool, String) {
	let mut child = std::process::Command::new("python3")
		.args(["-B", "-c", script])
		.current_dir(column())
		.env("MAKEFLAGS", pool.flags())
		.env("RNX_FINAL_STAGE_TOKENS", held.to_string())
		.stdout(std::process::Stdio::piped())
		.stderr(std::process::Stdio::piped())
		.spawn()
		.unwrap();
	for _ in 0..300 {
		if let Some(status) = child.try_wait().unwrap() {
			let output = child.wait_with_output().unwrap();
			return (
				status.success(),
				format!(
					"{}{}",
					String::from_utf8_lossy(&output.stdout),
					String::from_utf8_lossy(&output.stderr)
				),
			);
		}
		std::thread::sleep(Duration::from_millis(100));
	}
	child.kill().unwrap();
	let _ = child.wait();
	(false, "deadlocked: the Python client never finished".into())
}

#[test]
fn a_one_token_pool_still_finishes_the_python_cases() {
	let dir = Dir::new();
	let pool = Pool::create(&dir.0.join("jobserver.fifo"), 1).unwrap();
	// The controller holds the only token for the running stage.
	let stage = pool.take(1).unwrap().unwrap();
	let script = "import time, native_contracts\nfrom unittest.mock import patch\ncases = [{'name': 'exclusive', 'exclusive': True}] + [{'name': f'shared-{i}'} for i in range(3)]\ndef case(c, *a): time.sleep(0.2); return {'name': c['name'], 'status': 'PASS'}\nwith patch.object(native_contracts, 'run_case', side_effect=case), patch('host_budget.case_width', return_value=3):\n    rows = native_contracts.run_cases(cases, None, None, None, None)\nassert [r['name'] for r in rows] == [c['name'] for c in cases], rows\nprint('FINISHED')\n";
	let (ok, output) = python(&pool, 1, script);
	assert!(ok && output.contains("FINISHED"), "{output}");
	pool.give(stage).unwrap();
	// No token was lost or made.
	let all = pool.take(1).unwrap().expect("the one token is back");
	pool.give(all).unwrap();
	assert_eq!(pool.outstanding(), 0);
	pool.close().unwrap();
}

#[test]
fn an_exclusive_python_case_holds_every_token() {
	let dir = Dir::new();
	let pool = Pool::create(&dir.0.join("jobserver.fifo"), 3).unwrap();
	let stage = pool.take(1).unwrap().unwrap();
	// While the exclusive case runs, the Python client holds every other
	// token: this process can take none.
	let marker = dir.0.join("inside");
	let script = format!(
		"import time, native_contracts\nfrom unittest.mock import patch\ndef case(*a):\n    open({:?}, 'w').write('1'); time.sleep(1.0); return {{'status': 'PASS'}}\nwith patch.object(native_contracts, 'run_case', side_effect=case):\n    native_contracts.with_tokens(2, {{'name': 'exclusive'}}, None, None, None, None)\nprint('FINISHED')\n",
		marker.to_string_lossy()
	);
	let pool_ref = &pool;
	let seen = std::thread::scope(|scope| {
		let watcher = scope.spawn(|| {
			for _ in 0..100 {
				if marker.exists() {
					return pool_ref.take(1).unwrap().is_none();
				}
				std::thread::sleep(Duration::from_millis(20));
			}
			false
		});
		let (ok, output) = python(pool_ref, 1, &script);
		assert!(ok, "{output}");
		watcher.join().unwrap()
	});
	assert!(seen, "a token was free while the exclusive case ran");
	pool.give(stage).unwrap();
	let all = pool.take(3).unwrap().expect("all three tokens are back");
	pool.give(all).unwrap();
	pool.close().unwrap();
}

#[test]
fn python_case_workers_share_a_stage_slot_and_the_pool() {
	let dir = Dir::new();
	let pool = Pool::create(&dir.0.join("jobserver.fifo"), 2).unwrap();
	let stage = pool.take(1).unwrap().unwrap();
	let script = "import os, multiprocessing, stage_scheduler as s\nown = os.pipe(); os.write(own[1], b'+')\ntry:\n    s.Client.from_environment().acquire_one(own); raise SystemExit('a blocking slot pipe was accepted')\nexcept ValueError: pass\nos.set_blocking(own[0], False)\ndef take(q):\n    fd, token = s.Client.from_environment().acquire_one(own); q.put(fd == own[0])\nq = multiprocessing.get_context('fork').Queue()\nworkers = [multiprocessing.get_context('fork').Process(target=take, args=(q,)) for _ in range(2)]\nfor w in workers: w.start()\nfor w in workers: w.join(10)\nassert all(w.exitcode == 0 for w in workers), [w.exitcode for w in workers]\nassert sorted(q.get(timeout=1) for _ in range(2)) == [False, True]\nprint('FINISHED')\n";
	let (ok, output) = python(&pool, 1, script);
	assert!(ok && output.contains("FINISHED"), "{output}");
	// One worker took the pool's free token and exited holding it: return it.
	drop(stage);
	assert_eq!(pool.outstanding(), 0);
	pool.close().unwrap();
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
