//! A bounded HTTP/1 transport over the reusable caller-owned Rune invocation boundary.
mod log;
mod options;
mod value;
mod worker;
use crate::{Extensions, server::Program};
use axum::{
	body::{Body, Bytes, to_bytes},
	extract::{Request, State},
	response::Response,
};
use std::{
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	},
	task::{Context, Poll},
	time::Duration,
};
use tokio::{
	sync::{OwnedSemaphorePermit, Semaphore, oneshot, watch},
	task::JoinSet,
	time::Instant,
};
use value::{Input, Output};
use worker::{Job, Workers};
struct Hub {
	workers: Vec<tokio::sync::mpsc::Sender<Job>>,
	next: AtomicUsize,
	admission: Arc<Semaphore>,
	timeout: Duration,
	stop: watch::Receiver<Option<Instant>>,
	log: log::Log,
}
/// The admission permit follows the response body until it is sent or abandoned.
struct AdmittedBody {
	body: Body,
	_permit: OwnedSemaphorePermit,
}
impl hyper::body::Body for AdmittedBody {
	type Data = Bytes;
	type Error = axum::Error;
	fn poll_frame(
		mut self: Pin<&mut Self>,
		cx: &mut Context<'_>,
	) -> Poll<Option<Result<hyper::body::Frame<Bytes>, Self::Error>>> {
		Pin::new(&mut self.body).poll_frame(cx)
	}
	fn is_end_stream(&self) -> bool {
		hyper::body::Body::is_end_stream(&self.body)
	}
	fn size_hint(&self) -> hyper::body::SizeHint {
		hyper::body::Body::size_hint(&self.body)
	}
}
struct RequestLog {
	log: log::Log,
	method: String,
	path: String,
	start: Instant,
	status: Option<u16>,
}
impl Drop for RequestLog {
	fn drop(&mut self) {
		self.log
			.request(&self.method, &self.path, self.status, self.start.elapsed());
	}
}
async fn route(State(hub): State<Arc<Hub>>, req: Request) -> Response {
	let started = Instant::now();
	let mut trace = RequestLog {
		log: hub.log.clone(),
		method: req.method().to_string(),
		path: req.uri().path().chars().take(128).collect(),
		start: started,
		status: None,
	};
	let head = req.method() == axum::http::Method::HEAD;
	let (parts, body) = req.into_parts();
	let result = async {
		if hub.stop.borrow().is_some() {
			return (Output::error(503), None);
		}
		let permit = match hub.admission.clone().try_acquire_owned() {
			Ok(p) => p,
			Err(_) => return (Output::error(503), None),
		};
		if parts.uri.to_string().len() > 8192 || parts.headers.len() > value::HEADERS {
			return (Output::error(400), Some(permit));
		}
		if parts
			.headers
			.iter()
			.map(|(k, v)| k.as_str().len() + v.as_bytes().len() + 4)
			.sum::<usize>()
			> value::HEAD
		{
			return (Output::error(400), Some(permit));
		}
		if parts.headers.get("content-length").is_some_and(|s| {
			s.to_str()
				.ok()
				.and_then(|s| s.parse::<u64>().ok())
				.is_some_and(|n| n > value::BODY as u64)
		}) {
			return (Output::error(413), Some(permit));
		}
		let deadline = started + hub.timeout;
		let body_deadline = (started + Duration::from_secs(5)).min(deadline);
		let body = match tokio::time::timeout_at(body_deadline, to_bytes(body, value::BODY)).await {
			Err(_) => return (Output::error(408), Some(permit)),
			Ok(Err(_)) => return (Output::error(413), Some(permit)),
			Ok(Ok(b)) => b,
		};
		if hub.stop.borrow().is_some() {
			return (Output::error(503), Some(permit));
		}
		let mut headers = std::collections::BTreeMap::<String, Vec<Vec<u8>>>::new();
		for (k, v) in parts.headers.iter() {
			headers
				.entry(k.as_str().to_owned())
				.or_default()
				.push(v.as_bytes().to_vec());
		}
		let input = Input {
			method: parts.method.to_string(),
			path: parts.uri.path().into(),
			query: parts.uri.query().map(str::to_owned),
			headers,
			body: body.to_vec(),
		};
		let (reply, answer) = oneshot::channel();
		let mut job = Job {
			input,
			reply,
			deadline,
		};
		let start = hub.next.fetch_add(1, Ordering::Relaxed);
		let mut sent = false;
		for i in 0..hub.workers.len() {
			match hub.workers[(start + i) % hub.workers.len()].try_send(job) {
				Ok(()) => {
					sent = true;
					break;
				}
				Err(e) => {
					job = e.into_inner();
				}
			}
		}
		if !sent {
			return (Output::error(503), Some(permit));
		}
		let out = match tokio::time::timeout_at(deadline, answer).await {
			Err(_) => Output::error(504),
			Ok(Err(_)) => Output::error(500),
			Ok(Ok(o)) => o,
		};
		(out, Some(permit))
	}
	.await;
	trace.status = Some(result.0.status);
	let mut response = result.0.response(head);
	if let Some(permit) = result.1 {
		let body = std::mem::replace(response.body_mut(), Body::empty());
		*response.body_mut() = Body::new(AdmittedBody {
			body,
			_permit: permit,
		});
	}
	response
}
#[cfg(unix)]
async fn signal() -> std::io::Result<()> {
	use tokio::signal::unix::{SignalKind, signal};
	let mut int = signal(SignalKind::interrupt())?;
	let mut term = signal(SignalKind::terminate())?;
	tokio::select! {_=int.recv()=>{},_=term.recv()=>{}}
	Ok(())
}
#[cfg(not(unix))]
async fn signal() -> std::io::Result<()> {
	tokio::signal::ctrl_c().await
}
async fn serve(
	options: options::Options,
	program: Program,
	extensions: worker::Factory,
	log: log::Log,
) -> Result<(), String> {
	let listener = tokio::net::TcpListener::bind(options.bind)
		.await
		.map_err(|e| format!("serve bind {}: {e}", options.bind))?;
	let (stop_tx, stop_rx) = watch::channel(None);
	let (fatal_tx, mut fatal_rx) = watch::channel(None);
	let workers = Workers::start(
		program,
		extensions,
		&options,
		stop_rx.clone(),
		fatal_tx,
		log.clone(),
	)?;
	let hub = Arc::new(Hub {
		workers: workers.senders.clone(),
		next: AtomicUsize::new(0),
		admission: Arc::new(Semaphore::new(
			options.workers * (worker::QUEUE + worker::ACTIVE),
		)),
		timeout: options.timeout,
		stop: stop_rx.clone(),
		log: log.clone(),
	});
	let app = axum::Router::new().fallback(route).with_state(hub);
	let connections = Arc::new(Semaphore::new(256));
	let mut tasks = JoinSet::new();
	log.event(
		"ready",
		&listener
			.local_addr()
			.map_err(|e| e.to_string())?
			.to_string(),
	);
	let mut first_signal = Box::pin(signal());
	let failure = loop {
		tokio::select! {
			biased;
			sig=&mut first_signal=>{break sig.err().map(|e|e.to_string());},
			_=fatal_rx.changed()=>{break fatal_rx.borrow().clone().or(Some("HTTP worker stopped".into()));},
			_=tasks.join_next(),if !tasks.is_empty()=>{},
			incoming=listener.accept(), if connections.available_permits() > 0 => {
				let (stream,_)=match incoming {Ok(s)=>s,Err(e)=>break Some(e.to_string())};
				let permit=match connections.clone().try_acquire_owned(){Ok(p)=>p,Err(_)=>{drop(stream);continue;}};
				let service=hyper_util::service::TowerToHyperService::new(app.clone());
				let mut stop=stop_rx.clone();
				let log=log.clone();
				tasks.spawn(async move {
					let _permit=permit;
					let mut builder=hyper::server::conn::http1::Builder::new();
					builder.max_buf_size(value::HEAD).max_headers(value::HEADERS).header_read_timeout(Duration::from_secs(10)).timer(hyper_util::rt::TokioTimer::new());
					let connection=builder.serve_connection(hyper_util::rt::TokioIo::new(stream),service);
					tokio::pin!(connection);
					tokio::select! {
						r=&mut connection=>{if let Err(e)=r {log.event("connection_error",&e.to_string());}},
						_=stop.changed()=> {
							connection.as_mut().graceful_shutdown();
							let deadline=(*stop.borrow()).unwrap_or_else(Instant::now);
							let _=tokio::time::timeout_at(deadline,&mut connection).await;
						}
					}
				});
			}
		}
	};
	drop(listener);
	let deadline = Instant::now() + options.grace;
	let _ = stop_tx.send(Some(deadline));
	log.event("shutdown", failure.as_deref().unwrap_or("signal"));
	let drain = async {
		while tasks.join_next().await.is_some() {}
		drop(app);
		// Joining on a separate thread keeps a second signal actionable even if a
		// synchronous trusted native poll delays worker shutdown.
		let _ = tokio::task::spawn_blocking(move || workers.join()).await;
	};
	tokio::select! {
		_=drain=>{},
		_=signal()=>{log.event("shutdown","second signal: immediate exit");crate::terminal::exit(130);}
	}
	match failure {
		Some(e) => Err(e),
		None => Ok(()),
	}
}
pub(super) fn main(args: &[String], extensions: Extensions) -> crate::Result<()> {
	if args == ["--help"] || args == ["-h"] {
		println!("{}", options::HELP);
		return Ok(());
	}
	let options = options::Options::parse(args)?;
	if !extensions.names().is_empty() {
		return Err(format!("serve does not support executables with extensions yet ({}): builders are one-shot; use stock rnx serve. Adapter-enabled serving needs repeatable thread-safe factories", extensions.names().join(", ")).into());
	}
	let logger = log::Logger::new(log::stderr()?, options.request_logs)?;
	let result = (|| {
		let program = Program::compile(&options.program, extensions).map_err(|e| {
			let mut message = e.message().to_owned();
			if let (Some(path), Some((line, column))) = (e.path(), e.position()) {
				message = format!("{}:{line}:{column}: {message}", path.display());
			}
			message
		})?;
		let rt = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.map_err(|e| e.to_string())?;
		rt.block_on(serve(
			options,
			program,
			Arc::new(Extensions::none),
			logger.log.clone(),
		))
	})();
	if let Err(e) = &result {
		logger.log.event("fatal", e);
	}
	let report = logger.shutdown();
	// Accounted internally even when the stderr pipe cannot accept the summary.
	let _ = (
		report.enqueued,
		report.written,
		report.dropped,
		report.outstanding,
		report.detached,
	);
	if result.is_err() {
		// All transport/runtime owners have been disposed. Reporting again via
		// Rust's Termination stderr write could block behind the same full sink.
		std::process::exit(1);
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn stock_only_refusal_precedes_opening_the_source_or_building_extensions() {
		let e = Extensions::none().with("fixture", |_| panic!("builder must not run"));
		let err = main(&["does-not-exist.rn".into()], e)
			.unwrap_err()
			.to_string();
		assert!(err.contains("extensions yet (fixture)"));
		assert!(err.contains("stock rnx serve"));
	}
}
