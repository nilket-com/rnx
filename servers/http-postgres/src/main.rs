//! Standalone Linux HTTP/PostgreSQL application assembled through rnx's public API.
mod memory;
mod pool;
use rnx::{Extensions, rune, server::Program};
#[global_allocator]
static ALLOCATOR: memory::Counting = memory::Counting;
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Request, Response, body::Incoming, server::conn::http1, service::service_fn};
use hyper_util::rt::{TokioIo, TokioTimer};
use serde_json::json;
use std::{
	cell::{Cell, RefCell},
	collections::{BTreeMap, BTreeSet, VecDeque},
	convert::Infallible,
	fs::OpenOptions,
	io::Write,
	rc::Rc,
	sync::{
		Arc, Mutex, OnceLock,
		atomic::{AtomicBool, AtomicUsize, Ordering},
	},
	thread,
	time::{Duration, Instant},
};
use tokio::{
	sync::{mpsc, oneshot, watch},
	task::{JoinSet, LocalSet},
	time::{Instant as TInstant, sleep, timeout_at},
};
const LIMIT: usize = 1024 * 1024;
static EVENTS: OnceLock<Mutex<std::fs::File>> = OnceLock::new();
static ORIGIN: OnceLock<Instant> = OnceLock::new();
static TERM: AtomicBool = AtomicBool::new(false);
static BUILDS: AtomicUsize = AtomicUsize::new(0);
static RETIRED: AtomicUsize = AtomicUsize::new(0);
fn event(mut v: serde_json::Value) {
	v["ms"] = json!(ORIGIN.get().unwrap().elapsed().as_secs_f64() * 1000.);
	if let Some(file) = EVENTS.get() {
		writeln!(file.lock().unwrap(), "{v}").unwrap();
	}
}
extern "C" fn term(_: libc::c_int) {
	TERM.store(true, Ordering::Relaxed);
}
fn runtime() -> tokio::runtime::Runtime {
	tokio::runtime::Builder::new_current_thread()
		.enable_all()
		.build()
		.unwrap()
}
fn extensions(id: usize, client: Option<Arc<tokio_postgres::Client>>) -> Extensions {
	Extensions::none().with_lifecycle("app", move |m, scope| {
		m.function("record", move |mode: &str| {
			let mode = mode.to_owned();
			let client = client.clone();
			scope.track(async move {
				let c = client.ok_or_else(|| "no lease".to_owned())?;
				c.execute("INSERT INTO audit(tag) VALUES ($1)", &[&mode])
					.await
					.map_err(|e| e.to_string())?;
				event(json!({"event":"db_write","id":id,"mode":mode}));
				#[cfg(feature = "test-support")]
				if mode == "long" {
					c.batch_execute("SELECT pg_sleep(120)")
						.await
						.map_err(|e| e.to_string())?;
				}
				Ok::<_, String>(())
			})
		})
		.build()
		.map_err(|e| e.to_string())?;
		m.function("phase", move |phase: &str| {
			event(json!({"event":"phase","id":id,"phase":phase}));
		})
		.build()
		.map_err(|e| e.to_string())?;
		#[cfg(feature = "test-support")]
		m.function("shutdown_stall", move || {
			event(json!({"event":"native_stall_start","id":id}));
			thread::sleep(Duration::from_millis(6500));
			event(json!({"event":"native_stall_end","id":id}));
		})
		.build()
		.map_err(|e| e.to_string())?;
		#[cfg(feature = "test-support")]
		m.function("stall", || {
			thread::sleep(Duration::from_millis(2500));
		})
		.build()
		.map_err(|e| e.to_string())?;
		#[cfg(feature = "test-support")]
		m.function(
			"headers",
			|count: usize, size: usize| -> Result<rune::runtime::Object, String> {
				assert!(count <= 62 && size <= 16317);
				let mut object = rune::runtime::Object::new();
				for n in 0..count {
					object
						.insert(
							rune::alloc::String::try_from(format!("x-{n:03}").as_str()).unwrap(),
							rune::to_value(vec!["x".repeat(size)]).unwrap(),
						)
						.unwrap();
				}
				Ok(object)
			},
		)
		.build()
		.map_err(|e| e.to_string())?;
		Ok(vec![])
	})
}
#[derive(Clone)]
struct Input {
	method: String,
	path: String,
	query: Option<String>,
	headers: BTreeMap<String, Vec<Vec<u8>>>,
	body: Vec<u8>,
}
impl Input {
	fn rune(self) -> rune::Value {
		let mut obj = rune::runtime::Object::new();
		obj.insert(
			rune::alloc::String::try_from("method").unwrap(),
			rune::to_value(self.method).unwrap(),
		)
		.unwrap();
		obj.insert(
			rune::alloc::String::try_from("path").unwrap(),
			rune::to_value(self.path).unwrap(),
		)
		.unwrap();
		obj.insert(
			rune::alloc::String::try_from("query").unwrap(),
			match self.query {
				Some(v) => rune::to_value(v).unwrap(),
				None => rune::to_value(()).unwrap(),
			},
		)
		.unwrap();
		let mut headers = rune::runtime::Object::new();
		for (k, vs) in self.headers {
			let mut values = rune::runtime::Vec::new();
			for v in vs {
				values
					.push(rune::to_value(rune::runtime::Bytes::from_slice(v).unwrap()).unwrap())
					.unwrap();
			}
			headers
				.insert(
					rune::alloc::String::try_from(k.as_str()).unwrap(),
					rune::to_value(values).unwrap(),
				)
				.unwrap();
		}
		obj.insert(
			rune::alloc::String::try_from("headers").unwrap(),
			rune::to_value(headers).unwrap(),
		)
		.unwrap();
		obj.insert(
			rune::alloc::String::try_from("body").unwrap(),
			rune::to_value(rune::runtime::Bytes::from_slice(self.body).unwrap()).unwrap(),
		)
		.unwrap();
		rune::to_value(obj).unwrap()
	}
}
struct Output {
	status: u16,
	headers: Vec<(String, Vec<u8>)>,
	body: Vec<u8>,
}
fn decode(value: rune::Value) -> Result<Output, String> {
	let obj = value
		.borrow_ref::<rune::runtime::Object>()
		.map_err(|e| e.to_string())?;
	if obj.len() != 3
		|| !["status", "headers", "body"]
			.iter()
			.all(|k| obj.get(*k).is_some())
	{
		return Err("response fields".into());
	}
	let status = obj
		.get("status")
		.unwrap()
		.as_integer::<i64>()
		.map_err(|e| e.to_string())?;
	if !(200..=599).contains(&status) {
		return Err("status".into());
	}
	let body = obj.get("body").unwrap();
	let body = if let Ok(v) = body.borrow_string_ref() {
		if v.len() > LIMIT {
			return Err("body".into());
		}
		v.as_bytes().to_vec()
	} else {
		let b = body
			.borrow_ref::<rune::runtime::Bytes>()
			.map_err(|e| e.to_string())?;
		if b.len() > LIMIT {
			return Err("body".into());
		}
		b.to_vec()
	};
	if body.len() > LIMIT || ([204, 304].contains(&status) && !body.is_empty()) {
		return Err("body".into());
	}
	let h = obj
		.get("headers")
		.unwrap()
		.borrow_ref::<rune::runtime::Object>()
		.map_err(|e| e.to_string())?;
	let mut headers = Vec::new();
	// Reserve host Connection, Content-Length, and Hyper's Date field.
	let mut bytes = 62 + body.len().to_string().len();
	for (k, v) in h.iter() {
		if k.len() > 16384 {
			return Err("headers".into());
		}
		let name = k.to_ascii_lowercase();
		if [
			"content-length",
			"transfer-encoding",
			"connection",
			"trailer",
			"upgrade",
			"keep-alive",
			"te",
			"proxy-connection",
		]
		.contains(&name.as_str())
		{
			return Err("host framing".into());
		}
		hyper::header::HeaderName::from_bytes(k.as_bytes()).map_err(|e| e.to_string())?;
		for v in v
			.borrow_ref::<rune::runtime::Vec>()
			.map_err(|e| e.to_string())?
			.iter()
		{
			let b = if let Ok(s) = v.borrow_string_ref() {
				if s.len() > 16384 {
					return Err("headers".into());
				}
				s.as_bytes().to_vec()
			} else {
				let b = v
					.borrow_ref::<rune::runtime::Bytes>()
					.map_err(|e| e.to_string())?;
				if b.len() > 16384 {
					return Err("headers".into());
				}
				b.to_vec()
			};
			hyper::header::HeaderValue::from_bytes(&b).map_err(|e| e.to_string())?;
			bytes += k.len() + b.len();
			if headers.len() == 61 || bytes > 16 * 1024 {
				return Err("headers".into());
			}
			headers.push((k.as_str().to_owned(), b));
		}
	}
	Ok(Output {
		status: status as u16,
		headers,
		body,
	})
}
struct Work {
	id: usize,
	input: Input,
	cancel: watch::Receiver<bool>,
	reply: oneshot::Sender<Output>,
}
struct Done {
	worker: usize,
	id: usize,
}
fn pin(index: usize) {
	let Ok(raw) = std::env::var("RNX_HTTP_CPUS") else {
		return;
	};
	let cpus: Vec<usize> = raw.split(',').map(|v| v.parse().unwrap()).collect();
	assert_eq!(cpus.len(), 3);
	let cpu = cpus[index];
	assert!(cpu < (libc::CPU_SETSIZE as usize));
	unsafe {
		let mut set: libc::cpu_set_t = std::mem::zeroed();
		libc::CPU_ZERO(&mut set);
		libc::CPU_SET(cpu, &mut set);
		assert_eq!(
			libc::sched_setaffinity(0, std::mem::size_of_val(&set), &set),
			0
		);
	}
	event(json!({"event":"affinity","role":index,"cpu":cpu}));
}
fn worker(
	n: usize,
	mut jobs: mpsc::Receiver<Work>,
	done: mpsc::Sender<Done>,
	unit: Program,
	ready: std::sync::mpsc::Sender<()>,
) {
	pin(n);
	let rt = runtime();
	LocalSet::new().block_on(&rt,async move {
        let pool=pool::Pool::new(n).await;
        let mut tasks=JoinSet::new();ready.send(()).unwrap();
        loop {
            tokio::select! {
                job=jobs.recv(),if tasks.len()<4=>match job {
                    None=>break,
                    Some(mut job)=>{
                        let unit=unit.clone();let done=done.clone();let pool=pool.clone();
                        tasks.spawn_local(async move {
                            let start=Instant::now();
                            // A dispatched credit may wait behind a CPU poll; cancelled work builds nothing.
                            if *job.cancel.borrow() {done.send(Done{worker:n,id:job.id}).await.unwrap();return;}
                            let lease=if job.input.path == "/db" {
                                let acquired=pool.checkout(job.id,&mut job.cancel).await;
                                match acquired {Ok(l)=>Some(l),Err(e)=>{
                                    event(json!({"event":"checkout_error","id":job.id,"message":e}));
                                    let _=job.reply.send(Output{status:500,headers:vec![],body:b"handler failed".to_vec()});
                                    done.send(Done{worker:n,id:job.id}).await.unwrap();return;
                                }}
                            } else {None};
                            if *job.cancel.borrow() {
                                if let Some(lease)=lease {let _=lease.finish(false).await;}
                                done.send(Done{worker:n,id:job.id}).await.unwrap();return;
                            }
                            BUILDS.fetch_add(1, Ordering::SeqCst);
                            let prepared=unit.prepare(extensions(job.id,lease.as_ref().map(|l|l.client())),"main",job.input.rune(),10_000_000);
                            let mut invocation=match prepared {
                                Ok(invocation)=>invocation,
                                Err(error)=> {
                                    event(json!({"event":"prepare_error","id":job.id,"message":error.message()}));
                                    if let Some(lease)=lease {let _=lease.finish(false).await;}
                                    RETIRED.fetch_add(1, Ordering::SeqCst);
                                    if error.category()=="cleanup" {panic!("context cleanup failed");}
                                    let _=job.reply.send(Output{status:500,headers:vec![],body:b"handler failed".to_vec()});
                                    done.send(Done{worker:n,id:job.id}).await.unwrap();return;
                                }
                            };
                            event(json!({"event":"built","id":job.id,"worker":n,"build_ms":start.elapsed().as_secs_f64()*1000.}));
                            let mut result={
                                event(json!({"event":"vm_start","id":job.id,"worker":n}));
                                let run=invocation.run();
                                tokio::pin!(run);
                                tokio::select!{biased;
                                    _=job.cancel.changed()=>Err("cancelled".to_owned()),
                                    r=&mut run=>match r {
                                        Ok(value)=>decode(value),
                                        Err(error)=>{event(json!({"event":"fault","id":job.id,"origin":{
                                            "source":error.path().map(|p|p.to_string_lossy().into_owned()),
                                            "position":error.position(),"excerpt":error.excerpt(),"category":error.category()
                                        }}));Err(error.to_string())}
                                    }
                                }
                            };
                            if let Err(e)=&result {event(json!({"event":"handler_error","id":job.id,"message":e}));}
                            let close=invocation.close();
                            RETIRED.fetch_add(1,Ordering::SeqCst);
                            let cleanup_failed=close.as_ref().is_err_and(|e|e.category()=="cleanup");
                            if let Err(error)=close {result=Err(error.to_string());}
                            if let Some(lease)=lease && let Err(e)=lease.finish(result.is_ok()).await {
                                event(json!({"event":"transaction_error","id":job.id,"message":e}));result=Err(e);
                            }
                            if cleanup_failed {panic!("handler lifecycle state lost after rollback");}
                            // No per-handler runtime drain. Active credit ends only after logical cleanup.
                            let output=result.unwrap_or_else(|_|Output{status:500,headers:vec![],body:b"handler failed".to_vec()});
                            let _=job.reply.send(output);
                            event(json!({"event":"teardown","id":job.id,"worker":n}));
                            done.send(Done{worker:n,id:job.id}).await.unwrap();
                        });
                    }
                },
                r=tasks.join_next(),if !tasks.is_empty()=>{r.unwrap().unwrap();}
            }
        }
        while let Some(r)=tasks.join_next().await {r.unwrap();}
        pool.close().await;
    });
	// All owners have ended; only here is a whole-runtime drain appropriate.
	rt.block_on(async {
		let end = TInstant::now() + Duration::from_secs(1);
		while rt.metrics().num_alive_tasks() != 0 && TInstant::now() < end {
			tokio::task::yield_now().await;
			sleep(Duration::from_millis(1)).await;
		}
	});
	let alive = rt.metrics().num_alive_tasks();
	event(json!({"event":"worker_end","worker":n,"tasks":alive}));
	assert_eq!(alive, 0);
}
struct ConnectionPermit(Rc<Cell<usize>>);
impl Drop for ConnectionPermit {
	fn drop(&mut self) {
		self.0.set(self.0.get() - 1);
	}
}
fn sockets() -> BTreeSet<(u32, String)> {
	std::fs::read_dir("/proc/self/fd")
		.unwrap()
		.filter_map(Result::ok)
		.filter_map(|entry| {
			let target = std::fs::read_link(entry.path()).ok()?;
			let target = target.to_string_lossy();
			if !target.starts_with("socket:[") {
				return None;
			}
			Some((
				entry.file_name().to_str()?.parse().ok()?,
				target.into_owned(),
			))
		})
		.collect()
}

struct Pending {
	work: Work,
	deadline: TInstant,
}
struct Dispatch {
	queue: VecDeque<Pending>,
	active: [usize; 2],
	running: BTreeMap<usize, watch::Sender<bool>>,
	senders: Vec<mpsc::Sender<Work>>,
	next: usize,
	stopping: bool,
}
impl Dispatch {
	fn available(&mut self) -> Option<usize> {
		if self.active == [4, 4] {
			return None;
		}
		let n = if self.active[0] == self.active[1] {
			let n = self.next;
			self.next = 1 - n;
			n
		} else if self.active[0] < self.active[1] {
			0
		} else {
			1
		};
		Some(n)
	}
	fn pump(&mut self) {
		self.queue
			.retain(|p| !*p.work.cancel.borrow() && p.deadline > TInstant::now());
		if self.stopping {
			return;
		}
		while !self.queue.is_empty() {
			let Some(n) = self.available() else {
				break;
			};
			let p = self.queue.pop_front().unwrap();
			self.active[n] += 1;
			event(
				json!({"event":"dispatch","id":p.work.id,"worker":n,"active":self.active,"queue":self.queue.len()}),
			);
			self.senders[n]
				.try_send(p.work)
				.unwrap_or_else(|_| panic!("hidden worker queue"));
		}
	}
}
struct Cancel {
	id: usize,
	tx: watch::Sender<bool>,
	state: Rc<RefCell<Dispatch>>,
}
impl Drop for Cancel {
	fn drop(&mut self) {
		event(json!({"event":"request_cancel_signal","id":self.id}));
		let _ = self.tx.send(true);
		let mut s = self.state.borrow_mut();
		s.queue.retain(|p| p.work.id != self.id);
	}
}
fn response(status: u16, body: impl Into<Bytes>) -> Response<Full<Bytes>> {
	let body = body.into();
	Response::builder()
		.status(status)
		.header("connection", "close")
		.header("content-length", body.len())
		.body(Full::new(body))
		.unwrap()
}
async fn serve(
	req: Request<Incoming>,
	id: usize,
	state: Rc<RefCell<Dispatch>>,
) -> Response<Full<Bytes>> {
	let (parts, mut body) = req.into_parts();
	if parts.version != hyper::Version::HTTP_11 {
		return response(505, "HTTP/1.1 required");
	}
	if parts.uri.to_string().len() > 8192 {
		return response(414, "target");
	}
	if parts.uri.scheme().is_some()
		|| parts.uri.authority().is_some()
		|| !parts.uri.path().starts_with('/')
	{
		return response(400, "origin form");
	}
	if parts.headers.len() > 64
		|| parts
			.headers
			.iter()
			.map(|(k, v)| k.as_str().len() + v.len())
			.sum::<usize>()
			> 16384
	{
		return response(431, "headers");
	}
	if parts.headers.contains_key("expect") {
		return response(417, "expect");
	}
	if parts.method == hyper::Method::CONNECT || parts.headers.contains_key("upgrade") {
		return response(400, "upgrade");
	}
	if parts
		.headers
		.get_all("content-encoding")
		.iter()
		.any(|v| v.as_bytes() != b"identity")
	{
		return response(415, "encoding");
	}
	if parts
		.headers
		.get("content-length")
		.and_then(|v| v.to_str().ok())
		.and_then(|v| v.parse::<u64>().ok())
		.is_some_and(|n| n > LIMIT as u64)
	{
		return response(413, "body");
	}
	let mut data = Vec::new();
	let end = TInstant::now() + Duration::from_secs(5);
	loop {
		match timeout_at(end, body.frame()).await {
			Err(_) => return response(408, "body deadline"),
			Ok(None) => break,
			Ok(Some(Err(_))) => return response(400, "body framing"),
			Ok(Some(Ok(frame))) => {
				if frame.is_trailers() {
					return response(400, "trailers");
				}
				if let Ok(bytes) = frame.into_data() {
					if bytes.len() > LIMIT - data.len() {
						return response(413, "body");
					}
					data.extend_from_slice(&bytes);
				}
			}
		}
	}
	let path = parts.uri.path().to_owned();
	if !["/healthy", "/await", "/cpu", "/fail", "/db"].contains(&path.as_str()) {
		return response(404, "route");
	}
	if parts.method != hyper::Method::POST {
		let mut r = response(405, "method");
		r.headers_mut()
			.insert("allow", hyper::header::HeaderValue::from_static("POST"));
		return r;
	}
	let mut headers = BTreeMap::<String, Vec<Vec<u8>>>::new();
	for (k, v) in &parts.headers {
		headers
			.entry(k.as_str().into())
			.or_default()
			.push(v.as_bytes().to_vec());
	}
	event(
		json!({"event":"request","id":id,"path":path,"tag":parts.headers.get("x-probe-tag").and_then(|v|v.to_str().ok())}),
	);
	let input = Input {
		method: parts.method.to_string(),
		path,
		query: parts.uri.query().map(str::to_owned),
		headers,
		body: data,
	};
	let deadline = TInstant::now() + Duration::from_secs(2);
	let (cancel, rx) = watch::channel(false);
	let (reply, result) = oneshot::channel();
	{
		let mut s = state.borrow_mut();
		s.pump();
		if s.stopping || (s.queue.len() == 16 && s.active == [4, 4]) {
			event(
				json!({"event":"refused","id":id,"status":503,"builds":BUILDS.load(Ordering::SeqCst)}),
			);
			return response(503, "capacity");
		}
		s.running.insert(id, cancel.clone());
		s.queue.push_back(Pending {
			work: Work {
				id,
				input,
				cancel: rx,
				reply,
			},
			deadline,
		});
		s.pump();
		assert!(s.queue.len() <= 16);
		event(
			json!({"event":"admitted","id":id,"active":s.active,"queue":s.queue.len(),"path":s.queue.back().map(|p|p.work.input.path.as_str())}),
		);
	}
	let guard = Cancel {
		id,
		tx: cancel,
		state: state.clone(),
	};
	let output = match timeout_at(deadline, result).await {
		Err(_) => {
			event(json!({"event":"deadline","id":id,"active":state.borrow().active}));
			response(504, "request deadline")
		}
		Ok(Err(_)) => response(
			if state.borrow().stopping {
				503
			} else if TInstant::now() >= deadline {
				504
			} else {
				500
			},
			"owner ended",
		),
		Ok(Ok(out)) => {
			let mut r = response(out.status, out.body);
			for (k, v) in out.headers {
				r.headers_mut().append(
					hyper::header::HeaderName::from_bytes(k.as_bytes()).unwrap(),
					hyper::header::HeaderValue::from_bytes(&v).unwrap(),
				);
			}
			r
		}
	};
	drop(guard);
	output
}
async fn coordinator(unit: Program, inherited: BTreeSet<(u32, String)>) {
	pin(2);
	let (done_tx, mut done_rx) = mpsc::channel::<Done>(8);
	let (ready_tx, ready_rx) = std::sync::mpsc::channel();
	let mut threads = vec![];
	let mut senders = vec![];
	for n in 0..2 {
		let (tx, rx) = mpsc::channel(4);
		senders.push(tx);
		let done = done_tx.clone();
		let unit = unit.clone();
		let ready = ready_tx.clone();
		threads.push(thread::spawn(move || worker(n, rx, done, unit, ready)));
	}
	// Startup only, before opening the listener: no coordinator work is blocked.
	drop(ready_tx);
	ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
	ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
	drop(done_tx);
	let state = Rc::new(RefCell::new(Dispatch {
		queue: VecDeque::new(),
		active: [0, 0],
		running: BTreeMap::new(),
		senders,
		next: 0,
		stopping: false,
	}));
	let listener = tokio::net::TcpListener::bind(OPTIONS.get().unwrap().bind)
		.await
		.unwrap();
	memory::reset_peak();
	if EVENTS.get().is_none() {
		println!("listening on {}", listener.local_addr().unwrap());
	}
	event(
		json!({"event":"ready","address":listener.local_addr().unwrap().to_string(),"pid":std::process::id(),"live_bytes":memory::live()}),
	);
	let connections = Rc::new(Cell::new(0usize));
	let mut tasks = JoinSet::new();
	let mut id = 0;
	let mut tick = tokio::time::interval(Duration::from_millis(5));
	let mut sampled = Instant::now();
	loop {
		while let Some(r) = tasks.try_join_next() {
			r.unwrap();
		}
		tokio::select! {
			accepted=listener.accept()=>{
				let (socket,_)=accepted.unwrap();
				if connections.get()==32{drop(socket);event(json!({"event":"connection_refused"}));continue;}
				#[cfg(feature = "test-support")]
				if std::env::var_os("RNX_HTTP_SMALL_SEND_BUFFER").is_some() {
					use std::os::fd::AsRawFd;
					let size:libc::c_int=16384;
					assert_eq!(unsafe {libc::setsockopt(socket.as_raw_fd(),libc::SOL_SOCKET,libc::SO_SNDBUF,(&size as *const libc::c_int).cast(),std::mem::size_of_val(&size) as _)},0);
				}
				connections.set(connections.get()+1);id+=1;let id=id;
				let permit=ConnectionPermit(connections.clone());let state=state.clone();
				tasks.spawn_local(async move {
					let _permit=permit;
					let dispatched=Rc::new(Cell::new(None::<TInstant>));let stamp=dispatched.clone();
					let service=service_fn(move|req|{let state=state.clone();let stamp=stamp.clone();async move{let r=serve(req,id,state).await;stamp.set(Some(TInstant::now()));event(json!({"event":"response","id":id,"status":r.status().as_u16()}));Ok::<_,Infallible>(r)}});
					let mut builder=http1::Builder::new();builder.keep_alive(false).max_buf_size(16384).max_headers(64).timer(TokioTimer::new()).header_read_timeout(Duration::from_secs(5));
					let connection=builder.serve_connection(TokioIo::new(socket),service);tokio::pin!(connection);
					let result=loop{
						tokio::select!{
							r=&mut connection=>break format!("{r:?}"),
							_=sleep(Duration::from_millis(5))=>{
								if dispatched.get().is_some_and(|at|at.elapsed()>=Duration::from_secs(1)){break "write deadline".into();}
							}
						}
					};
					event(json!({"event":"connection_end","id":id,"result":result}));
				});
			},
			Some(done)=done_rx.recv()=>{let mut s=state.borrow_mut();s.active[done.worker]-=1;s.running.remove(&done.id);s.pump();},
			r=tasks.join_next(),if !tasks.is_empty()=>{r.unwrap().unwrap();},
			_=tick.tick()=>{
				state.borrow_mut().pump();
				if sampled.elapsed()>=Duration::from_millis(100) {
					let s=state.borrow();
					event(json!({"event":"sample","active":s.active,"queue":s.queue.len(),"connections":connections.get(),"connection_tasks":tasks.len(),"live_bytes":memory::live(),"peak_bytes":memory::peak()}));
					sampled=Instant::now();
				}
				// Queued cancellations have no Done message; retire their cancellation senders too.
				{let mut s=state.borrow_mut();s.running.retain(|_,tx|tx.receiver_count()!=0);}
				if TERM.load(Ordering::Relaxed)||threads.iter().any(|t|t.is_finished()){break;}
			}
		}
	}
	let limit = TInstant::now() + Duration::from_secs(5);
	drop(listener);
	event(
		json!({"event":"shutdown","active":state.borrow().active,"queue":state.borrow().queue.len()}),
	);
	{
		let mut s = state.borrow_mut();
		s.stopping = true;
		for tx in s.running.values() {
			let _ = tx.send(true);
		}
		let queued: Vec<_> = s.queue.iter().map(|p| p.work.id).collect();
		event(json!({"event":"queued_shutdown","ids":queued}));
		s.queue.clear();
		s.running.retain(|_, tx| tx.receiver_count() != 0);
		s.senders.clear();
	}
	// Active connection tasks are disposed; pending reads cannot hold shutdown open.
	tasks.abort_all();
	while let Some(r) = tasks.join_next().await {
		assert!(r.is_ok() || r.unwrap_err().is_cancelled());
	}
	while threads.iter().any(|t| !t.is_finished()) {
		tokio::select! {
			Some(done)=done_rx.recv()=>{let mut s=state.borrow_mut();s.active[done.worker]-=1;s.running.remove(&done.id);},
			_=sleep(Duration::from_millis(2))=>{}
		}
		if TInstant::now() >= limit {
			let s = state.borrow();
			let requests: Vec<_> = s.running.keys().copied().collect();
			let workers: Vec<_> = threads
				.iter()
				.enumerate()
				.filter_map(|(n, t)| (!t.is_finished()).then_some(n))
				.collect();
			let owned: Vec<_> = sockets().difference(&inherited).cloned().collect();
			fail_shutdown(
				json!({"event":"shutdown_failed","reason":"overall deadline","active":s.active,"requests":requests,"workers_unjoined":workers,"connection_tasks":tasks.len(),"owned_sockets":owned}),
			);
		}
	}
	let mut failed = vec![];
	for (n, t) in threads.into_iter().enumerate() {
		if t.join().is_err() {
			failed.push(n);
		}
	}
	if !failed.is_empty() {
		fail_shutdown(
			json!({"event":"shutdown_failed","reason":"worker failure","failed_workers":failed,"active":state.borrow().active,"owned_sockets":sockets().difference(&inherited).cloned().collect::<Vec<_>>()}),
		);
	}
	while let Ok(done) = done_rx.try_recv() {
		state.borrow_mut().active[done.worker] -= 1;
	}
	assert_eq!(state.borrow().active, [0, 0]);
	assert_eq!(connections.get(), 0);
	let remaining = sockets();
	let owned: Vec<_> = remaining.difference(&inherited).collect();
	assert!(
		owned.is_empty(),
		"owned socket descriptors remain: {owned:?}"
	);
	assert_eq!(remaining, inherited, "inherited socket descriptors changed");
	assert_eq!(
		BUILDS.load(Ordering::SeqCst),
		RETIRED.load(Ordering::SeqCst)
	);
	event(
		json!({"event":"closed","sockets":owned.len(),"inherited_sockets":inherited.len(),"connections":connections.get(),"active":state.borrow().active,"builds":BUILDS.load(Ordering::SeqCst),"retired":RETIRED.load(Ordering::SeqCst),"live_bytes":memory::live(),"peak_bytes":memory::peak()}),
	);
}

struct Options {
	program: std::path::PathBuf,
	bind: std::net::SocketAddr,
	events: Option<std::path::PathBuf>,
}
static OPTIONS: OnceLock<Options> = OnceLock::new();
fn options() -> Result<Options, String> {
	let mut args = std::env::args_os().skip(1);
	let mut program = None;
	let mut events = None;
	let mut bind = "127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap();
	while let Some(arg) = args.next() {
		match arg.to_str() {
            Some("--program")=>program=Some(args.next().ok_or("--program needs a file")?.into()),
            Some("--events")=>events=Some(args.next().ok_or("--events needs a file")?.into()),
            Some("--bind")=>{bind=args.next().ok_or("--bind needs an address")?.to_str().ok_or("address must be Unicode")?.parse().map_err(|_|"address must be numeric")?;},
            _=>return Err("usage: rnx-http-postgres --program FILE [--bind 127.0.0.1:PORT] [--events FILE]; RNX_POOL_URL supplies the database".into()),
        }
	}
	if !bind.ip().is_loopback() {
		return Err("only loopback binding is supported".into());
	}
	if std::env::var_os("RNX_POOL_URL").is_none() {
		return Err("RNX_POOL_URL is required".into());
	}
	Ok(Options {
		program: program.ok_or("--program is required")?,
		bind,
		events,
	})
}
// Executable-only containment. No destructor, buffered stderr or a library hook
// may postpone this after the coordinator's deadline. The structured owner
// report has already been emitted to the optional regular-file event log.
fn fail_shutdown(report: serde_json::Value) -> ! {
	event(report.clone());
	let text = report.to_string();
	hard_exit(&text)
}
fn hard_exit(message: &str) -> ! {
	let bytes = message.as_bytes();
	unsafe {
		let flags = libc::fcntl(2, libc::F_GETFL);
		if flags >= 0 && libc::fcntl(2, libc::F_SETFL, flags | libc::O_NONBLOCK) >= 0 {
			libc::write(2, bytes.as_ptr().cast(), bytes.len().min(4096));
		}
		libc::_exit(1);
	}
}
fn main() {
	let options = match options() {
		Ok(options) => options,
		Err(e) => {
			eprintln!("{e}");
			std::process::exit(1);
		}
	};
	OPTIONS.set(options).ok().unwrap();
	let outcome = std::panic::catch_unwind(server);
	if outcome.is_err() {
		hard_exit("server failed; cleanup not certified");
	}
}
fn server() {
	let inherited = sockets();
	ORIGIN.set(Instant::now()).unwrap();
	let options = OPTIONS.get().unwrap();
	if let Some(path) = &options.events {
		let file = OpenOptions::new()
			.create(true)
			.append(true)
			.open(path)
			.unwrap();
		assert!(
			file.metadata().unwrap().is_file(),
			"event log must be a regular file"
		);
		EVENTS.set(Mutex::new(file)).unwrap();
	}
	event(json!({"event":"socket_baseline","descriptors":inherited}));
	BUILDS.fetch_add(1, Ordering::SeqCst);
	let unit = Program::compile(&options.program, extensions(0, None)).unwrap();
	RETIRED.fetch_add(1, Ordering::SeqCst);
	unsafe {
		libc::signal(libc::SIGTERM, term as *const () as libc::sighandler_t);
		libc::signal(libc::SIGINT, term as *const () as libc::sighandler_t);
	}
	let rt = runtime();
	LocalSet::new().block_on(&rt, coordinator(unit, inherited));
}
