//! Record 0034: loopback-only HTTP contract and connection lifecycle gates.
#![cfg(feature = "test-support")]
use serde_json::Value;
use std::{
	io::{BufRead, BufReader, Read, Write},
	net::{TcpListener, TcpStream},
	process::{Command, Stdio},
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicUsize, Ordering},
		mpsc,
	},
	time::{Duration, Instant},
};
const WAIT: Duration = Duration::from_secs(5);
#[derive(Debug)]
enum Event {
	Request(String),
	Eof,
	Error(String),
}
struct Server {
	port: u16,
	count: Arc<AtomicUsize>,
	events: mpsc::Receiver<Event>,
	stop: Arc<AtomicBool>,
}
impl Drop for Server {
	fn drop(&mut self) {
		self.stop.store(true, Ordering::Relaxed);
	}
}
impl Server {
	fn new() -> Self {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let port = listener.local_addr().unwrap().port();
		listener.set_nonblocking(true).unwrap();
		let count = Arc::new(AtomicUsize::new(0));
		let stop = Arc::new(AtomicBool::new(false));
		let (tx, events) = mpsc::channel();
		let (counter, stopping) = (count.clone(), stop.clone());
		std::thread::spawn(move || {
			while !stopping.load(Ordering::Relaxed) {
				match listener.accept() {
					Ok((stream, _)) => {
						counter.fetch_add(1, Ordering::Relaxed);
						let tx = tx.clone();
						std::thread::spawn(move || serve(stream, tx));
					}
					Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
						std::thread::sleep(Duration::from_millis(2))
					}
					Err(e) => panic!("{e}"),
				}
			}
		});
		Self {
			port,
			count,
			events,
			stop,
		}
	}
	fn url(&self, path: &str) -> String {
		format!("http://127.0.0.1:{}{path}", self.port)
	}
	fn request_seen(&self, path: &str) {
		loop {
			match self.events.recv_timeout(WAIT).unwrap() {
				Event::Request(s) if s.contains(path) => return,
				Event::Error(e) => panic!("fixture read failed: {e}"),
				_ => {}
			}
		}
	}
	fn eof(&self) {
		loop {
			match self.events.recv_timeout(WAIT).unwrap() {
				Event::Eof => return,
				Event::Error(e) => panic!("not clean EOF: {e}"),
				_ => {}
			}
		}
	}
}
fn serve(mut socket: TcpStream, tx: mpsc::Sender<Event>) {
	socket
		.set_read_timeout(Some(Duration::from_secs(5)))
		.unwrap();
	let mut reader = BufReader::new(socket.try_clone().unwrap());
	loop {
		let mut line = String::new();
		match reader.read_line(&mut line) {
			Ok(0) => {
				let _ = tx.send(Event::Eof);
				return;
			}
			Ok(_) => {}
			Err(e) => {
				let _ = tx.send(Event::Error(e.to_string()));
				return;
			}
		}
		let mut headers = String::new();
		loop {
			let mut h = String::new();
			match reader.read_line(&mut h) {
				Ok(0) | Err(_) => return,
				_ if h == "\r\n" => break,
				_ => headers.push_str(&h),
			}
		}
		let len = headers
			.lines()
			.find_map(|h| {
				h.to_ascii_lowercase()
					.strip_prefix("content-length:")
					.and_then(|n| n.trim().parse::<usize>().ok())
			})
			.unwrap_or(0);
		let mut sent_body = vec![0; len];
		if reader.read_exact(&mut sent_body).is_err() {
			return;
		}
		let path = line.split_whitespace().nth(1).unwrap().to_owned();
		let _ = tx.send(Event::Request(format!("{line}{headers}")));
		if path == "/hang" {
			continue;
		} // continue reading to observe cancellation's EOF
		let mut status = 200;
		let mut extra = "X-Case: yes\r\nSet-Cookie: a=1\r\nSet-Cookie: b=2\r\n".to_owned();
		let mut body = match path.as_str() {
			"/json" => br#"{"n":18446744073709551615}"#.to_vec(),
			"/deep" => format!("{}0{}", "[".repeat(128), "]".repeat(128)).into_bytes(),
			"/bad-json" => b"[1,]".to_vec(),
			"/invalid" => {
				extra.push_str("Content-Type: text/plain; charset=windows-1252\r\n");
				vec![0xff]
			}
			"/echo" => sent_body,
			"/404" => {
				status = 404;
				b"missing".to_vec()
			}
			"/500" => {
				status = 500;
				b"broken".to_vec()
			}
			_ => b"hello".to_vec(),
		};
		if path.starts_with("/redirect/") {
			let n: usize = path.trim_start_matches("/redirect/").parse().unwrap();
			if n > 0 {
				status = 302;
				extra.push_str(&format!("Location: /redirect/{}\r\n", n - 1));
			}
		}
		if path.starts_with("/cross/") {
			status = 302;
			extra.push_str(&format!(
				"Location: http://127.0.0.1:{}/ok\r\n",
				path.trim_start_matches("/cross/")
			));
		}
		if path == "/gzip" {
			let mut encoder =
				flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
			encoder.write_all(&vec![b'x'; 4096]).unwrap();
			body = encoder.finish().unwrap();
			extra.push_str("Content-Encoding: gzip\r\n");
		}
		if path == "/short" {
			let _ = socket
				.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1073741824\r\n\r\n0123456789");
			return;
		}
		if path == "/headers-only" {
			if socket
				.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1073741824\r\n\r\n")
				.is_err()
			{
				return;
			}
			continue;
		}
		if path == "/slow-body" {
			if socket
				.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 10\r\n\r\n12345")
				.is_err()
			{
				return;
			}
			continue;
		}
		if path == "/chunked" {
			let _ = socket.write_all(b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n5\r\nhello\r\n5\r\nworld\r\n0\r\n\r\n");
			continue;
		}
		let head = format!(
			"HTTP/1.1 {status} Fixture\r\n{extra}Content-Length: {}\r\n\r\n",
			body.len()
		);
		if socket.write_all(head.as_bytes()).is_err() {
			return;
		}
		if !line.starts_with("HEAD ") && socket.write_all(&body).is_err() {
			return;
		}
	}
}
fn command() -> Command {
	let mut cmd = Command::new(env!("CARGO_BIN_EXE_rnx"));
	// Rustyline only writes pipe-mode prompts for unsupported terminals.
	// This helper deliberately tests that mode, independent of parent TERM.
	cmd.env("TERM", "dumb");
	for name in [
		"HTTP_PROXY",
		"HTTPS_PROXY",
		"ALL_PROXY",
		"NO_PROXY",
		"http_proxy",
		"https_proxy",
		"all_proxy",
		"no_proxy",
	] {
		cmd.env_remove(name);
	}
	cmd
}
fn finish(mut child: std::process::Child) -> std::process::Output {
	let end = Instant::now() + WAIT;
	while child.try_wait().unwrap().is_none() {
		if Instant::now() > end {
			child.kill().unwrap();
			panic!("rnx exceeded fixture time bound");
		}
		std::thread::sleep(Duration::from_millis(2));
	}
	child.wait_with_output().unwrap()
}
fn eval(source: &str) -> String {
	let out = finish(
		command()
			.args(["eval", source])
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap(),
	);
	assert!(
		out.status.success(),
		"{source}: {}",
		String::from_utf8_lossy(&out.stderr)
	);
	String::from_utf8(out.stdout).unwrap().trim().to_owned()
}
fn request(server: &Server, path: &str, options: &str) -> String {
	format!(
		"http::request(\"GET\", {:?}, {options}).await",
		server.url(path)
	)
}
fn refusal(expr: &str) -> String {
	serde_json::from_str(&eval(&format!(
		"match {expr} {{ Ok(_) => \"UNEXPECTED SUCCESS\", Err(e) => e }}"
	)))
	.unwrap()
}
fn response(expr: &str) -> Value {
	let text: String =
		serde_json::from_str(&eval(&format!("host::json_stringify({expr}?)?"))).unwrap();
	serde_json::from_str(&text).unwrap()
}
#[test]
fn responses_methods_headers_and_statuses() {
	let s = Server::new();
	let r = response(&format!("http::get({:?}).await", s.url("/ok")));
	assert_eq!(r["body"], "hello");
	assert_eq!(r["status"], 200);
	assert_eq!(r["headers"]["x-case"], serde_json::json!(["yes"]));
	assert_eq!(
		r["headers"]["set-cookie"],
		serde_json::json!(["a=1", "b=2"])
	);
	let bytes = response(&format!("http::get_bytes({:?}).await", s.url("/ok")));
	assert_eq!(bytes["body"], serde_json::json!([104, 101, 108, 108, 111]));
	for method in ["POST", "PUT", "PATCH", "DELETE"] {
		let r = response(&format!(
			"http::request({method:?}, {:?}, #{{body: \"payload\"}}).await",
			s.url("/echo")
		));
		assert_eq!(r["body"], "payload");
	}
	let head = response(&format!(
		"http::request(\"HEAD\", {:?}, #{{}}).await",
		s.url("/ok")
	));
	assert_eq!(head["body"], "");
	assert_eq!(head["headers"]["content-length"][0], "5");
	for status in [404, 500] {
		assert_eq!(
			response(&request(&s, &format!("/{status}"), "#{}"))["status"],
			status
		);
	}
}
#[test]
fn json_text_and_byte_contracts() {
	let s = Server::new();
	assert_eq!(
		eval(&format!(
			"let r = http::get({:?}).await?; host::json_parse(r.body)?.n == 18446744073709551615u64",
			s.url("/json")
		)),
		"true"
	);
	for path in ["/deep", "/bad-json"] {
		let message = refusal(&format!(
			"host::json_parse(http::get({:?}).await?.body)",
			s.url(path)
		));
		assert!(message.contains("JSON document"), "{message}");
	}
	let message = refusal(&request(&s, "/invalid", "#{}"));
	assert!(
		message.contains("not UTF-8 at byte 0") && message.contains("get_bytes"),
		"{message}"
	);
	let r = response(&format!(
		"http::request_bytes(\"GET\", {:?}, #{{}}).await",
		s.url("/invalid")
	));
	assert_eq!(r["body"], serde_json::json!([255]));
}
#[test]
fn bounds_deadlines_and_invalid_options() {
	let s = Server::new();
	for path in ["/hang", "/slow-body", "/headers-only"] {
		let start = Instant::now();
		let message = refusal(&request(&s, path, "#{timeout_ms: 100}"));
		assert!(message.contains("deadline of 100 ms"), "{message}");
		assert!(
			start.elapsed() < Duration::from_millis(355),
			"{path}: {:?}",
			start.elapsed()
		);
	}
	for path in ["/chunked", "/gzip"] {
		let message = refusal(&request(&s, path, "#{body_limit: 100}"));
		if path == "/chunked" {
			assert_eq!(
				response(&request(&s, path, "#{body_limit: 10}"))["body"],
				"helloworld"
			);
		} else {
			assert!(message.contains("limit of 100 bytes"), "{message}");
		}
	}
	assert!(refusal(&request(&s, "/chunked", "#{body_limit: 9}")).contains("limit of 9 bytes"));
	let message = refusal(&request(&s, "/short", "#{}"));
	assert!(
		message.contains("after 10 of 1073741824 bytes"),
		"{message}"
	);
	let count = s.count.load(Ordering::Relaxed);
	for (options, word) in [
		("#{timeout_ms: 0}", "deadline"),
		("#{timeout_ms: 90001}", "deadline"),
		("#{body_limit: 0}", "body_limit"),
		("#{body_limit: 67108865}", "body_limit"),
		("#{timout_ms: 1}", "timout_ms"),
		("#{headers: #{\"bad name\": \"x\"}}", "header"),
	] {
		assert!(refusal(&request(&s, "/ok", options)).contains(word));
	}
	assert_eq!(
		s.count.load(Ordering::Relaxed),
		count,
		"invalid options sent a request"
	);
}
#[test]
fn redirects_credentials_proxy_and_dns() {
	let (s, other) = (Server::new(), Server::new());
	assert!(
		response(&request(&s, "/redirect/3", "#{}"))["url"]
			.as_str()
			.unwrap()
			.ends_with("/redirect/0")
	);
	assert!(refusal(&request(&s, "/redirect/11", "#{}")).contains("redirect"));
	response(&request(
		&s,
		&format!("/cross/{}", other.port),
		"#{headers: #{Authorization: \"secret\"}}",
	));
	let Event::Request(observed) = other.events.recv_timeout(WAIT).unwrap() else {
		panic!()
	};
	assert!(!observed.to_lowercase().contains("authorization") && !observed.contains("secret"));
	assert!(refusal("http::get(\"http://rnx-dns-failure.invalid/\").await").contains("DNS"));
	assert!(refusal("http::get(\"not a url\").await").contains("invalid URL"));
	let closed = TcpListener::bind("127.0.0.1:0")
		.unwrap()
		.local_addr()
		.unwrap()
		.port();
	assert!(
		refusal(&format!("http::get(\"http://127.0.0.1:{closed}/\").await")).contains("connect")
	);
	let proxy = Server::new();
	let source = format!("http::get({:?}).await?.body", other.url("/via-proxy"));
	let output = finish(
		command()
			.env("HTTP_PROXY", proxy.url(""))
			.args(["eval", &source])
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap(),
	);
	assert!(
		output.status.success(),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	proxy.request_seen("/via-proxy");
}
#[test]
fn file_and_session_entry_points_and_module_names() {
	let s = Server::new();
	let source = format!(
		"pub async fn main(_) {{ http::get({:?}).await?.body }}",
		s.url("/ok")
	);
	let path = std::env::temp_dir().join(format!("rnx-http-{}.rn", std::process::id()));
	std::fs::write(&path, source).unwrap();
	let out = finish(
		command()
			.arg("run")
			.arg(&path)
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap(),
	);
	std::fs::remove_file(path).unwrap();
	assert!(
		out.status.success(),
		"{}",
		String::from_utf8_lossy(&out.stderr)
	);
	assert_eq!(String::from_utf8(out.stdout).unwrap(), "\"hello\"\n");
	let mut repl = Repl::new();
	repl.prompt();
	repl.send(&format!("http::get({:?}).await?.body", s.url("/ok")));
	assert!(repl.prompt().contains("hello"));
	for name in [
		"http::get",
		"http::get_bytes",
		"http::request",
		"http::request_bytes",
	] {
		repl.send(&format!(":help {name}"));
		let help = repl.prompt();
		assert!(help.contains(name) && help.contains("HTTP"), "{help}");
	}
	repl.close();
}

struct Repl {
	child: std::process::Child,
	input: Option<std::process::ChildStdin>,
	output: mpsc::Receiver<String>,
}
impl Repl {
	fn new() -> Self {
		let mut child = command()
			.arg("repl")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		let input = child.stdin.take();
		let mut stdout = child.stdout.take().unwrap();
		let (tx, output) = mpsc::channel();
		std::thread::spawn(move || {
			let mut acc = Vec::new();
			let mut byte = [0];
			while stdout.read(&mut byte).unwrap_or(0) != 0 {
				acc.push(byte[0]);
				if acc.ends_with(b"rnx> ") {
					let _ = tx.send(String::from_utf8_lossy(&acc).into_owned());
					acc.clear();
				}
			}
		});
		Self {
			child,
			input,
			output,
		}
	}
	fn send(&mut self, s: &str) {
		writeln!(self.input.as_mut().unwrap(), "{s}").unwrap();
	}
	fn prompt(&self) -> String {
		self.output
			.recv_timeout(WAIT)
			.expect("no prompt within five seconds")
	}
	fn close(mut self) {
		drop(self.input.take());
		let end = Instant::now() + WAIT;
		while self.child.try_wait().unwrap().is_none() {
			assert!(Instant::now() < end);
			std::thread::sleep(Duration::from_millis(5));
		}
		let mut errors = String::new();
		self.child
			.stderr
			.as_mut()
			.unwrap()
			.read_to_string(&mut errors)
			.unwrap();
		assert!(!errors.contains("cleanup did not finish"), "{errors}");
	}
}
impl Drop for Repl {
	fn drop(&mut self) {
		if self.child.try_wait().unwrap().is_none() {
			let _ = self.child.kill();
			let _ = self.child.wait();
		}
	}
}

#[test]
#[cfg(unix)]
fn cancellation_discards_healthy_pool_and_retained_request_then_reset_closes_reused_socket() {
	let (healthy, hanging) = (Server::new(), Server::new());
	let mut repl = Repl::new();
	repl.prompt();
	let first_start = Instant::now();
	repl.send(&format!("http::get({:?}).await?.body", healthy.url("/ok")));
	repl.prompt();
	let first = first_start.elapsed();
	let second_start = Instant::now();
	repl.send(&format!("http::get({:?}).await?.body", healthy.url("/ok")));
	repl.prompt();
	let second = second_start.elapsed();
	assert_eq!(healthy.count.load(Ordering::Relaxed), 1);
	println!(
		"session request through next prompt: first {first:?}, reused {second:?}; one accepted socket"
	);
	// Retain the future in a binding from an earlier input: cleanup cannot
	// rely on dropping the input's VM to destroy the request.
	repl.send(&format!(
		"let pending = http::get({:?});",
		hanging.url("/hang")
	));
	repl.prompt();
	repl.send("pending.await?");
	hanging.request_seen("/hang");
	let start = Instant::now();
	assert!(
		Command::new("kill")
			.args(["-INT", &repl.child.id().to_string()])
			.status()
			.unwrap()
			.success()
	);
	repl.prompt();
	healthy.eof();
	hanging.eof();
	assert!(start.elapsed() < Duration::from_millis(355));
	repl.send(&format!("http::get({:?}).await?.body", healthy.url("/ok")));
	assert!(repl.prompt().contains("hello"));
	assert_eq!(healthy.count.load(Ordering::Relaxed), 2);
	repl.send(":reset");
	repl.prompt();
	healthy.eof();
	repl.send(&format!("http::get({:?}).await?.body", healthy.url("/ok")));
	repl.prompt();
	assert_eq!(healthy.count.load(Ordering::Relaxed), 3);
	repl.close();
}

#[test]
fn tls_verification_and_plaintext_protocol_failures_are_distinct() {
	use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let port = listener.local_addr().unwrap().port();
	let config = rustls::ServerConfig::builder()
		.with_no_client_auth()
		.with_single_cert(
			vec![CertificateDer::from(
				include_bytes!("fixtures/http/untrusted-cert.der").to_vec(),
			)],
			PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
				include_bytes!("fixtures/http/untrusted-key.der").to_vec(),
			)),
		)
		.unwrap();
	let (tx, rx) = mpsc::channel();
	std::thread::spawn(move || {
		let (stream, _) = listener.accept().unwrap();
		stream.set_read_timeout(Some(WAIT)).unwrap();
		let session = rustls::ServerConnection::new(Arc::new(config)).unwrap();
		let mut tls = rustls::StreamOwned::new(session, stream);
		let result = tls.read(&mut [0u8; 1]);
		tx.send(result.is_err()).unwrap();
	});
	let refused = refusal(&format!("http::get(\"https://127.0.0.1:{port}/\").await"));
	assert!(refused.contains("certificate"), "{refused}");
	assert!(rx.recv_timeout(WAIT).unwrap());

	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let port = listener.local_addr().unwrap().port();
	std::thread::spawn(move || {
		let (mut stream, _) = listener.accept().unwrap();
		let _ = stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n");
	});
	let refused = refusal(&format!("http::get(\"https://127.0.0.1:{port}/\").await"));
	assert!(
		refused.to_lowercase().contains("connect") && !refused.contains("certificate"),
		"{refused}"
	);
}

#[test]
#[cfg(feature = "count-allocations")]
fn declared_size_never_becomes_a_buffer_allocation() {
	let s = Server::new();
	let peak = eval(&format!(
		"host::test_reset_allocation_peak(); let r = {} ; host::test_allocation_peak()",
		request(&s, "/short", "#{}")
	));
	let peak: u64 = peak.parse().unwrap();
	assert!(
		peak < 32 * 1024 * 1024,
		"1 GiB header drove peak allocation to {peak}"
	);
}

#[test]
fn stalled_system_lookup_does_not_delay_run_eval_session_or_reset() {
	let expression = "match http::request(\"GET\", \"http://rnx-dns-stall.invalid/\", #{timeout_ms: 100}).await { Ok(_) => \"unexpected success\", Err(e) => e }";
	let path = std::env::temp_dir().join(format!("rnx-http-dns-{}.rn", std::process::id()));
	std::fs::write(&path, format!("pub async fn main(_) {{ {expression} }}")).unwrap();
	for mode in ["run", "eval"] {
		let start = Instant::now();
		let mut cmd = command();
		cmd.arg(mode).arg(if mode == "run" {
			path.to_str().unwrap()
		} else {
			expression
		});
		let output = finish(
			cmd.stdout(Stdio::piped())
				.stderr(Stdio::piped())
				.spawn()
				.unwrap(),
		);
		assert!(
			output.status.success(),
			"{}",
			String::from_utf8_lossy(&output.stderr)
		);
		assert!(String::from_utf8_lossy(&output.stdout).contains("deadline of 100 ms"));
		assert!(
			start.elapsed() < Duration::from_millis(355),
			"{mode}: {:?}",
			start.elapsed()
		);
		println!(
			"stalled DNS {mode} process completed in {:?}",
			start.elapsed()
		);
	}
	std::fs::remove_file(path).unwrap();
	let mut repl = Repl::new();
	repl.prompt();
	let start = Instant::now();
	repl.send(expression);
	assert!(repl.prompt().contains("deadline of 100 ms"));
	repl.send("42");
	assert!(repl.prompt().contains("42"));
	repl.send(":reset");
	assert!(repl.prompt().contains("session reset"));
	repl.close();
	assert!(
		start.elapsed() < Duration::from_millis(355),
		"session: {:?}",
		start.elapsed()
	);
	println!(
		"stalled DNS session deadline, next input, reset and exit completed in {:?}",
		start.elapsed()
	);
}
