//! Ports of the Python scheduling controls (test_stage_scheduler.Scheduling).
mod common;
use anyhow::Result;
use job_budget::fifo::Pool;
use rnx_final::child::Done;
use rnx_final::controller::{self, Hooks, Settings, StageCall};
use rnx_final::failure::Environment;
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use rnx_final::scheduler::{ALONE, FIRST, Resources, Targets};
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

struct Dir(PathBuf);
impl Dir {
	fn new() -> Self {
		let path = std::env::temp_dir().join(format!("rnx-final-scheduling-{}", invocation_id()));
		std::fs::create_dir(&path).unwrap();
		Self(path.canonicalize().unwrap())
	}
}
impl Drop for Dir {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

/// Each fake stage records when it ran, and sleeps or fails as told.
struct Fake {
	plan: Vec<(&'static str, Vec<&'static str>)>,
	behavior: BTreeMap<&'static str, (f64, i32)>,
	spans: Mutex<BTreeMap<String, (Instant, Instant)>>,
	weight: usize,
	check_delay: Duration,
	/// Free bytes each admission sees, in turn; the last one repeats.
	free: Mutex<Vec<u64>>,
	environments: Mutex<Vec<(Option<String>, Option<String>)>>,
	/// A stage that borrows tokens from the pool, as Cargo's jobs do, and
	/// returns them while it keeps running: (stage, tokens, held seconds).
	borrow: Option<(&'static str, usize, f64)>,
	/// A stage whose jobserver client runs an exclusive contract case.
	exclusive: Option<&'static str>,
	weights: BTreeMap<&'static str, usize>,
	/// Remove the schedule after the stages, before finalization.
	drop_schedule: bool,
	/// A stage that borrows tokens and returns them mid-stage (ADR 0290).
	returns: Option<Returns>,
	flow: Mutex<Flow>,
	moved: std::sync::Condvar,
	/// The controller has taken the failing returner's end.
	processed: Arc<(Mutex<bool>, std::sync::Condvar)>,
}

/// A running stage `stage` borrows `borrow` tokens as Cargo does; `gate`
/// ends once it has, so the next pass refuses `waiter`; after the
/// controller blocks behind that refusal, `stage` returns `chunks` by raw
/// fifo writes and, when `waiter` should start, stays alive until it has.
#[derive(Clone, Default)]
struct Returns {
	stage: &'static str,
	gate: &'static str,
	waiter: &'static str,
	borrow: usize,
	chunks: Vec<usize>,
	/// Another client takes the gate's returned token before the returns,
	/// and gives it back after them.
	competitor: bool,
	/// The returns are written from inside the controller's
	/// `before_block`, between its refusal and its block.
	in_block: bool,
	/// The controller's watch is on a decoy fifo, whose removal the stage
	/// causes instead of returning: a real IN_IGNORED.
	unlink: bool,
	/// The watch cannot start.
	watch_fails: bool,
	/// Count the fifo's reads from the returns until the controller has
	/// rechecked and blocked again.
	count_reads: bool,
	/// After the chunks, `n` more tokens are written at a forced point of
	/// the controller's handoff, and the watcher's decision on them kept.
	handoff: Option<(Handoff, usize)>,
	/// The stage fails, its borrowed tokens coming back only after the
	/// controller has taken the failure.
	fail: bool,
	expect_start: bool,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Handoff {
	/// The controller has taken the Tokens event, its flag still set.
	Outstanding,
	/// The flag cleared, the recheck has just refused the waiter.
	AfterRecheck,
}

#[derive(Default)]
struct Flow {
	fifo: Option<PathBuf>,
	borrowed: bool,
	gate_returned: bool,
	gate_processed: bool,
	gate_refusals: usize,
	blocks: usize,
	started: BTreeSet<String>,
	written: bool,
	decoy: Option<PathBuf>,
	/// When enough tokens for the waiter were first back.
	enough_at: Option<Instant>,
	returner_returned: bool,
	returner_processed: bool,
	/// Reads of the fifo while too few tokens were back (`count_reads`).
	reads: Option<usize>,
	/// The watcher's decisions, sent or coalesced, in order.
	decisions: Vec<bool>,
	taken_after_written: usize,
	/// The watcher's decision on the handoff's write.
	handoff_decision: Option<bool>,
	timeouts: Vec<String>,
}

/// An inotify watch for reads of `path`, as a raw descriptor.
fn read_watch(path: &Path) -> i32 {
	let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
	// SAFETY: flags only, then a valid descriptor and NUL-terminated path.
	unsafe {
		let fd = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
		assert!(fd >= 0 && libc::inotify_add_watch(fd, name.as_ptr(), libc::IN_ACCESS) >= 0);
		fd
	}
}

/// The read records queued on a `read_watch`, and the descriptor closed.
fn reads(fd: i32) -> usize {
	let mut buffer = [0u8; 4096];
	let mut records = 0;
	loop {
		// SAFETY: a valid descriptor and buffer.
		let n = unsafe { libc::read(fd, buffer.as_mut_ptr().cast(), buffer.len()) };
		if n <= 0 {
			break;
		}
		records += n as usize / std::mem::size_of::<libc::inotify_event>();
	}
	// SAFETY: closing the descriptor this function was given.
	unsafe { libc::close(fd) };
	records
}

impl Fake {
	fn new(
		plan: Vec<(&'static str, Vec<&'static str>)>,
		behavior: &[(&'static str, f64, i32)],
	) -> Self {
		Self {
			plan,
			behavior: behavior.iter().map(|(n, s, c)| (*n, (*s, *c))).collect(),
			spans: Mutex::new(BTreeMap::new()),
			weight: 1,
			check_delay: Duration::ZERO,
			free: Mutex::new(vec![]),
			environments: Mutex::new(vec![]),
			borrow: None,
			exclusive: None,
			weights: BTreeMap::new(),
			drop_schedule: false,
			returns: None,
			flow: Mutex::new(Flow::default()),
			moved: std::sync::Condvar::new(),
			processed: Arc::new((Mutex::new(false), std::sync::Condvar::new())),
		}
	}
	fn span(&self, name: &str) -> Option<(Instant, Instant)> {
		self.spans.lock().unwrap().get(name).copied()
	}
}

impl Hooks for Fake {
	fn head(&self, _: &Path) -> Option<String> {
		Some("c".into())
	}
	fn disk_check(&self, _: &Path, _: &Path, _: &Environment) -> Result<()> {
		Ok(())
	}
	fn select(
		&self,
		_: &Path,
		_: &Path,
		_: Option<&[String]>,
		_: &str,
		_: &Environment,
	) -> Result<Value> {
		let stages: Vec<Value> = self.plan.iter().map(|(n, _)| json!([n, [n]])).collect();
		Ok(common::selection("explicit", json!("c"), json!(stages)))
	}
	fn check_selection(&self, _: &Value, _: &Path, _: &Environment) -> Result<()> {
		std::thread::sleep(self.check_delay);
		// Called after the controller takes a stage's end: the gate's end
		// has now been processed, so the next block follows its pass.
		let mut flow = self.flow.lock().unwrap();
		if flow.gate_returned {
			flow.gate_processed = true;
		}
		if flow.returner_returned {
			flow.returner_processed = true;
			let (lock, moved) = &*self.processed;
			*lock.lock().unwrap() = true;
			moved.notify_all();
		}
		self.moved.notify_all();
		Ok(())
	}
	fn tokens_received(&self) {
		// Any event taken once the chunks are written: its flag is still set.
		let armed = {
			let mut flow = self.flow.lock().unwrap();
			if flow.written {
				flow.taken_after_written += 1;
			}
			flow.written
		};
		self.handoff(Handoff::Outstanding, armed);
	}
	fn after_token_refusal(&self) {
		// A refusal after an event taken since the chunks: the recheck's,
		// its flag already cleared.
		let armed = {
			let mut flow = self.flow.lock().unwrap();
			if flow.gate_processed && !flow.written {
				flow.gate_refusals += 1;
				self.moved.notify_all();
			}
			flow.taken_after_written >= 1
		};
		self.handoff(Handoff::AfterRecheck, armed);
	}
	fn watcher_decided(&self, sent: bool) {
		self.flow.lock().unwrap().decisions.push(sent);
		self.moved.notify_all();
	}
	fn jobserver_watch(&self, pool: &Pool) -> Result<job_budget::fifo::Watch> {
		match &self.returns {
			Some(r) if r.watch_fails => anyhow::bail!("planted watch failure"),
			Some(r) if r.unlink => {
				let decoy = pool.path().with_file_name("decoy.fifo");
				let status = std::process::Command::new("mkfifo").arg(&decoy).status()?;
				anyhow::ensure!(status.success());
				let watch = job_budget::fifo::Watch::new(&decoy)?;
				self.flow.lock().unwrap().decoy = Some(decoy);
				Ok(watch)
			}
			_ => pool.watch(),
		}
	}
	fn before_block(&self) {
		let mut flow = self.flow.lock().unwrap();
		if flow.gate_processed {
			flow.blocks += 1;
			let r = self.returns.as_ref().unwrap();
			if r.in_block && !flow.written {
				write_tokens(flow.fifo.as_ref().unwrap(), &r.chunks);
				flow.written = true;
			}
		}
		self.moved.notify_all();
	}
	fn finalize(
		&self,
		_: &Value,
		_: &Map<String, Value>,
		root: &Path,
		_: &Environment,
	) -> Result<Value> {
		if self.drop_schedule {
			std::fs::remove_file(root.join("final/SCHEDULE.json"))?;
		}
		Ok(common::finalization())
	}
	fn resources(&self, name: &str, _: &[String], _: &Path, _: &Targets) -> Result<Resources> {
		Ok(self
			.plan
			.iter()
			.find(|(n, _)| *n == name)
			.unwrap()
			.1
			.iter()
			.map(|s| s.to_string())
			.collect())
	}
	fn weight(&self, name: &str, _: &[String], _: &Path) -> Result<usize> {
		Ok(self.weights.get(name).copied().unwrap_or(self.weight))
	}
	fn tools_identity(&self, _: &Settings) -> Result<Value> {
		Ok(json!({}))
	}
	fn compact(
		&self,
		_: &Value,
		_: &Map<String, Value>,
		_: &rnx_final::evidence::Owners,
		_: &Value,
	) -> Result<Value> {
		Ok(json!({"schema": 1, "candidate": "c", "roots": []}))
	}
	fn disk_free(&self, path: &Path) -> Result<u64> {
		let mut free = self.free.lock().unwrap();
		match free.len() {
			0 => controller::disk_free(path),
			1 => Ok(free[0]),
			_ => Ok(free.remove(0)),
		}
	}
	fn stage(&self, call: &StageCall) -> Option<Result<Done>> {
		if let Some(r) = &self.returns {
			self.returning(r, call);
		}
		self.environments.lock().unwrap().push((
			std::env::var("GIT_OPTIONAL_LOCKS").ok(),
			call.environment.get("GIT_OPTIONAL_LOCKS").cloned(),
		));
		let (seconds, code) = self
			.behavior
			.get(call.name.as_str())
			.copied()
			.unwrap_or((0.3, 0));
		let start = Instant::now();
		if let Some((_, count, hold)) = self.borrow.filter(|(stage, _, _)| *stage == call.name) {
			// Borrow tokens as a Cargo job does, and return them mid-stage.
			let flags = &call.environment["MAKEFLAGS"];
			let path = flags
				.split("fifo:")
				.nth(1)
				.unwrap()
				.split_whitespace()
				.next()
				.unwrap();
			let mut fifo = std::fs::OpenOptions::new()
				.read(true)
				.write(true)
				.open(path)
				.unwrap();
			let mut tokens = vec![0u8; count];
			std::io::Read::read_exact(&mut fifo, &mut tokens).unwrap();
			std::thread::sleep(Duration::from_secs_f64(hold));
			std::io::Write::write_all(&mut fifo, &tokens).unwrap();
		}
		if self.exclusive == Some(call.name.as_str()) {
			// The case takes every token its stage does not hold; bounded at
			// 30 s, so a deadlock fails instead of hanging.
			let environment = call.environment.clone();
			let (sender, receiver) = std::sync::mpsc::channel();
			std::thread::spawn(move || {
				let cases = [json!({"name": "exclusive", "exclusive": true})];
				let done = rnx_final::contracts::run_cases(
					&cases,
					&|_| {
						std::thread::sleep(Duration::from_millis(500));
						Ok(json!({"status": "PASS"}))
					},
					&environment,
				);
				let _ = sender.send(done.is_ok());
			});
			if receiver.recv_timeout(Duration::from_secs(30)) != Ok(true) {
				return Some(Err(anyhow::anyhow!(
					"the exclusive case never got its tokens"
				)));
			}
		}
		std::thread::sleep(Duration::from_secs_f64(seconds));
		self.spans
			.lock()
			.unwrap()
			.insert(call.name.clone(), (start, Instant::now()));
		Some((|| {
			std::fs::create_dir(&call.directory)?;
			let receipt = call.directory.join("RESULT.json");
			let mut row = common::stage_receipt(code == 0, code, json!([]), json!({}));
			if code != 0 {
				row["error"] = json!("planted failure");
			}
			atomic_json(&receipt, &row)?;
			Ok(Done {
				returncode: code,
				validation: json!({"path": receipt, "sha256": digest(&receipt)?}),
				declared_output: None,
			})
		})())
	}
}

impl Fake {
	/// At the forced point `at`, once, after the chunks were written: write
	/// the handoff's tokens and wait for the watcher to decide on them.
	fn handoff(&self, at: Handoff, armed: bool) {
		let Some(Returns {
			handoff: Some((point, n)),
			..
		}) = &self.returns
		else {
			return;
		};
		let (fifo, before) = {
			let flow = self.flow.lock().unwrap();
			if *point != at || !armed || !flow.written || flow.handoff_decision.is_some() {
				return;
			}
			(flow.fifo.clone().unwrap(), flow.decisions.len())
		};
		write_tokens(&fifo, &[*n]);
		self.until("the watcher decided on the handoff", |f| {
			f.decisions.len() > before
		});
		let mut flow = self.flow.lock().unwrap();
		flow.handoff_decision = flow.decisions.get(before).copied();
	}
}

/// Raw writes to the fifo, one per chunk, as Cargo's jobs return tokens.
fn write_tokens(fifo: &Path, chunks: &[usize]) {
	let mut f = std::fs::OpenOptions::new().write(true).open(fifo).unwrap();
	for &n in chunks {
		std::io::Write::write_all(&mut f, &vec![b'+'; n]).unwrap();
	}
}

impl Fake {
	/// Wait, bounded, until `done` holds of the flow.
	/// Wait, bounded, until `done` holds of the flow; a wait that runs out
	/// is recorded, and fails the control.
	fn until(&self, what: &str, done: impl Fn(&Flow) -> bool) {
		let flow = self.flow.lock().unwrap();
		let (mut flow, _) = self
			.moved
			.wait_timeout_while(flow, Duration::from_secs(20), |f| !done(f))
			.unwrap();
		if !done(&flow) {
			flow.timeouts.push(what.to_owned());
		}
	}

	fn returning(&self, r: &Returns, call: &StageCall) {
		let name = call.name.as_str();
		if name == r.waiter {
			self.flow.lock().unwrap().started.insert(name.into());
			self.moved.notify_all();
		}
		if name == r.gate {
			self.until("the returner borrowed", |f| f.borrowed);
			self.flow.lock().unwrap().gate_returned = true;
		}
		if name != r.stage {
			return;
		}
		let flags = &call.environment["MAKEFLAGS"];
		let path = PathBuf::from(
			flags
				.split("fifo:")
				.nth(1)
				.unwrap()
				.split_whitespace()
				.next()
				.unwrap(),
		);
		let mut fifo = std::fs::OpenOptions::new()
			.read(true)
			.write(true)
			.open(&path)
			.unwrap();
		let mut borrowed = vec![0u8; r.borrow];
		std::io::Read::read_exact(&mut fifo, &mut borrowed).unwrap();
		{
			let mut flow = self.flow.lock().unwrap();
			flow.fifo = Some(path.clone());
			flow.borrowed = true;
		}
		self.moved.notify_all();
		let mut held = vec![0u8; usize::from(r.competitor)];
		// The competitor takes the gate's token as it comes back.
		std::io::Read::read_exact(&mut fifo, &mut held).unwrap();
		self.until("the controller blocked behind the refusal", |f| {
			f.blocks >= 1
		});
		if r.handoff.is_some() {
			// The first refusal follows the gate's completion. Its token
			// notice triggers the second. Wait for that recheck so the
			// first chunk gets its own notice to drive either handoff.
			self.until("the controller rechecked the gate's token notice", |f| {
				f.gate_refusals >= 2
			});
		}
		// Counted before the writes, so a fast wake is not missed.
		let handled = self.flow.lock().unwrap().blocks;
		let watched = r.count_reads.then(|| read_watch(&path));
		if r.unlink {
			let decoy = self.flow.lock().unwrap().decoy.clone().unwrap();
			std::fs::remove_file(decoy).unwrap();
		} else if r.fail {
			// Returned only once the controller has taken the failure; the
			// stage ends now, and the helper waits on the flow.
			let mut back = fifo.try_clone().unwrap();
			self.flow.lock().unwrap().returner_returned = true;
			let processed = self.processed.clone();
			let n = r.borrow;
			std::thread::spawn(move || {
				let (lock, moved) = &*processed;
				let done = lock.lock().unwrap();
				let _ = moved
					.wait_timeout_while(done, Duration::from_secs(20), |d| !*d)
					.unwrap();
				std::io::Write::write_all(&mut back, &vec![b'+'; n]).unwrap();
			});
			return;
		} else if !r.in_block {
			// Written and published under one lock, so no callback sees the
			// write without its flag.
			let mut flow = self.flow.lock().unwrap();
			write_tokens(&path, &r.chunks);
			flow.written = true;
		}
		if r.competitor {
			self.until("the controller took the returns", |f| f.blocks > handled);
			std::io::Write::write_all(&mut fifo, &held).unwrap();
		}
		if r.expect_start {
			self.until("the waiter started", |f| f.started.contains(r.waiter));
		} else if !r.unlink && !r.chunks.is_empty() {
			// Let the controller take every write and block again.
			self.until("the controller took the returns", |f| f.blocks > handled);
		}
		if let Some(fd) = watched {
			// Nothing else reads the fifo now: any read is the controller's.
			self.flow.lock().unwrap().reads = Some(reads(fd));
		}
		// Whatever is still borrowed goes back as the stage ends.
		let returned: usize = if r.unlink {
			0
		} else {
			r.chunks.iter().sum::<usize>() + r.handoff.map_or(0, |(_, n)| n)
		};
		if returned < r.borrow {
			self.flow.lock().unwrap().enough_at = Some(Instant::now());
		}
		std::io::Write::write_all(&mut fifo, &vec![b'+'; r.borrow - returned]).unwrap();
	}
}

fn run(fake: Arc<Fake>, tokens: usize) -> (Value, Dir) {
	let dir = Dir::new();
	let names: Vec<String> = fake.plan.iter().map(|(n, _)| n.to_string()).collect();
	let settings = Settings {
		root: dir.0.clone(),
		out: dir.0.join("final"),
		names: Some(names),
		scope: "explicit".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: fake,
		tokens: Some(tokens),
	};
	(controller::run(&settings).unwrap(), dir)
}

fn overlap(a: (Instant, Instant), b: (Instant, Instant)) -> bool {
	a.0 < b.1 && b.0 < a.1
}

fn names(r: &Value) -> Vec<String> {
	r["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| c["name"].as_str().unwrap().to_owned())
		.collect()
}

#[test]
fn disjoint_stages_overlap_shared_ones_keep_order_and_alone_overlaps_nothing() {
	let fake = Arc::new(Fake::new(
		vec![
			("a", vec!["x"]),
			("b", vec!["y"]),
			("c", vec!["x"]),
			("fixed-route", vec![ALONE]),
			("d", vec!["z"]),
		],
		&[],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	let span = |n| fake.span(n).unwrap();
	assert!(overlap(span("a"), span("b")));
	assert!(!overlap(span("a"), span("c")) && span("a").1 <= span("c").0);
	for name in ["a", "b", "c", "d"] {
		assert!(!overlap(span("fixed-route"), span(name)), "{name}");
	}
	assert_eq!(names(&r), ["a", "b", "c", "fixed-route", "d"]);
}

#[test]
fn first_stages_run_before_everything() {
	let fake = Arc::new(Fake::new(
		vec![
			("commit-discipline", vec![FIRST]),
			("a", vec!["x"]),
			("b", vec!["y"]),
		],
		&[],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS");
	assert!(
		fake.span("commit-discipline").unwrap().1
			<= fake.span("a").unwrap().0.min(fake.span("b").unwrap().0)
	);
}

#[test]
fn a_failure_stops_admission_and_running_stages_keep_receipts() {
	let fake = Arc::new(Fake::new(
		vec![
			("slow", vec!["y"]),
			("broken", vec!["x"]),
			("after", vec!["x"]),
			("other", vec!["z"]),
		],
		&[("slow", 1.0, 0), ("broken", 0.1, 1), ("other", 0.1, 0)],
	));
	let (r, _dir) = run(fake.clone(), 2);
	assert_eq!(
		(r["status"].clone(), r["failed_stage"].clone()),
		(json!("FAIL"), json!("broken"))
	);
	assert!(fake.span("after").is_none() && fake.span("other").is_none());
	let exits: BTreeMap<String, i64> = r["checks"]
		.as_array()
		.unwrap()
		.iter()
		.map(|c| {
			(
				c["name"].as_str().unwrap().to_owned(),
				c["exit"].as_i64().unwrap(),
			)
		})
		.collect();
	assert_eq!(
		exits,
		[("slow".to_string(), 0), ("broken".to_string(), 1)].into()
	);
}

#[test]
fn a_declared_producer_finishes_before_its_consumer() {
	// The catalogue's own producer pair.
	let fake = Arc::new(Fake::new(
		vec![
			("polars-inventories", vec!["x"]),
			("polars-suite", vec!["y"]),
		],
		&[("polars-inventories", 1.0, 0), ("polars-suite", 0.1, 0)],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS");
	assert!(fake.span("polars-inventories").unwrap().1 <= fake.span("polars-suite").unwrap().0);
}

#[test]
fn a_failed_producer_never_runs_its_consumer() {
	let fake = Arc::new(Fake::new(
		vec![
			("polars-inventories", vec!["x"]),
			("polars-suite", vec!["y"]),
		],
		&[("polars-inventories", 0.1, 1)],
	));
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["failed_stage"], "polars-inventories");
	assert!(fake.span("polars-suite").is_none());
}

#[test]
fn a_failure_that_already_returned_stops_the_next_admission() {
	let mut fake = Fake::new(
		vec![("broken", vec!["x"]), ("b", vec!["y"]), ("c", vec!["z"])],
		&[("broken", 0.0, 1), ("b", 0.1, 0), ("c", 0.1, 0)],
	);
	// Each admission rechecks the selection, slowly.
	fake.check_delay = Duration::from_millis(500);
	let fake = Arc::new(fake);
	let (r, _dir) = run(fake.clone(), 8);
	assert_eq!(r["failed_stage"], "broken");
	assert!(fake.span("c").is_none());
}

#[test]
fn a_test_width_above_the_budget_is_refused_before_any_stage() {
	for tokens in [1, 2] {
		let mut fake = Fake::new(vec![("a", vec!["x"])], &[]);
		fake.weight = 4;
		let fake = Arc::new(fake);
		let (r, _dir) = run(fake.clone(), tokens);
		assert!(
			r["error"].as_str().unwrap().contains(&format!(
				"budget of {tokens} tokens is below the test width of a (4)"
			)),
			"{r}"
		);
		assert!(fake.span("a").is_none());
	}
	let mut fake = Fake::new(vec![("a", vec!["x"])], &[]);
	fake.weight = 4;
	assert_eq!(run(Arc::new(fake), 4).0["status"], "PASS");
}

#[test]
fn tokens_bound_running_stages() {
	let fake = Arc::new(Fake::new(
		vec![("a", vec!["x"]), ("b", vec!["y"]), ("c", vec!["z"])],
		&[],
	));
	let (r, _dir) = run(fake.clone(), 1);
	assert_eq!(r["status"], "PASS");
	let mut spans: Vec<(Instant, Instant)> = ["a", "b", "c"]
		.iter()
		.map(|n| fake.span(n).unwrap())
		.collect();
	spans.sort();
	assert!(spans.windows(2).all(|w| w[0].1 <= w[1].0));
}

const GIB: u64 = 1024 * 1024 * 1024;

#[test]
fn disk_short_of_the_estimate_or_the_remaining_estimate_is_refused() {
	let floor = controller::DISK_FLOOR_BYTES;
	let bulk = rnx_final::scheduler::declared_bulk("clippy");
	let fake = Fake::new(vec![("a", vec!["x"]), ("clippy", vec!["x"])], &[]);
	*fake.free.lock().unwrap() = vec![floor + bulk - 1];
	let fake = Arc::new(fake);
	let (r, dir) = run(fake.clone(), 2);
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("disk admission before a"),
		"{r}"
	);
	assert!(fake.span("a").is_none());
	let _ = dir;
	// Enough for both at first; the second stage then finds too little.
	let fake = Fake::new(vec![("a", vec!["x"]), ("clippy", vec!["x"])], &[]);
	*fake.free.lock().unwrap() = vec![floor + bulk, floor + bulk - 1];
	let (r, dir) = run(Arc::new(fake), 2);
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("disk admission before clippy"),
		"{r}"
	);
	let acceptance: Value =
		serde_json::from_slice(&std::fs::read(dir.0.join("final/ACCEPTANCE.json")).unwrap())
			.unwrap();
	// "x" is outside the checkout, so it adds no growth.
	assert_eq!(acceptance["disk"]["estimate_bytes"], json!(floor + bulk));
	assert_eq!(
		acceptance["disk"]["lowest_free_bytes"],
		json!(floor + bulk - 1)
	);
}

#[test]
fn a_cold_target_directory_counts_its_growth() {
	let dir = Dir::new();
	let cold: &'static str = Box::leak(
		dir.0
			.join("adapters/large/target/debug")
			.to_string_lossy()
			.into_owned()
			.into_boxed_str(),
	);
	// The run's root is its own temporary checkout, so the lane is inside it.
	let fake = Fake::new(vec![("suite", vec![cold])], &[]);
	*fake.free.lock().unwrap() =
		vec![controller::DISK_FLOOR_BYTES + rnx_final::scheduler::COLD_BYTES - 1];
	let fake = Arc::new(fake);
	let settings = Settings {
		root: dir.0.clone(),
		out: dir.0.join("final"),
		names: Some(vec!["suite".into()]),
		scope: "explicit".into(),
		fresh: false,
		environment: std::env::vars().collect(),
		guardian: PathBuf::from(env!("CARGO_BIN_EXE_rnx-final")),
		started: Instant::now(),
		bootstrap: None,
		unit: None,
		hooks: fake.clone(),
		tokens: Some(2),
	};
	let r = controller::run(&settings).unwrap();
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("disk admission before suite"),
		"{r}"
	);
	assert!(fake.span("suite").is_none());
	let acceptance: Value =
		serde_json::from_slice(&std::fs::read(dir.0.join("final/ACCEPTANCE.json")).unwrap())
			.unwrap();
	assert_eq!(
		acceptance["disk"]["estimate_bytes"],
		json!(controller::DISK_FLOOR_BYTES + rnx_final::scheduler::COLD_BYTES)
	);
}

#[test]
fn target_growth_is_declared_warm_or_cold() {
	let dir = Dir::new();
	let warm = dir.0.join("warm/debug");
	std::fs::create_dir_all(&warm).unwrap();
	std::fs::write(warm.join("artifact"), vec![b'x'; 1000]).unwrap();
	assert_eq!(
		rnx_final::scheduler::growth(&warm.to_string_lossy(), &dir.0),
		GIB
	);
	assert_eq!(
		rnx_final::scheduler::growth(&dir.0.join("missing/debug").to_string_lossy(), &dir.0),
		rnx_final::scheduler::COLD_BYTES
	);
	assert_eq!(rnx_final::scheduler::growth(ALONE, &dir.0), 0);
}

#[test]
fn every_selected_stage_is_declared_and_resolved() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let targets = Targets::new(&root);
	let commands = rnx_final::catalogue::commands(&root.join("target/unused"), &root).unwrap();
	let plan: BTreeMap<String, Resources> = commands
		.iter()
		.map(|(n, c)| {
			(
				n.clone(),
				rnx_final::scheduler::resources(n, c, &root, &targets).unwrap(),
			)
		})
		.collect();
	let target = |a: &str, b: &str| -> String {
		if b.is_empty() {
			root.join(a).to_string_lossy().into_owned()
		} else {
			root.join(a).join(b).to_string_lossy().into_owned()
		}
	};
	assert_eq!(
		plan["rune-suite"],
		[target("target", "debug"), target("target", "engine-dev")].into()
	);
	assert_eq!(
		plan["polars-suite"],
		[
			target("adapters/polars/target", "debug"),
			target("target/0073", "")
		]
		.into()
	);
	assert!(plan["polars-inventories"].contains(&target("target/0072", "")));
	assert_eq!(
		plan["production-cli-build"],
		plan["production-cli-controls"]
	);
	assert_eq!(
		plan["production-cli-build"],
		[target("adapters/polars/target", "release")].into()
	);
	assert!(plan["package-hegel"].contains(&target("rune/hegel_tests/target", "debug")));
	assert!(plan["format"].is_empty());
	assert_eq!(plan["fixed-route"], [ALONE.to_string()].into());
	assert_eq!(plan["fetch-Cargo.toml"], [FIRST.to_string()].into());
	assert_eq!(
		plan["clippy"],
		[
			target("target/column-clippy-published", "debug"),
			target("target/column-clippy-candidate", "debug")
		]
		.into()
	);
	assert!(
		rnx_final::scheduler::declared(
			"new-stage",
			&[
				"python3".into(),
				"-B".into(),
				"tools/column/new_stage.py".into()
			]
		)
		.unwrap_err()
		.to_string()
		.contains("not declared")
	);
	let by_name: BTreeMap<String, Vec<String>> = commands.into_iter().collect();
	let weights: BTreeMap<&str, usize> = [
		"rune-suite",
		"root-tests",
		"generated-projects",
		"clippy",
		"cross",
		"package-hegel",
		"format",
		"column-host",
		"fixed-route",
	]
	.iter()
	.map(|n| {
		(
			*n,
			rnx_final::scheduler::weight(n, &by_name[*n], &root).unwrap(),
		)
	})
	.collect();
	assert_eq!(
		weights,
		[
			("rune-suite", 4),
			("root-tests", 4),
			("generated-projects", 1),
			("clippy", 1),
			("cross", 4),
			("package-hegel", 4),
			("format", 1),
			("column-host", 4),
			("fixed-route", 1)
		]
		.into()
	);
}

#[test]
fn stages_run_git_without_optional_locks() {
	let fake = Arc::new(Fake::new(vec![("a", vec!["x"])], &[]));
	let (r, _dir) = run(fake.clone(), 2);
	assert_eq!(r["status"], "PASS");
	// The stage's own environment carries it; the test process is untouched.
	assert_eq!(fake.environments.lock().unwrap()[0].1.as_deref(), Some("0"));
}

#[test]
fn python_suites_and_the_screen_get_no_jobserver() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let commands: BTreeMap<String, Vec<String>> =
		rnx_final::catalogue::commands(&root.join("target/unused"), &root)
			.unwrap()
			.into_iter()
			.collect();
	let dir = Dir::new();
	let pool = job_budget::fifo::Pool::create(&dir.0.join("jobserver.fifo"), 4).unwrap();
	for name in [
		"controller-controls",
		"benchmark-protocol",
		"census-refresh-controls",
		"fixed-route",
	] {
		assert!(
			rnx_final::scheduler::stage_environment(name, &commands[name], &pool, 1).is_empty(),
			"{name}"
		);
	}
	let env: BTreeMap<String, String> =
		rnx_final::scheduler::stage_environment("rune-suite", &commands["rune-suite"], &pool, 4)
			.into_iter()
			.collect();
	assert!(env["MAKEFLAGS"].contains("--jobserver-auth=fifo:"));
	assert_eq!(
		(
			env["RNX_FINAL_STAGE_TOKENS"].as_str(),
			env["RUST_TEST_THREADS"].as_str()
		),
		("4", "4")
	);
	pool.close().unwrap();
}

/// The cycle a partial reservation would create: a running stage's
/// jobserver client starts an exclusive case and needs every token
/// its stage does not hold, while a four-wide stage waits beside it. With
/// whole-width admission the wide stage holds nothing while it waits, so the
/// exclusive case runs, its stage ends, and the wide stage follows. The
/// client runs under a 30 s timeout, so a deadlock fails instead of hanging.
#[test]
fn a_waiting_wide_stage_never_starves_a_running_exclusive_case() {
	let mut fake = Fake::new(
		vec![("holder", vec!["x"]), ("wide", vec!["y"])],
		&[("holder", 0.0, 0), ("wide", 0.1, 0)],
	);
	fake.exclusive = Some("holder");
	fake.weights = [("holder", 1), ("wide", 4)].into();
	let fake = Arc::new(fake);
	let started = Instant::now();
	let (r, _dir) = run(fake.clone(), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	assert!(
		started.elapsed() < Duration::from_secs(25),
		"the exclusive case waited on tokens the wide stage held"
	);
	let (holder, wide) = (fake.span("holder").unwrap(), fake.span("wide").unwrap());
	assert!(
		wide.0 >= holder.1,
		"the wide stage ran beside a stage holding a token"
	);
}

fn schedule(dir: &Dir) -> Value {
	serde_json::from_slice(&std::fs::read(dir.0.join("final/SCHEDULE.json")).unwrap()).unwrap()
}

fn waits(s: &Value, stage: &str) -> Vec<(String, Vec<String>)> {
	let trace = s["stages"]
		.as_array()
		.unwrap()
		.iter()
		.find(|t| t["name"] == stage)
		.unwrap();
	trace["waits"]
		.as_array()
		.unwrap()
		.iter()
		.map(|w| {
			(
				w["gate"].as_str().unwrap().to_owned(),
				w["behind"]
					.as_array()
					.map(|b| b.iter().map(|x| x.as_str().unwrap().to_owned()).collect())
					.unwrap_or_default(),
			)
		})
		.collect()
}

fn trace<'a>(s: &'a Value, stage: &str) -> &'a Value {
	s["stages"]
		.as_array()
		.unwrap()
		.iter()
		.find(|t| t["name"] == stage)
		.unwrap()
}

/// A lane, a token refusal and the stage that refusal left unevaluated
/// are each recorded once, in order, as the gates admission reached; the
/// controller's block and what woke it are recorded too.
#[test]
fn lanes_tokens_and_a_queued_stage_are_recorded_once() {
	let mut fake = Fake::new(
		vec![
			("a", vec!["x"]),
			("b", vec!["x"]),
			("c", vec!["y"]),
			("d", vec!["z"]),
		],
		&[("a", 0.6, 0)],
	);
	fake.weights = [("a", 2)].into();
	let (r, dir) = run(Arc::new(fake), 2);
	assert_eq!(r["status"], "PASS", "{r}");
	let s = schedule(&dir);
	assert!(waits(&s, "a").is_empty());
	assert_eq!(
		waits(&s, "b"),
		vec![("lanes".to_string(), vec!["a".to_string()])]
	);
	assert_eq!(waits(&s, "c"), vec![("tokens".to_string(), vec![])]);
	assert_eq!(trace(&s, "c")["waits"][0]["evidence"], json!({"weight": 1}));
	// Queued behind c's refusal; then, once a ends, b and c take both
	// tokens, so d is itself refused them.
	assert_eq!(
		waits(&s, "d"),
		vec![
			("queued".to_string(), vec!["c".to_string()]),
			("tokens".to_string(), vec![])
		]
	);
	// A queued stage was not evaluated: its readiness comes only later.
	assert!(
		trace(&s, "d")["ready"].as_f64().unwrap()
			>= trace(&s, "d")["waits"][0]["at"].as_f64().unwrap()
	);
	for stage in ["a", "b", "c", "d"] {
		assert_eq!(trace(&s, stage)["state"], "passed", "{stage}");
	}
	let events: Vec<(String, Value)> = s["controller"]
		.as_array()
		.unwrap()
		.iter()
		.map(|e| (e["event"].as_str().unwrap().into(), e["by"].clone()))
		.collect();
	assert_eq!(events.first(), Some(&("blocked".to_string(), Value::Null)));
	assert!(
		events.contains(&("woken".to_string(), json!("a"))),
		"{events:?}"
	);
}

/// A declared producer, then the disk, then a stage that runs alone.
#[test]
fn producers_disk_and_alone_waits_are_recorded() {
	// polars-suite waits for its producer polars-inventories.
	let fake = Fake::new(
		vec![
			("polars-inventories", vec!["p"]),
			("polars-suite", vec!["q"]),
		],
		&[("polars-inventories", 0.4, 0)],
	);
	let (r, dir) = run(Arc::new(fake), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	assert_eq!(
		waits(&schedule(&dir), "polars-suite"),
		vec![(
			"producers".to_string(),
			vec!["polars-inventories".to_string()]
		)]
	);
	// The second stage sees too little disk while the first runs.
	let fake = Fake::new(vec![("a", vec!["x"]), ("b", vec!["y"])], &[("a", 0.4, 0)]);
	*fake.free.lock().unwrap() = vec![u64::MAX / 2, 1, u64::MAX / 2];
	let (r, dir) = run(Arc::new(fake), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	let s = schedule(&dir);
	assert_eq!(waits(&s, "b"), vec![("disk".to_string(), vec![])]);
	assert_eq!(
		trace(&s, "b")["waits"][0]["evidence"]["free_bytes"],
		json!(1)
	);
	// A stage that runs alone waits for the running one.
	let fake = Fake::new(vec![("a", vec!["x"]), ("b", vec![ALONE])], &[("a", 0.4, 0)]);
	let (r, dir) = run(Arc::new(fake), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	assert_eq!(
		waits(&schedule(&dir), "b"),
		vec![("alone".to_string(), vec!["a".to_string()])]
	);
}

/// A failed final keeps its schedule: the failed stage ended failed, a
/// stage that never started is marked so and has no end.
#[test]
fn a_failed_final_records_its_stages_states() {
	let mut fake = Fake::new(vec![("a", vec!["x"]), ("b", vec!["x"])], &[("a", 0.1, 1)]);
	fake.weight = 1;
	let (r, dir) = run(Arc::new(fake), 2);
	assert_eq!(r["status"], "FAIL");
	let s = schedule(&dir);
	assert_eq!(trace(&s, "a")["state"], "failed");
	assert!(trace(&s, "a")["ended"].is_f64());
	assert_eq!(trace(&s, "b")["state"], "not started");
	assert!(trace(&s, "b")["started"].is_null() && trace(&s, "b")["ended"].is_null());
	let result: rnx_final::types::FinalResult =
		rnx_final::types::read(&dir.0.join("final/RESULT.json")).unwrap();
	let acceptance: rnx_final::types::Acceptance =
		rnx_final::types::read(&dir.0.join("final/ACCEPTANCE.json")).unwrap();
	rnx_final::evidence::check_schedule(&dir.0.join("final"), &result, &acceptance).unwrap();
}

/// A declared schedule must be the bound one and hold exactly the
/// selection's stages, in order and consistently; a final from before the
/// record declares and binds none.
#[test]
fn a_tampered_missing_or_malformed_schedule_is_refused() {
	use rnx_final::evidence::check_schedule;
	use rnx_final::types::{Acceptance, FinalResult, read};
	let fake = Fake::new(vec![("a", vec!["x"]), ("b", vec!["y"])], &[]);
	let (r, dir) = run(Arc::new(fake), 4);
	assert_eq!(r["status"], "PASS", "{r}");
	let out = dir.0.join("final");
	let result: FinalResult = read(&out.join("RESULT.json")).unwrap();
	let acceptance: Acceptance = read(&out.join("ACCEPTANCE.json")).unwrap();
	check_schedule(&out, &result, &acceptance).unwrap();
	assert_eq!(
		result.records.get("SCHEDULE"),
		Some(&digest(&out.join("SCHEDULE.json")).unwrap())
	);
	// Unbound while declared.
	let mut unbound = result.clone();
	unbound.records.remove("SCHEDULE");
	assert!(check_schedule(&out, &unbound, &acceptance).is_err());
	// Neither declared nor bound: a final from before ADR 0288.
	let mut old = acceptance.clone();
	old.schedule_sha256 = rnx_final::receipt::Field::Absent;
	assert!(check_schedule(&out, &unbound, &old).is_ok());
	// Bound but not declared.
	assert!(check_schedule(&out, &result, &old).is_err());
	let original = std::fs::read(out.join("SCHEDULE.json")).unwrap();
	let mut s: Value = serde_json::from_slice(&original).unwrap();
	for (name, edit) in [
		(
			"a missing stage",
			Box::new(|s: &mut Value| {
				s["stages"].as_array_mut().unwrap().pop();
			}) as Box<dyn Fn(&mut Value)>,
		),
		(
			"an extra stage",
			Box::new(|s: &mut Value| {
				let t = s["stages"][0].clone();
				s["stages"].as_array_mut().unwrap().push(t);
			}),
		),
		(
			"an unknown gate",
			Box::new(|s: &mut Value| {
				s["stages"][0]["waits"] = json!([{"at": 0.0, "gate": "luck"}]);
			}),
		),
		(
			"an end before its start",
			Box::new(|s: &mut Value| {
				s["stages"][0]["ended"] = json!(-1.0);
			}),
		),
		(
			"a state that disagrees",
			Box::new(|s: &mut Value| {
				s["stages"][0]["state"] = json!("not started");
			}),
		),
		(
			"controller events out of order",
			Box::new(|s: &mut Value| {
				s["controller"] =
					json!([{"at": 5.0, "event": "blocked"}, {"at": 1.0, "event": "woken"}]);
			}),
		),
		(
			"woken by an unknown stage",
			Box::new(|s: &mut Value| {
				s["controller"] = json!([{"at": 0.0, "event": "woken", "by": "nobody"}]);
			}),
		),
		(
			"running, ready after its start",
			Box::new(|s: &mut Value| {
				let t = &mut s["stages"][0];
				t["state"] = json!("running");
				t["ended"] = Value::Null;
				t["ready"] = json!(t["started"].as_f64().unwrap() + 1.0);
			}),
		),
		(
			"running, started without ready",
			Box::new(|s: &mut Value| {
				let t = &mut s["stages"][0];
				t["state"] = json!("running");
				t["ended"] = Value::Null;
				t["ready"] = Value::Null;
			}),
		),
		(
			"running, waited after its start",
			Box::new(|s: &mut Value| {
				let t = &mut s["stages"][0];
				t["state"] = json!("running");
				t["ended"] = Value::Null;
				t["waits"] =
					json!([{"at": t["started"].as_f64().unwrap() + 1.0, "gate": "tokens"}]);
			}),
		),
	] {
		let mut edited = s.clone();
		edit(&mut edited);
		atomic_json(&out.join("SCHEDULE.json"), &edited).unwrap();
		// Rebound, so only the content is wrong.
		let mut rebound = result.clone();
		rebound.records.insert(
			"SCHEDULE".into(),
			digest(&out.join("SCHEDULE.json")).unwrap(),
		);
		let mut declared = acceptance.clone();
		declared.schedule_sha256 =
			rnx_final::receipt::Field::Value(digest(&out.join("SCHEDULE.json")).unwrap());
		assert!(check_schedule(&out, &rebound, &declared).is_err(), "{name}");
	}
	s = serde_json::from_slice(&original).unwrap();
	atomic_json(&out.join("SCHEDULE.json"), &s).unwrap();
}

/// A well-formed edit to a failed final's schedule, with neither its
/// declaration nor its binding updated, is refused by the schedule check
/// and by comparison, which does not revalidate a failed final.
#[test]
fn a_failed_finals_stale_schedule_binding_is_refused() {
	use rnx_final::evidence::check_schedule;
	use rnx_final::types::{Acceptance, FinalResult, read};
	let fake = Fake::new(vec![("a", vec!["x"]), ("b", vec!["x"])], &[("a", 0.1, 1)]);
	let (r, dir) = run(Arc::new(fake), 2);
	assert_eq!(r["status"], "FAIL");
	let out = dir.0.join("final");
	let result: FinalResult = read(&out.join("RESULT.json")).unwrap();
	let acceptance: Acceptance = read(&out.join("ACCEPTANCE.json")).unwrap();
	check_schedule(&out, &result, &acceptance).unwrap();
	let before = rnx_final::compare::compare(&out, &out, None).map_err(|e| format!("{e:#}"));
	let mut s = schedule(&dir);
	s["stages"][0]["ended"] = json!(s["stages"][0]["ended"].as_f64().unwrap() + 0.001);
	atomic_json(&out.join("SCHEDULE.json"), &s).unwrap();
	let error = check_schedule(&out, &result, &acceptance).unwrap_err();
	assert!(
		format!("{error:#}").contains("SCHEDULE differs"),
		"{error:#}"
	);
	let compared = rnx_final::compare::compare(&out, &out, None).map_err(|e| format!("{e:#}"));
	assert!(
		compared
			.as_ref()
			.is_err_and(|e| e.contains("SCHEDULE differs")),
		"before {before:?}, after {compared:?}"
	);
}

/// A final that ran its stages owes their schedule: one removed before
/// binding fails the final instead of passing as a pre-schedule final.
#[test]
fn a_missing_schedule_fails_a_final_that_ran_stages() {
	let mut fake = Fake::new(vec![("a", vec!["x"])], &[]);
	fake.drop_schedule = true;
	let (r, dir) = run(Arc::new(fake), 2);
	assert_eq!(r["status"], "FAIL", "{r}");
	assert!(
		r["error"].as_str().unwrap().contains("schedule record"),
		"{r}"
	);
	assert!(r["records"].get("SCHEDULE").is_none());
	let acceptance: Value =
		serde_json::from_slice(&std::fs::read(dir.0.join("final/ACCEPTANCE.json")).unwrap())
			.unwrap();
	assert_eq!(acceptance["status"], "FAIL");
}

/// Too little disk before any stage runs fails the final, and the trace
/// keeps the refusal with its numbers; the stage after it was not evaluated.
#[test]
fn a_disk_refusal_with_nothing_running_is_recorded() {
	let fake = Fake::new(vec![("a", vec!["x"]), ("b", vec!["y"])], &[]);
	*fake.free.lock().unwrap() = vec![1];
	let (r, dir) = run(Arc::new(fake), 2);
	assert_eq!(r["status"], "FAIL", "{r}");
	let s = schedule(&dir);
	assert_eq!(waits(&s, "a"), vec![("disk".to_string(), vec![])]);
	assert_eq!(
		trace(&s, "a")["waits"][0]["evidence"]["free_bytes"],
		json!(1)
	);
	assert_eq!(trace(&s, "a")["state"], "not started");
	assert_eq!(
		waits(&s, "b"),
		vec![("queued".to_string(), vec!["a".to_string()])]
	);
	assert!(trace(&s, "b")["ready"].is_null());
}

/// The plan every token-return control runs: `returner` (weight 1) and
/// `gate` (weight 1) start, `waiter` (weight 4) is refused; capacity 5.
fn returning(r: Returns, codes: &[(&'static str, f64, i32)]) -> (Value, Dir, Arc<Fake>) {
	let mut fake = Fake::new(
		vec![
			("returner", vec!["x"]),
			("gate", vec!["y"]),
			("waiter", vec!["z"]),
		],
		codes,
	);
	fake.weights = [("returner", 1), ("gate", 1), ("waiter", 4)].into();
	fake.returns = Some(r);
	let fake = Arc::new(fake);
	let (result, dir) = run(fake.clone(), 5);
	let timeouts = fake.flow.lock().unwrap().timeouts.clone();
	assert!(timeouts.is_empty(), "readiness waits ran out: {timeouts:?}");
	(result, dir, fake)
}

fn tokens_wakes(s: &Value) -> usize {
	s["controller"]
		.as_array()
		.unwrap()
		.iter()
		.filter(|e| e["by"] == "tokens")
		.count()
}

fn base() -> Returns {
	Returns {
		stage: "returner",
		gate: "gate",
		waiter: "waiter",
		borrow: 3,
		..Default::default()
	}
}

/// Tokens a running stage's Cargo returns by raw writes let the waiting
/// stage start while that stage is still alive. (Before ADR 0290 only a
/// stage's end retried a refusal, so the waiter started after it ended.)
#[test]
fn returned_tokens_start_a_waiting_stage_before_the_returner_ends() {
	let (r, dir, fake) = returning(
		Returns {
			chunks: vec![3],
			expect_start: true,
			..base()
		},
		&[],
	);
	assert_eq!(r["status"], "PASS", "{r}");
	let (returner, waiter) = (fake.span("returner").unwrap(), fake.span("waiter").unwrap());
	assert!(
		waiter.0 < returner.1,
		"the waiter started only after the returner ended"
	);
	let s = schedule(&dir);
	assert_eq!(waits(&s, "waiter").last().unwrap().0, "tokens");
	assert!(tokens_wakes(&s) >= 1);
}

/// Returns that leave too few tokens start nothing, read nothing, and wake
/// the controller at most once per write, its own included; the waiter starts only once
/// enough are back, as the returner ends, with every token conserved.
#[test]
fn too_few_returned_tokens_start_nothing() {
	let (r, dir, fake) = returning(
		Returns {
			chunks: vec![2],
			count_reads: true,
			..base()
		},
		&[],
	);
	assert_eq!(r["status"], "PASS", "{r}");
	// The recheck on too few tokens read nothing, so wrote nothing back.
	assert_eq!(fake.flow.lock().unwrap().reads, Some(0));
	let waiter = fake.span("waiter").unwrap();
	let enough = fake.flow.lock().unwrap().enough_at.unwrap();
	assert!(waiter.0 >= enough, "the waiter started on too few tokens");
	// The writes after the refusal: the gate's lease coming back as it
	// ends, the returned chunk, and the returner's last token as it ends.
	// One wake each at most; an insufficient pass adds none of its own.
	let writes = 3;
	assert!(tokens_wakes(&schedule(&dir)) <= writes);
}

/// A competing client holds the gate's returned token through the
/// returns; the waiter starts only once the competitor gives it back.
#[test]
fn a_competing_client_delays_the_waiter_until_it_returns() {
	let (r, _dir, fake) = returning(
		Returns {
			chunks: vec![3],
			competitor: true,
			expect_start: true,
			..base()
		},
		&[],
	);
	assert_eq!(r["status"], "PASS", "{r}");
	let (returner, waiter) = (fake.span("returner").unwrap(), fake.span("waiter").unwrap());
	assert!(waiter.0 < returner.1);
}

/// A burst of returns, each its own write, before the controller takes
/// its one pending event; and returns written between the controller's
/// refusal and its block. One notification suffices for either.
#[test]
fn a_burst_and_a_return_before_the_block_each_wake_once_enough() {
	for r in [
		Returns {
			chunks: vec![1, 1, 1],
			expect_start: true,
			..base()
		},
		Returns {
			chunks: vec![1, 2],
			expect_start: true,
			..base()
		},
		Returns {
			chunks: vec![3],
			in_block: true,
			expect_start: true,
			..base()
		},
	] {
		let label = format!("{:?} in_block={}", r.chunks, r.in_block);
		let (result, _dir, fake) = returning(r, &[]);
		assert_eq!(result["status"], "PASS", "{label}: {result}");
		let (returner, waiter) = (fake.span("returner").unwrap(), fake.span("waiter").unwrap());
		assert!(waiter.0 < returner.1, "{label}");
	}
}

/// A stage failing while another waits on tokens stops admission: the
/// waiter never starts, even once the tokens come back, and the final
/// ends with the watcher joined.
#[test]
fn a_failure_while_a_stage_waits_on_tokens_stops_admission() {
	let (r, dir, _fake) = returning(
		Returns {
			fail: true,
			..base()
		},
		&[("returner", 0.0, 1)],
	);
	assert_eq!(r["status"], "FAIL", "{r}");
	assert_eq!(trace(&schedule(&dir), "waiter")["state"], "not started");
}

/// A watch the kernel removes (IN_IGNORED, here from a decoy fifo's
/// unlink) ends with a named controller error, and the running stages are
/// drained; a watch that cannot start is an explicit error before any
/// stage.
#[test]
fn a_removed_jobserver_watch_fails_the_final_by_name() {
	let (r, dir, _fake) = returning(
		Returns {
			unlink: true,
			..base()
		},
		&[],
	);
	assert_eq!(r["status"], "FAIL", "{r}");
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("jobserver watch failed"),
		"{r}"
	);
	let s = schedule(&dir);
	assert_eq!(trace(&s, "waiter")["state"], "not started");
	assert_eq!(trace(&s, "returner")["state"], "passed");
	let mut fake = Fake::new(vec![("returner", vec!["x"])], &[]);
	fake.returns = Some(Returns {
		watch_fails: true,
		..base()
	});
	let (r, _dir) = run(Arc::new(fake), 5);
	assert_eq!(r["status"], "FAIL", "{r}");
	assert!(
		r["error"]
			.as_str()
			.unwrap()
			.contains("planted watch failure"),
		"{r}"
	);
	assert!(r["checks"].as_array().unwrap().is_empty(), "{r}");
}

/// A write while the controller holds an outstanding Tokens event, its
/// flag not yet cleared, is coalesced into it, and the recheck that event
/// leads to finds the whole capacity.
#[test]
fn capacity_returned_while_an_event_is_outstanding_is_found_by_its_recheck() {
	let (r, _dir, fake) = returning(
		Returns {
			chunks: vec![1],
			handoff: Some((Handoff::Outstanding, 2)),
			expect_start: true,
			..base()
		},
		&[],
	);
	assert_eq!(r["status"], "PASS", "{r}");
	assert_eq!(
		fake.flow.lock().unwrap().handoff_decision,
		Some(false),
		"the write was not coalesced"
	);
	let (returner, waiter) = (fake.span("returner").unwrap(), fake.span("waiter").unwrap());
	assert!(waiter.0 < returner.1);
}

/// A write after the flag is cleared and the recheck has refused the
/// waiter, before the controller blocks, sends a new event and is not
/// lost.
#[test]
fn capacity_returned_after_the_recheck_is_not_lost() {
	let (r, _dir, fake) = returning(
		Returns {
			chunks: vec![1],
			handoff: Some((Handoff::AfterRecheck, 2)),
			expect_start: true,
			..base()
		},
		&[],
	);
	assert_eq!(r["status"], "PASS", "{r}");
	assert_eq!(
		fake.flow.lock().unwrap().handoff_decision,
		Some(true),
		"the write raised no event"
	);
	let (returner, waiter) = (fake.span("returner").unwrap(), fake.span("waiter").unwrap());
	assert!(waiter.0 < returner.1);
}
