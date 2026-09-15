//! One worker operation, one IOPub owner, independent control and heartbeat.
use crate::{
	connection::Connection,
	transport::{Handler, Publisher, Res, Route, Server},
	wire::{Codec, Request},
	worker::{Interrupt, StreamEvent, Worker},
};
use serde_json::{Value, json};
use std::{
	collections::VecDeque,
	path::Path,
	sync::{
		Arc, Mutex, OnceLock,
		atomic::{AtomicU64, Ordering::SeqCst},
	},
};
use tokio::{
	sync::{Notify, OwnedSemaphorePermit, Semaphore, watch},
	time::{Duration, Instant, timeout_at},
};
const M: usize = 1024 * 1024;
struct Parent {
	route: Route,
	routing: Vec<Vec<u8>>,
	header: Vec<u8>,
}
impl Parent {
	fn from_request(route: Route, r: &Request<'_>) -> Self {
		Self {
			route,
			routing: r.routing.to_vec(),
			header: r.header_bytes.to_vec(),
		}
	}
}
struct Job {
	parent: Arc<Parent>,
	code: String,
	expressions: Vec<String>,
	silent: bool,
	store: bool,
	stop: bool,
	_credit: OwnedSemaphorePermit,
}
#[derive(Default)]
struct Queue {
	pending: VecDeque<Job>,
	retiring: bool,
}
#[derive(Default)]
struct Book {
	history: VecDeque<(u64, String)>,
	bytes: usize,
	origins: VecDeque<((u64, u64), Option<u64>)>,
}
impl Book {
	fn store(&mut self, count: u64, source: &str) {
		while !self.history.is_empty()
			&& (self.history.len() >= 10000 || self.bytes + source.len() > 16 * M)
		{
			let old = self.history.pop_front().unwrap();
			self.bytes -= old.1.len();
		}
		self.history.push_back((count, source.into()));
		self.bytes += source.len();
	}
	fn map(&mut self, reply: &Value, count: Option<u64>) {
		if let (Some(epoch), Some(input)) = (reply["epoch"].as_u64(), reply["input"].as_u64()) {
			if self.origins.len() == 10000 {
				self.origins.pop_front();
			}
			self.origins.push_back(((epoch, input), count));
		}
	}
	fn metadata(&self, reply: &Value) -> Value {
		let origin = &reply["failure"]["origin"];
		let count = origin["epoch"]
			.as_u64()
			.zip(origin["input"].as_u64())
			.and_then(|key| {
				self.origins
					.iter()
					.rev()
					.find(|(k, _)| *k == key)
					.and_then(|(_, v)| *v)
			});
		json!({"rnx":{"worker_generation":1,"origin":origin,"notebook_count":count}})
	}
}
fn error(name: &str, text: &str) -> Value {
	json!({"status":"error","ename":name,"evalue":text,"traceback":text.lines().collect::<Vec<_>>()})
}
// IOPub error becomes an nbformat output verbatim in JupyterLab. Reply-only
// status/count fields must not leak into that narrower schema.
fn error_output(reply: &Value) -> Value {
	json!({"ename":reply["ename"],"evalue":reply["evalue"],"traceback":reply["traceback"]})
}
fn category(f: &Value) -> &'static str {
	match f["category"].as_str() {
		Some("compile") => "CompileError",
		Some("runtime") => "RuntimeError",
		Some("interrupted") => "Interrupted",
		Some("budget") => "BudgetExhausted",
		Some("over_ceiling") => "AllocationCeiling",
		_ => "Refused",
	}
}
struct Output {
	codec: Codec,
	publisher: OnceLock<Publisher>,
	order: Mutex<()>,
	idle: watch::Sender<bool>,
}
impl Output {
	fn frames(
		&self,
		routing: &[Vec<u8>],
		parent: &[u8],
		kind: &str,
		meta: &Value,
		content: &Value,
	) -> Res<Vec<Vec<u8>>> {
		self.codec.encode(
			routing,
			parent,
			kind,
			&jiff::Timestamp::now().to_string(),
			meta,
			content,
		)
	}
	fn publish_locked(&self, parent: &[u8], kind: &str, meta: &Value, content: &Value) -> Res<()> {
		let frames = self.frames(&[kind.as_bytes().to_vec()], parent, kind, meta, content)?;
		self.publisher
			.get()
			.ok_or("publisher not ready")?
			.publish(&frames)
	}
	fn publish(&self, parent: &[u8], kind: &str, meta: &Value, content: &Value) -> Res<()> {
		let _order = self.order.lock().unwrap();
		self.publish_locked(parent, kind, meta, content)
	}
	fn status(&self, parent: &[u8], status: &str) -> Res<()> {
		let _order = self.order.lock().unwrap();
		self.idle.send_replace(status == "idle");
		self.publish_locked(
			parent,
			"status",
			&json!({}),
			&json!({"execution_state":status}),
		)
	}
	// A shell kernel-info future in JupyterLab needs its own idle. Delay the whole
	// response until execution is idle, retaining the transport's receive credit.
	// The order lock closes the race with the next cell beginning execution.
	async fn shell_info(&self, parent: &Parent, content: &Value) -> Res<()> {
		let mut idle = self.idle.subscribe();
		let subscription_end = Instant::now() + Duration::from_secs(2);
		while !self
			.publisher
			.get()
			.ok_or("publisher not ready")?
			.has_subscriber(b"status")
		{
			if Instant::now() >= subscription_end {
				break;
			}
			tokio::time::sleep(Duration::from_millis(5)).await;
		}
		loop {
			{
				let _order = self.order.lock().unwrap();
				if *idle.borrow_and_update() {
					self.publish_locked(
						&parent.header,
						"status",
						&json!({}),
						&json!({"execution_state":"busy"}),
					)?;
					self.reply(parent, "kernel_info_reply", &json!({}), content)?;
					return self.publish_locked(
						&parent.header,
						"status",
						&json!({}),
						&json!({"execution_state":"idle"}),
					);
				}
			}
			idle.changed().await?;
		}
	}

	fn control_info(&self, parent: &Parent, content: &Value) -> Res<()> {
		let _order = self.order.lock().unwrap();
		let idle = *self.idle.borrow();
		if idle {
			self.publish_locked(
				&parent.header,
				"status",
				&json!({}),
				&json!({"execution_state":"busy"}),
			)?;
		}
		self.reply(parent, "kernel_info_reply", &json!({}), content)?;
		if idle {
			self.publish_locked(
				&parent.header,
				"status",
				&json!({}),
				&json!({"execution_state":"idle"}),
			)?;
		}
		Ok(())
	}

	fn reply(&self, parent: &Parent, kind: &str, meta: &Value, content: &Value) -> Res<()> {
		if parent.route.is_closed() {
			return Ok(());
		}
		parent
			.route
			.reply(&self.frames(&parent.routing, &parent.header, kind, meta, content)?)
	}
	fn stream(&self, parent: &Parent, silent: bool, event: StreamEvent) -> Res<()> {
		if silent && !event.unassociated {
			return Ok(());
		}
		self.publish(
			if event.unassociated {
				b"{}"
			} else {
				&parent.header
			},
			"stream",
			&json!({"rnx":{"utf8_replaced":event.replaced}}),
			&json!({"name":event.name,"text":event.text}),
		)
	}
	fn refusal(
		&self,
		parent: &Parent,
		silent: bool,
		name: &str,
		text: &str,
		count: u64,
	) -> Res<()> {
		let _order = self.order.lock().unwrap();
		self.publish_locked(
			&parent.header,
			"status",
			&json!({}),
			&json!({"execution_state":"busy"}),
		)?;
		let mut failure = error(name, text);
		failure["execution_count"] = json!(count);
		if !silent {
			self.publish_locked(&parent.header, "error", &json!({}), &error_output(&failure))?;
		}
		self.reply(parent, "execute_reply", &json!({}), &failure)?;
		self.publish_locked(
			&parent.header,
			"status",
			&json!({}),
			&json!({"execution_state":"idle"}),
		)
	}
}
struct App {
	out: Arc<Output>,
	queue: Mutex<Queue>,
	bytes: Arc<Semaphore>,
	notify: Notify,
	interrupt: Arc<Interrupt>,
	stop: watch::Sender<Option<Instant>>,
	replied: watch::Sender<bool>,
	ready: watch::Sender<bool>,
	count: AtomicU64,
	book: Mutex<Book>,
}
fn flag(value: &Value, name: &str, default: bool) -> Res<bool> {
	match value.get(name) {
		None => Ok(default),
		Some(v) => v
			.as_bool()
			.ok_or_else(|| format!("{name} must be a boolean").into()),
	}
}
struct Options<'a> {
	code: &'a str,
	keys: Vec<&'a str>,
	silent: bool,
	store: bool,
	stop: bool,
	weight: usize,
}
fn options<'a>(request: &'a Request<'_>, payload_bytes: usize) -> Res<Options<'a>> {
	let v = &request.content;
	let code = v["code"].as_str().ok_or("code must be a string")?;
	let silent = flag(v, "silent", false)?;
	let store = flag(v, "store_history", true)? && !silent;
	let stop = flag(v, "stop_on_error", true)?;
	flag(v, "allow_stdin", true)?;
	let mut keys = vec![];
	let mut response =
		request.header_bytes.len() + request.routing.iter().map(Vec::len).sum::<usize>() + 16384;
	if let Some(expressions) = v.get("user_expressions") {
		for (key, value) in expressions
			.as_object()
			.ok_or("user_expressions must be an object")?
		{
			if !value.is_string() {
				return Err("user_expressions values must be strings".into());
			}
			// Reserve before constructing the per-key UnsupportedFeature map.
			response += serde_json::to_vec(key)?.len() + 192;
			if response > M {
				return Err("user_expressions reply exceeds the 1 MiB reply budget".into());
			}
			keys.push(key.as_str());
		}
	}
	// The record budgets serialized admission, including ignored extension fields.
	let weight = payload_bytes;
	Ok(Options {
		code,
		keys,
		silent,
		store,
		stop,
		weight,
	})
}
impl Handler for App {
	fn handle<'a>(
		&'a self,
		channel: &'static str,
		route: Route,
		parts: &'a [Vec<u8>],
	) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res<()>> + Send + 'a>> {
		Box::pin(async move {
			let mut ready = self.ready.subscribe();
			if !*ready.borrow_and_update() {
				ready.changed().await?;
			}
			let Some(request) = self.out.codec.decode(parts)? else {
				return Ok(());
			};
			match request.kind() {
				"execute_request" if channel == "shell" => {
					let opts = match options(&request, parts.iter().map(Vec::len).sum()) {
						Ok(o) => o,
						Err(e) => {
							let p = Parent::from_request(route, &request);
							return self.out.refusal(
								&p,
								request.content["silent"] == true,
								"InvalidRequest",
								&e.to_string(),
								self.count.load(SeqCst),
							);
						}
					};
					let mut q = self.queue.lock().unwrap();
					let permit = self
						.bytes
						.clone()
						.try_acquire_many_owned(opts.weight.max(1) as u32);
					if q.retiring || q.pending.len() == 64 || permit.is_err() {
						let p = Parent::from_request(route, &request);
						drop(q);
						return self.out.refusal(
							&p,
							opts.silent,
							"KernelBusy",
							"execution admission is closed or full",
							self.count.load(SeqCst),
						);
					}
					q.pending.push_back(Job {
						parent: Arc::new(Parent::from_request(route, &request)),
						code: opts.code.into(),
						expressions: opts.keys.into_iter().map(str::to_owned).collect(),
						silent: opts.silent,
						store: opts.store,
						stop: opts.stop,
						_credit: permit.unwrap(),
					});
					drop(q);
					self.notify.notify_one();
					Ok(())
				}
				"kernel_info_request" => {
					let p = Parent::from_request(route, &request);
					let content = json!({"status":"ok","protocol_version":"5.4","implementation":"rnx","implementation_version":"0.0.0","language_info":{"name":"rune","version":"0.14.2","mimetype":"text/plain","file_extension":".rn"},"banner":"Rune 0.14.2 on rnx","help_links":[]});
					if channel == "shell" {
						self.out.shell_info(&p, &content).await
					} else {
						self.out.control_info(&p, &content)
					}
				}
				"interrupt_request" if channel == "control" => {
					let p = Parent::from_request(route, &request);
					let q = self.queue.lock().unwrap();
					self.interrupt.request(!q.pending.is_empty())?;
					drop(q);
					self.out
						.reply(&p, "interrupt_reply", &json!({}), &json!({"status":"ok"}))
				}
				"shutdown_request" if channel == "control" || channel == "shell" => {
					let restart = flag(&request.content, "restart", false)?;
					self.queue.lock().unwrap().retiring = true;
					let p = Parent::from_request(route, &request);
					let frames = self.out.frames(
						&p.routing,
						&p.header,
						"shutdown_reply",
						&json!({}),
						&json!({"restart":restart,"status":"ok"}),
					)?;
					let first = self.stop.send_if_modified(|end| {
						if end.is_none() {
							*end = Some(Instant::now() + Duration::from_secs(5));
							true
						} else {
							false
						}
					});
					let result = p.route.reply_written(&frames).await;
					if first {
						self.replied.send_replace(true);
					}
					result
				}
				_ => Ok(()),
			}
		})
	}
}
impl App {
	fn pop(&self) -> Option<Job> {
		let mut q = self.queue.lock().unwrap();
		let job = q.pending.pop_front()?;
		self.interrupt.begin();
		Some(job)
	}
	fn take_pending(&self, retire: bool) -> VecDeque<Job> {
		let mut q = self.queue.lock().unwrap();
		q.retiring |= retire;
		std::mem::take(&mut q.pending)
	}
	fn fail_pending(&self, jobs: VecDeque<Job>, name: &str, text: &str) -> Res<()> {
		for job in jobs {
			self.out
				.refusal(&job.parent, job.silent, name, text, self.count.load(SeqCst))?;
		}
		Ok(())
	}
	fn settle(&self, job: &Job, count: u64, reply: &Value) -> Res<()> {
		let failed = !reply["failure"].is_null();
		let aborted = if failed && job.stop {
			self.take_pending(false)
		} else {
			VecDeque::new()
		};
		let metadata = {
			let mut b = self.book.lock().unwrap();
			b.map(reply, if job.store { Some(count) } else { None });
			b.metadata(reply)
		};
		let mut result = if failed {
			let f = &reply["failure"];
			error(
				category(f),
				f["diagnostic"]
					.as_str()
					.ok_or("worker failure lacks diagnostic")?,
			)
		} else {
			let mut expressions = serde_json::Map::new();
			for name in &job.expressions {
				expressions.insert(
					name.clone(),
					error("UnsupportedFeature", "user_expressions are not supported"),
				);
			}
			json!({"status":"ok","user_expressions":expressions,"payload":[]})
		};
		result["execution_count"] = json!(count);
		if !job.silent {
			if failed {
				self.out.publish(
					&job.parent.header,
					"error",
					&metadata,
					&error_output(&result),
				)?;
			} else if let Some(text) = reply["text_plain"].as_str() {
				self.out.publish(
					&job.parent.header,
					"execute_result",
					&metadata,
					&json!({"execution_count":count,"data":{"text/plain":text},"metadata":{}}),
				)?;
			}
		}
		self.out
			.reply(&job.parent, "execute_reply", &metadata, &result)?;
		self.out.status(&job.parent.header, "idle")?;
		self.fail_pending(
			aborted,
			"ExecutionAborted",
			"an earlier request failed with stop_on_error enabled",
		)
	}
	async fn execute(&self, worker: &mut Worker, job: &Job) -> Res<()> {
		let count = if job.store {
			let n = self
				.count
				.load(SeqCst)
				.checked_add(1)
				.filter(|n| *n < 1 << 53)
				.ok_or("notebook counter exhausted")?;
			self.count.store(n, SeqCst);
			self.book.lock().unwrap().store(n, &job.code);
			n
		} else {
			self.count.load(SeqCst)
		};
		self.out.status(&job.parent.header, "busy")?;
		if job.code.len() > 32768 {
			self.settle(job,count,&json!({"failure":{"category":"refused","diagnostic":"input exceeds 32768 bytes","origin":null},"text_plain":null}))?;
			self.interrupt.finish();
			return Ok(());
		}
		if !job.silent {
			self.out.publish(
				&job.parent.header,
				"execute_input",
				&json!({}),
				&json!({"code":job.code,"execution_count":count}),
			)?;
		}

		if job.silent && job.code.is_empty() {
			self.settle(job, count, &json!({"failure":null,"text_plain":null}))?;
			self.interrupt.finish();
			return Ok(());
		}
		let mut sink = |event| async { self.out.stream(&job.parent, job.silent, event) };
		let mut complete = |reply: Value| async move { self.settle(job, count, &reply) };
		let result = worker
			.operate(Some(&job.code), &mut sink, &mut complete)
			.await?;
		if result["state_lost"] == true {
			return Err("worker retired with state loss".into());
		}
		Ok(())
	}
}

pub async fn run(connection: Connection, binary: &Path) -> Res<()> {
	let interrupt = Arc::new(Interrupt::default());
	let mut worker = match Worker::spawn(binary, interrupt.clone()).await {
		Ok(w) => w,
		Err(e) => {
			crate::containment::sweep(0, Instant::now() + Duration::from_secs(5)).await?;
			return Err(e);
		}
	};
	let (stop, mut stopped) = watch::channel(None);
	let (replied, mut reply_done) = watch::channel(false);
	let (ready, _) = watch::channel(false);
	let app = Arc::new(App {
		out: Arc::new(Output {
			codec: Codec::new(connection.key),
			publisher: OnceLock::new(),
			order: Mutex::new(()),
			idle: watch::channel(true).0,
		}),
		queue: Mutex::new(Queue::default()),
		bytes: Arc::new(Semaphore::new(4 * M)),
		notify: Notify::new(),
		interrupt,
		stop,
		replied,
		ready,
		count: AtomicU64::new(0),
		book: Mutex::new(Book::default()),
	});
	let mut server = match Server::bind(connection.addresses, app.clone()).await {
		Ok(s) => s,
		Err(e) => {
			worker
				.terminate(Instant::now() + Duration::from_secs(5))
				.await?;
			return Err(e);
		}
	};
	let _ = app.out.publisher.set(server.publisher());
	app.ready.send_replace(true);
	let notify = worker.notify();
	let mut failure = None;
	let mut shutdown_end = None;
	loop {
		if let Some(end) = *stopped.borrow() {
			shutdown_end = Some(end);
			break;
		}
		if let Err(e) = worker.health() {
			failure = Some(e);
			break;
		}
		if let Some(job) = app.pop() {
			let future = app.execute(&mut worker, &job);
			tokio::pin!(future);
			let result = tokio::select! {
				r=&mut future=>Some(r),
				_=stopped.changed()=>{
					let end=stopped.borrow().unwrap_or_else(||Instant::now()+Duration::from_secs(5));shutdown_end=Some(end);let _=app.interrupt.request(true);
					let _=timeout_at(Instant::now()+Duration::from_secs(1),&mut future).await;None
				}
			};
			if result.is_none() {
				break;
			}
			if let Some(Err(e)) = result {
				let text = format!("worker boundary failed; state lost: {e}");
				let mut reply = error("WorkerDied", &text);
				reply["execution_count"] = json!(app.count.load(SeqCst));
				if !job.silent {
					let _ = app.out.publish(
						&job.parent.header,
						"error",
						&json!({"rnx":{"state_lost":true}}),
						&error_output(&reply),
					);
				}
				let _ = app
					.out
					.reply(&job.parent, "execute_reply", &json!({}), &reply);
				failure = Some(e);
				break;
			}
		} else {
			let mut late = |event: StreamEvent| {
				std::future::ready(app.out.publish(
					b"{}",
					"stream",
					&json!({"rnx":{"utf8_replaced":event.replaced}}),
					&json!({"name":event.name,"text":event.text}),
				))
			};
			if let Err(e) = worker.forward_late(&mut late).await {
				failure = Some(e);
				break;
			}
			tokio::select! {_=stopped.changed()=>{},_=app.notify.notified()=>{},_=notify.notified()=>{}}
		}
	}
	app.queue.lock().unwrap().retiring = true;
	let end = shutdown_end.unwrap_or_else(|| Instant::now() + Duration::from_secs(5));
	if failure.is_none() {
		let mut sink = |_: StreamEvent| async { Ok(()) };
		let mut complete = |_: Value| async { Ok(()) };
		let _ = timeout_at(
			(Instant::now() + Duration::from_secs(1)).min(end),
			worker.operate(None, &mut sink, &mut complete),
		)
		.await;
	}
	let cleanup = worker.terminate(end).await;
	let _ = app.fail_pending(
		app.take_pending(true),
		"WorkerDied",
		"kernel is shutting down; state lost",
	);
	if shutdown_end.is_some() && !*reply_done.borrow_and_update() {
		let _ = timeout_at(end, reply_done.changed()).await;
	}
	let transport = timeout_at(end, server.shutdown()).await;
	cleanup?;
	transport??;
	if let Some(e) = failure {
		return Err(e);
	}
	Ok(())
}
#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn iopub_errors_exclude_reply_only_fields() {
		let mut reply = error("RuntimeError", "example");
		reply["execution_count"] = json!(3);
		let output = error_output(&reply);
		assert_eq!(
			output,
			json!({"ename":"RuntimeError","evalue":"example","traceback":["example"]})
		);
	}
	#[test]
	fn history_and_origins_evict_without_borrowing_the_current_count() {
		let mut b = Book::default();
		for i in 1..=10001 {
			b.map(&json!({"epoch":1,"input":i}), Some(i));
		}
		assert_eq!(b.origins.len(), 10000);
		assert!(b.metadata(&json!({"failure":{"origin":{"epoch":1,"input":1}}}))["rnx"]["notebook_count"].is_null());
		b.map(&json!({"epoch":1,"input":10002}), None);
		assert!(b.metadata(&json!({"failure":{"origin":{"epoch":1,"input":10002}}}))["rnx"]["notebook_count"].is_null());
		for i in 0..600 {
			b.store(i, &"x".repeat(32768));
		}
		assert!(b.bytes <= 16 * M);
		assert_eq!(b.history.back().unwrap().0, 599);
	}
}
