//! Thread-local Rune values and slots never cross an HTTP worker boundary.
use super::{
	log::Log,
	options::Options,
	value::{Input, Output},
};
use crate::{
	Extensions,
	server::{Failure, Program, Slot},
};
use std::{
	future::Future,
	panic::{AssertUnwindSafe, catch_unwind},
	pin::Pin,
	task::{Context, Poll},
	thread,
};
use tokio::{
	sync::{mpsc, oneshot, watch},
	task::{JoinSet, LocalSet},
	time::Instant,
};
pub(super) type Factory = std::sync::Arc<dyn Fn() -> Extensions + Send + Sync>;
pub(super) const ACTIVE: usize = 4;
pub(super) const QUEUE: usize = 16;
pub(super) struct Job {
	pub input: Input,
	pub reply: oneshot::Sender<Output>,
	pub deadline: Instant,
}
pub(super) struct Catch<F>(pub Pin<Box<F>>);
impl<F: Future> Future for Catch<F> {
	type Output = Result<F::Output, ()>;
	fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
		match crate::extensions::build_catching(|| Ok(self.0.as_mut().poll(cx))) {
			Ok(Poll::Ready(v)) => Poll::Ready(Ok(v)),
			Ok(Poll::Pending) => Poll::Pending,
			Err(_) => Poll::Ready(Err(())),
		}
	}
}
/// VM/native messages can include request data. Log category and source coordinates only.
fn diagnosis(failure: &Failure) -> String {
	match failure.position() {
		Some((line, column)) => format!("{} at source {line}:{column}", failure.category()),
		None => failure.category().to_owned(),
	}
}
/// Everything owning request Values is inside the caught future, and is disposed before replacement.
async fn invoke(slot: Slot, mut job: Job, budget: usize, log: Log) -> Option<Slot> {
	if job.reply.is_closed() {
		let _ = job.reply.send(Output::error(499));
		return Some(slot);
	}
	if Instant::now() >= job.deadline {
		let _ = job.reply.send(Output::error(504));
		return Some(slot);
	}
	let handler = job
		.input
		.routing
		.as_ref()
		.and_then(|r| r.handler.as_deref())
		.map(str::to_owned);
	let expected = job.input.routing.as_ref().and_then(|r| r.status);
	let allow = job.input.routing.as_ref().and_then(|r| r.allow.clone());
	let args = match job.input.rune() {
		Ok(v) => vec![v],
		Err(e) => {
			log.event("request_value_error", &e);
			let _ = job.reply.send(Output::error(500));
			return Some(slot);
		}
	};
	let mut call = match slot.prepare(handler.as_deref().unwrap_or("main"), args, budget) {
		Ok(c) => c,
		Err((e, slot)) => {
			log.event("prepare_error", &diagnosis(&e));
			let _ = job.reply.send(Output::error(500));
			return slot;
		}
	};
	let result = tokio::select! {
		result = call.run() => result.map_err(|e| diagnosis(&e)).and_then(|v| Output::handler(v).map_err(|e|e.redacted())).and_then(|o|o.routed(expected,allow.as_deref())),
		_ = tokio::time::sleep_until(job.deadline) => Ok(Output::error(504)),
		_ = job.reply.closed() => Ok(Output::error(499)),
	}; // The run future is dropped, and returned Values decoded/dropped, before close.
	let (slot, output) = match call.close() {
		Ok(s) => (Some(s), result),
		Err((e, s)) => {
			log.event("invocation_error", &diagnosis(&e));
			// Cancellation is the expected close result after dropping a timed-out
			// or disconnected run future. Preserve its transport result only when
			// the slot is healthy; cleanup failures still take precedence.
			let expected = e.category() == "cancelled"
				&& s.is_some()
				&& result.as_ref().is_ok_and(|o| matches!(o.status, 504 | 499));
			(s, if expected { result } else { Err(diagnosis(&e)) })
		}
	};
	let output = match output {
		Ok(o) => o,
		Err(e) => {
			log.event("handler_error", &e);
			Output::error(500)
		}
	};
	let _ = job.reply.send(output);
	slot
}
pub(super) struct Workers {
	pub senders: Vec<mpsc::Sender<Job>>,
	threads: Vec<thread::JoinHandle<()>>,
}
impl Workers {
	pub fn start(
		program: Program,
		extensions: Factory,
		options: &Options,
		stop: watch::Receiver<Option<Instant>>,
		fatal: watch::Sender<Option<String>>,
		log: Log,
	) -> Result<Self, String> {
		let mut out = Self {
			senders: vec![],
			threads: vec![],
		};
		let (ready_tx, ready_rx) = std::sync::mpsc::channel();
		for n in 0..options.workers {
			let (tx, rx) = mpsc::channel(QUEUE);
			let (program, extensions, stop, fatal, log, ready) = (
				program.clone(),
				extensions.clone(),
				stop.clone(),
				fatal.clone(),
				log.clone(),
				ready_tx.clone(),
			);
			let budget = options.budget;
			let thread = thread::Builder::new()
				.name(format!("rnx-http-worker-{n}"))
				.spawn(move || {
					let result = catch_unwind(AssertUnwindSafe(|| {
						let rt = tokio::runtime::Builder::new_current_thread()
							.enable_all()
							.build()
							.map_err(|e| e.to_string())?;
						LocalSet::new().block_on(&rt, async {
							let slots = (0..ACTIVE)
								.map(|_| program.slot(extensions()).map_err(|e| e.to_string()))
								.collect::<Result<Vec<_>, _>>()?;
							ready
								.send(Ok(()))
								.map_err(|_| "startup receiver gone".to_owned())?;
							run(rx, slots, program, extensions, budget, stop, log).await
						})
					}));
					if let Err(e) = result.unwrap_or_else(|_| Err("HTTP worker panicked".into())) {
						let _ = ready.send(Err(e.clone()));
						let _ = fatal.send(Some(e));
					}
				})
				.map_err(|e| e.to_string());
			match thread {
				Ok(t) => {
					out.threads.push(t);
					out.senders.push(tx);
				}
				Err(e) => {
					out.senders.clear();
					out.join();
					return Err(e);
				}
			}
		}
		drop(ready_tx);
		for _ in 0..options.workers {
			if let Err(e) = ready_rx
				.recv()
				.unwrap_or_else(|_| Err("HTTP worker stopped during startup".into()))
			{
				out.senders.clear();
				out.join();
				return Err(e);
			}
		}
		Ok(out)
	}
	pub fn join(self) {
		for t in self.threads {
			let _ = t.join();
		}
	}
}
async fn run(
	mut jobs: mpsc::Receiver<Job>,
	mut slots: Vec<Slot>,
	program: Program,
	extensions: Factory,
	budget: usize,
	mut stop: watch::Receiver<Option<Instant>>,
	log: Log,
) -> Result<(), String> {
	let mut tasks = JoinSet::new();
	let mut deadline = *stop.borrow();
	let mut retired = 0_u64;
	let mut replaced = 0_u64;
	loop {
		if let Some(d) = deadline {
			jobs.close();
			while let Ok(job) = jobs.try_recv() {
				let _ = job.reply.send(Output::error(503));
			}
			if tasks.is_empty() {
				break;
			}
			if Instant::now() >= d {
				tasks.abort_all();
				while tasks.join_next().await.is_some() {}
				break;
			}
		}
		tokio::select! {
			biased;
			changed=stop.changed(), if deadline.is_none()=>{if changed.is_err(){deadline=Some(Instant::now());}else{deadline = *stop.borrow();}},
			_ = async { match deadline {Some(d)=>tokio::time::sleep_until(d).await,None=>std::future::pending::<()>().await} }=>{},
			completed=tasks.join_next(),if !tasks.is_empty()=> {
				let slot=match completed {Some(Ok(Ok(s)))=>s,Some(Ok(Err(())))=>{log.event("native_unwind","request disposed");None},_=>None};
				let slot=match slot {Some(s)=>s,None=>{retired+=1;log.event("slot_retired", &retired.to_string());let s=program.slot(extensions()).map_err(|e|format!("slot replacement failed: {e}"))?;replaced+=1;log.event("slot_replaced", &replaced.to_string());s}};
				slots.push(slot);
			},
			job=jobs.recv(),if deadline.is_none() && !slots.is_empty()=>match job {
				Some(job)=>{let slot=slots.pop().unwrap();tasks.spawn_local(Catch(Box::pin(invoke(slot,job,budget,log.clone()))));},
				None=>{deadline=Some(Instant::now());}
			}
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::rune::Value;
	use std::{
		cell::RefCell,
		sync::{
			Arc,
			atomic::{AtomicUsize, Ordering},
		},
		time::Duration,
	};
	thread_local! {static KEPT:RefCell<Vec<Value>>=const{RefCell::new(vec![])};}
	struct Bomb;
	impl Future for Bomb {
		type Output = Result<(), String>;
		fn poll(self: Pin<&mut Self>, _: &mut Context<'_>) -> Poll<Self::Output> {
			Poll::Pending
		}
	}
	impl Drop for Bomb {
		fn drop(&mut self) {
			panic!("injected tracked destructor failure");
		}
	}
	fn fixture() -> Extensions {
		Extensions::none().with_lifecycle("fixture", |m, scope| {
			m.function("bomb", move || scope.track(Bomb))
				.build()
				.map_err(|e| e.to_string())?;
			m.function("keep", |v: Value| KEPT.with_borrow_mut(|k| k.push(v)))
				.build()
				.map_err(|e| e.to_string())?;
			m.function("panic", || -> i64 { panic!("injected native unwind") })
				.build()
				.map_err(|e| e.to_string())?;
			Ok(vec![])
		})
	}
	const SOURCE: &str = r#"pub async fn main(req) {
		if req.path == "/cleanup" { fixture::keep(fixture::bomb()); }
		if req.path == "/panic" { fixture::panic(); }
		if req.path == "/wait" { time::sleep(30).await?; }
		#{status:200,headers:#{},body:"okay"}
	}"#;
	fn job(path: &str) -> (Job, oneshot::Receiver<Output>) {
		let (reply, answer) = oneshot::channel();
		(
			Job {
				input: Input {
					routing: None,
					method: "GET".into(),
					path: path.into(),
					query: None,
					headers: Default::default(),
					body: vec![],
				},
				reply,
				deadline: Instant::now() + Duration::from_secs(3),
			},
			answer,
		)
	}
	#[test]
	fn retired_slot_is_replaced_under_load_and_native_unwind_never_resumes_it() {
		let rt = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap();
		rt.block_on(async {
			let program = Program::compile_source("<retirement>", SOURCE, fixture()).unwrap();
			let count = Arc::new(AtomicUsize::new(0));
			let c = count.clone();
			let factory: Factory = Arc::new(move || {
				c.fetch_add(1, Ordering::SeqCst);
				fixture()
			});
			let (stop, rx) = watch::channel(None);
			let (fatal, _) = watch::channel(None);
			let logger = super::super::log::Logger::new(std::io::sink(), false).unwrap();
			let options =
				Options::parse(&["--workers".into(), "1".into(), "fixture.rn".into()]).unwrap();
			let workers =
				Workers::start(program, factory, &options, rx, fatal, logger.log.clone()).unwrap();
			assert_eq!(count.load(Ordering::SeqCst), 4);
			// An expired queued native-panic job must never enter the handler.
			let (mut expired, answer) = job("/panic");
			expired.deadline = Instant::now() - Duration::from_millis(1);
			workers.senders[0].send(expired).await.unwrap();
			assert_eq!(answer.await.unwrap().status, 504);
			assert_eq!(count.load(Ordering::SeqCst), 4);
			let mut answers = vec![];
			for path in ["/cleanup", "/wait", "/wait", "/wait"] {
				let (j, a) = job(path);
				workers.senders[0].send(j).await.unwrap();
				answers.push(a);
			}
			assert_eq!(answers.remove(0).await.unwrap().status, 500);
			for a in answers {
				assert_eq!(a.await.unwrap().status, 200);
			}
			assert_eq!(count.load(Ordering::SeqCst), 5);
			let (j, a) = job("/panic");
			workers.senders[0].send(j).await.unwrap();
			assert!(a.await.is_err());
			let (j, a) = job("/");
			workers.senders[0].send(j).await.unwrap();
			assert_eq!(a.await.unwrap().status, 200);
			assert_eq!(count.load(Ordering::SeqCst), 6);
			let _ = stop.send(Some(Instant::now()));
			workers.join();
			assert!(!logger.shutdown().detached);
		});
	}
	#[test]
	fn named_route_survives_retirement_and_startup_owners_close_on_failure() {
		let rt = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap();
		rt.block_on(async {
			let source = SOURCE.replace("pub async fn main", "pub async fn handler")
				+ "\npub fn routes() { [(\"GET\",\"/\",\"handler\"),(\"GET\",\"/{part}\",\"handler\")] }";
			let program = Program::compile_source("<named>", &source, fixture()).unwrap();
			let logger = super::super::log::Logger::new(std::io::sink(), false).unwrap();
			let options =
				Options::parse(&["--workers".into(), "1".into(), "fixture.rn".into()]).unwrap();
			let routes = super::super::routes::Routes::load(&program, &options, &logger.log)
				.await
				.unwrap()
				.unwrap();
			let count = Arc::new(AtomicUsize::new(0));
			let c = count.clone();
			let factory: Factory = Arc::new(move || {
				c.fetch_add(1, Ordering::SeqCst);
				fixture()
			});
			let (stop, rx) = watch::channel(None);
			let (fatal, _) = watch::channel(None);
			let workers =
				Workers::start(program, factory, &options, rx, fatal, logger.log.clone()).unwrap();
			for (path, status) in [("/cleanup", 500), ("/", 200)] {
				let (mut j, a) = job(path);
				j.input.routing = Some(routes.select("GET", path));
				workers.senders[0].send(j).await.unwrap();
				assert_eq!(a.await.unwrap().status, status);
			}
			assert_eq!(count.load(Ordering::SeqCst), 5);
			let _ = stop.send(Some(Instant::now()));
			workers.join();
			for (body, want) in [
				("fixture::keep(fixture::bomb()); []", "injected"),
				("fixture::panic(); []", "panicked"),
			] {
				let p = Program::compile_source(
					"<startup-fault>",
					&format!("pub fn routes() {{{body}}}"),
					fixture(),
				)
				.unwrap();
				let e = super::super::routes::Routes::load_with_extensions(
					&p,
					&options,
					&logger.log,
					fixture(),
				)
				.await
				.unwrap_err();
				assert!(e.contains(want), "{e}");
			}
			assert!(!logger.shutdown().detached);
		});
	}

	#[test]
	fn replacement_failure_is_fatal_instead_of_silent_shrinkage() {
		let rt = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap();
		rt.block_on(async {
			let program = Program::compile_source("<replacement>", SOURCE, fixture()).unwrap();
			let count = Arc::new(AtomicUsize::new(0));
			let c = count.clone();
			let factory: Factory = Arc::new(move || {
				if c.fetch_add(1, Ordering::SeqCst) < 4 {
					fixture()
				} else {
					Extensions::none()
						.with("broken", |_| Err("injected construction refusal".into()))
				}
			});
			let (stop, rx) = watch::channel(None);
			let (fatal, mut failures) = watch::channel(None);
			let logger = super::super::log::Logger::new(std::io::sink(), false).unwrap();
			let options =
				Options::parse(&["--workers".into(), "1".into(), "fixture.rn".into()]).unwrap();
			let workers =
				Workers::start(program, factory, &options, rx, fatal, logger.log.clone()).unwrap();
			let (j, a) = job("/cleanup");
			workers.senders[0].send(j).await.unwrap();
			assert_eq!(a.await.unwrap().status, 500);
			tokio::time::timeout(Duration::from_secs(3), failures.changed())
				.await
				.unwrap()
				.unwrap();
			assert!(
				failures
					.borrow()
					.as_ref()
					.unwrap()
					.contains("slot replacement failed")
			);
			let _ = stop.send(Some(Instant::now()));
			workers.join();
			assert!(!logger.shutdown().detached);
		});
	}
}

#[cfg(test)]
mod measurements {
	use super::*;
	use std::time::Duration;
	#[test]
	#[ignore = "retained release measurement; run without concurrent builds or load"]
	fn stock_slot_pool_startup_cost() {
		let program = Program::compile_source(
			"<pool-cost>",
			"pub fn main(r) { #{status:200,headers:#{},body:\"okay\"} }",
			Extensions::none(),
		)
		.unwrap();
		for workers in [1, 2] {
			for sample in 0..10 {
				let logger = super::super::log::Logger::new(std::io::sink(), false).unwrap();
				let (stop, rx) = watch::channel(None);
				let (fatal, _) = watch::channel(None);
				let options =
					Options::parse(&["--workers".into(), workers.to_string(), "fixture.rn".into()])
						.unwrap();
				let before = Instant::now();
				let pool = Workers::start(
					program.clone(),
					std::sync::Arc::new(Extensions::none),
					&options,
					rx,
					fatal,
					logger.log.clone(),
				)
				.unwrap();
				println!(
					"pool_cost workers={workers} slots={} sample={sample} us={}",
					workers * ACTIVE,
					before.elapsed().as_micros()
				);
				let _ = stop.send(Some(Instant::now() + Duration::from_millis(10)));
				pool.join();
				assert!(!logger.shutdown().detached);
			}
		}
	}
}
