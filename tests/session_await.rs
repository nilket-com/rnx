//! Native, loopback-only readiness and signal controls for direct host waits.
#![cfg(unix)]
use std::{
	io::{BufRead, BufReader, Read, Write},
	net::TcpListener,
	process::{Child, Command, Stdio},
	sync::mpsc,
	time::{Duration, Instant},
};
struct Running(Child);
impl Drop for Running {
	fn drop(&mut self) {
		let _ = self.0.kill();
		let _ = self.0.wait();
	}
}
fn pending(engine: &str, http: bool, nested: bool) -> f64 {
	let listener = TcpListener::bind("127.0.0.1:0").unwrap();
	let url = format!(
		"http://127.0.0.1:{}/",
		listener.local_addr().unwrap().port()
	);
	let (tx, rx) = mpsc::channel();
	let server = if http {
		Some(std::thread::spawn(move || {
			let (mut socket, _) = listener.accept().unwrap();
			socket
				.set_read_timeout(Some(Duration::from_secs(5)))
				.unwrap();
			let mut reader = BufReader::new(socket.try_clone().unwrap());
			loop {
				let mut line = String::new();
				reader.read_line(&mut line).unwrap();
				if line == "\r\n" {
					break;
				}
			}
			tx.send(()).unwrap();
			let mut b = [0];
			assert_eq!(
				socket.read(&mut b).unwrap(),
				0,
				"pending request socket did not close"
			);
		}))
	} else {
		drop(listener);
		None
	};
	let child = Command::new(env!("CARGO_BIN_EXE_rnx"))
		.args(["repl", &format!("--engine={engine}")])
		.env_clear()
		.env("PATH", std::env::var_os("PATH").unwrap())
		.env("TERM", "dumb")
		.env("TZ", "UTC")
		.env("NO_PROXY", "*")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let mut child = Running(child);
	let mut input = child.0.stdin.take().unwrap();
	let stderr = child.0.stderr.take().unwrap();
	let (send, events) = mpsc::channel();
	let reader = std::thread::spawn(move || {
		let mut text = String::new();
		for line in BufReader::new(stderr).lines() {
			let line = line.unwrap();
			if line.contains("AWAIT270-READY") {
				send.send(("ready", Instant::now())).unwrap();
			}
			if line == "interrupted" {
				send.send(("interrupted", Instant::now())).unwrap();
			}
			text.push_str(&line);
			text.push('\n');
		}
		text
	});
	writeln!(input, "let v=[1];let n=1;0").unwrap();
	let wait = if http {
		format!("http::get({url:?}).await")
	} else {
		"time::sleep(10000).await".into()
	};
	let wait = if nested {
		writeln!(
			input,
			"async fn c(){{{wait}}}\nasync fn b(){{c().await}}\nasync fn a(){{b().await}}"
		)
		.unwrap();
		"a().await".to_owned()
	} else {
		wait
	};
	writeln!(input,"v.push(2);n=2;let staged=3;io::eprint(\"AWAIT270-READY\\n\");match {wait} {{Ok(_)=>v.push(99),Err(_)=>v.push(98)}};v.push(97);7").unwrap();
	input.flush().unwrap();
	let ready = match events.recv_timeout(Duration::from_secs(10)) {
		Ok(event) => event,
		Err(error) => {
			let _ = child.0.kill();
			let _ = child.0.wait();
			let stderr = reader.join().unwrap();
			panic!("{engine} http={http}: no readiness ({error}): {stderr}");
		}
	};
	assert_eq!(ready.0, "ready");
	if http {
		rx.recv_timeout(Duration::from_secs(10)).unwrap();
	} else {
		std::thread::sleep(Duration::from_millis(20));
	}
	let sent = Instant::now();
	assert_eq!(unsafe { libc::kill(child.0.id() as i32, libc::SIGINT) }, 0);
	let (event, ended) = events.recv_timeout(Duration::from_secs(3)).unwrap();
	assert_eq!(event, "interrupted");
	let elapsed = ended.duration_since(sent);
	assert!(
		elapsed < Duration::from_millis(250),
		"{engine} {http}: {elapsed:?}"
	);
	writeln!(input, "(v,n)\nstaged\ntime::sleep(0).await?;42\n:quit").unwrap();
	drop(input);
	let mut stdout = String::new();
	child
		.0
		.stdout
		.take()
		.unwrap()
		.read_to_string(&mut stdout)
		.unwrap();
	assert!(child.0.wait().unwrap().success());
	let stderr = reader.join().unwrap();
	assert!(
		stdout.contains("([1, 2], 1)") && stdout.contains("42"),
		"{stdout}\n{stderr}"
	);
	assert!(stderr.contains("staged"), "{stderr}");
	if let Some(server) = server {
		server.join().unwrap();
	}
	elapsed.as_secs_f64() * 1000.
}
#[test]
fn pending_wait_is_terminal_bounded_and_next_input_recovers_in_both_engines() {
	let mut rows = Vec::new();
	for engine in ["legacy", "execution"] {
		for http in [false, true] {
			rows.push(
				serde_json::json!({"engine":engine,"http":http,"interrupt_ms":pending(engine,http,false)}),
			);
		}
	}
	println!(
		"AWAIT-INTERRUPT-JSON {}",
		serde_json::to_string(&rows).unwrap()
	);
}

#[test]
fn nested_script_wait_is_terminal_and_bindings_survive_a_real_signal() {
	for engine in ["legacy", "execution"] {
		for http in [false, true] {
			pending(engine, http, true);
		}
	}
}

#[test]
fn async_script_fetches_loopback_once_from_an_earlier_input() {
	for engine in ["legacy", "execution"] {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let url = format!("http://{}/", listener.local_addr().unwrap());
		let server = std::thread::spawn(move || {
			let (mut socket, _) = listener.accept().unwrap();
			socket
				.set_read_timeout(Some(Duration::from_secs(5)))
				.unwrap();
			let mut reader = BufReader::new(socket.try_clone().unwrap());
			loop {
				let mut line = String::new();
				reader.read_line(&mut line).unwrap();
				if line == "\r\n" {
					break;
				}
			}
			socket
				.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
				.unwrap();
		});
		let child = Command::new(env!("CARGO_BIN_EXE_rnx"))
			.args(["repl", &format!("--engine={engine}")])
			.env_clear()
			.env("PATH", std::env::var_os("PATH").unwrap())
			.env("NO_PROXY", "*")
			.env("TERM", "dumb")
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		let mut child = Running(child);
		let mut input = child.0.stdin.take().unwrap();
		writeln!(
			input,
			"async fn fetch(url){{http::get(url).await}}\nfetch({url:?}).await?.body\n:quit"
		)
		.unwrap();
		drop(input);
		let mut output = String::new();
		child
			.0
			.stdout
			.take()
			.unwrap()
			.read_to_string(&mut output)
			.unwrap();
		let mut errors = String::new();
		child
			.0
			.stderr
			.take()
			.unwrap()
			.read_to_string(&mut errors)
			.unwrap();
		assert!(child.0.wait().unwrap().success(), "{errors}");
		assert_eq!(
			output.matches("\"ok\"").count(),
			1,
			"{engine}: {output}\n{errors}"
		);
		server.join().unwrap();
	}
}
