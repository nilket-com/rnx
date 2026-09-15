//! Acceptance adapter only: not an installed kernel or public protocol mode.
use rnx_jupyter::transport::{Handler, Res, Route, Server};
use serde_json::{Value, json};
use std::{
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering::SeqCst},
	},
	time::Duration,
};
use tokio::{io::AsyncBufReadExt, sync::mpsc};
static BAD: AtomicUsize = AtomicUsize::new(0);
static ACCEPTED: AtomicUsize = AtomicUsize::new(0);
// Independent allocator accounting: requested Rust allocation sizes, not payload credits.
struct Alloc;
static LIVE: AtomicUsize = AtomicUsize::new(0);
static PEAK: AtomicUsize = AtomicUsize::new(0);
static LARGEST: AtomicUsize = AtomicUsize::new(0);
unsafe impl std::alloc::GlobalAlloc for Alloc {
	unsafe fn alloc(&self, l: std::alloc::Layout) -> *mut u8 {
		let p = unsafe { std::alloc::System.alloc(l) };
		if !p.is_null() {
			let n = LIVE.fetch_add(l.size(), SeqCst) + l.size();
			PEAK.fetch_max(n, SeqCst);
			LARGEST.fetch_max(l.size(), SeqCst);
		}
		p
	}
	unsafe fn dealloc(&self, p: *mut u8, l: std::alloc::Layout) {
		LIVE.fetch_sub(l.size(), SeqCst);
		unsafe { std::alloc::System.dealloc(p, l) };
	}
	unsafe fn realloc(&self, p: *mut u8, l: std::alloc::Layout, n: usize) -> *mut u8 {
		let q = unsafe { std::alloc::System.realloc(p, l, n) };
		if !q.is_null() {
			if n >= l.size() {
				LIVE.fetch_add(n - l.size(), SeqCst);
			} else {
				LIVE.fetch_sub(l.size() - n, SeqCst);
			}
			PEAK.fetch_max(LIVE.load(SeqCst), SeqCst);
			LARGEST.fetch_max(n, SeqCst);
		}
		q
	}
}
#[global_allocator]
static ALLOC: Alloc = Alloc;
async fn application(
	codec: &rnx_jupyter::wire::Codec,
	route: &Route,
	parts: &[Vec<u8>],
	jobs: &mpsc::Sender<(usize, bool)>,
) -> Res<()> {
	let Some(request) = codec.decode(parts)? else {
		BAD.fetch_add(1, SeqCst);
		return Ok(());
	};
	ACCEPTED.fetch_add(1, SeqCst);
	if let Some(n) = request.content["publish"].as_u64() {
		jobs.try_send((
			n.min(20000) as usize,
			request.content["paced"].as_bool().unwrap_or(false),
		))?;
	}
	let out = codec.encode(
		request.routing,
		request.header_bytes,
		"probe_reply",
		"2026-09-15T00:00:00Z",
		&json!({}),
		&json!({"status":"ok","echo":request.content}),
	)?;
	route.reply(&out)
}
struct App {
	codec: rnx_jupyter::wire::Codec,
	jobs: mpsc::Sender<(usize, bool)>,
}
impl Handler for App {
	fn handle<'a>(
		&'a self,
		_channel: &'static str,
		route: Route,
		parts: &'a [Vec<u8>],
	) -> std::pin::Pin<Box<dyn std::future::Future<Output = Res<()>> + Send + 'a>> {
		Box::pin(async move { application(&self.codec, &route, parts, &self.jobs).await })
	}
}
fn emit(v: Value) {
	use std::io::Write;
	let mut w = std::io::stdout().lock();
	writeln!(w, "{v}").unwrap();
	w.flush().unwrap();
}
fn stats(server: &Server) -> Value {
	let mut v = server.snapshot();
	v["allocation_live"] = json!(LIVE.load(SeqCst));
	v["allocation_peak"] = json!(PEAK.load(SeqCst));
	v["largest_allocation"] = json!(LARGEST.load(SeqCst));
	v["accepted"] = json!(ACCEPTED.load(SeqCst));
	v["bad_signature"] = json!(BAD.load(SeqCst));
	v
}
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() -> Res<()> {
	let (jobs, mut pending) = mpsc::channel(8);
	let app = Arc::new(App {
		codec: rnx_jupyter::wire::Codec::new(
			if std::env::args().nth(1).as_deref() == Some("empty") {
				vec![]
			} else {
				b"local-probe-key".to_vec()
			},
		),
		jobs,
	});
	let mut server = Server::bind(["127.0.0.1:0".parse()?; 5], app).await?;
	let publisher = server.publisher();
	let task = tokio::spawn(async move {
		while let Some((n, paced)) = pending.recv().await {
			for _ in 0..n {
				let _ = publisher.publish(&[b"probe".to_vec(), vec![b'x'; 16384]]);
				if paced {
					tokio::time::sleep(Duration::from_millis(2)).await;
				} else {
					tokio::task::yield_now().await;
				}
			}
		}
	});
	let mut addresses = serde_json::Map::new();
	for (name, address) in ["shell", "control", "stdin", "hb", "iopub"]
		.into_iter()
		.zip(server.addresses())
	{
		addresses.insert(name.into(), json!(format!("tcp://{address}")));
	}
	emit(Value::Object(addresses));
	let mut lines = tokio::io::BufReader::new(tokio::io::stdin()).lines();
	while let Some(line) = lines.next_line().await? {
		if line == "stop" {
			break;
		}
		emit(stats(&server));
	}
	task.abort();
	let _ = task.await;
	server.shutdown().await?;
	emit(json!({"shutdown":stats(&server)}));
	Ok(())
}
