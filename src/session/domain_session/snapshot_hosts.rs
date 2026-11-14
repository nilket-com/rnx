//! ADR0275: observed parser parity and snapshot access errors.
use super::transcripts::parity_observe;
use super::*;
fn contexts() -> (Session, DomainSession) {
	let mut a = Context::with_default_modules().unwrap();
	crate::install_core(&mut a).unwrap();
	crate::time::install(&mut a).unwrap();
	crate::text::install(&mut a).unwrap();
	let mut b = Context::with_default_modules().unwrap();
	crate::install_core(&mut b).unwrap();
	crate::time::install(&mut b).unwrap();
	crate::text::install(&mut b).unwrap();
	(
		Session::new(a).unwrap(),
		DomainSession::new(b, DEFAULT_CEILING).unwrap(),
	)
}
#[test]
fn time_and_response_errors_options_and_owned_results() {
	let (mut old, mut new) = contexts();
	for input in [
		"time::from_parts(#{year:1970,month:1,day:1},\"UTC\")",
		"let stamp=time::from_parts(time::parts(0,\"UTC\").unwrap(),\"UTC\");stamp",
		"time::from_parts(#{},\"UTC\")",
		"time::from_parts(#{year:1970,month:13,day:1},\"UTC\")",
		"time::from_parts(#{year:2023,month:2,day:29},\"UTC\")",
		"time::from_parts(#{year:1970,month:1,day:1,hour:24},\"UTC\")",
		"time::from_parts(#{year:1970,month:1,day:1,minute:60},\"UTC\")",
		"time::from_parts(#{year:1970,month:1,day:1,second:60},\"UTC\")",
		"time::from_parts(#{year:1970,month:1,day:1,millisecond:1000},\"UTC\")",
		"time::from_parts(#{year:[1],month:1,day:1},\"UTC\")",
		"time::from_parts(#{year:1970,month:1,day:1,ignored:||1},\"UTC\")",
		"time::from_parts(#{year:1970,month:1,day:1},\"invalid-zone\")",
		"time::from_parts(#{year:2024,month:3,day:10,hour:2,minute:30},\"America/New_York\")",
		"time::from_parts(#{year:2024,month:11,day:3,hour:1,minute:30},\"America/New_York\")",
		"time::from_parts(#{year:2024,month:11,day:3,hour:1,minute:30,offset_seconds:-14400},\"America/New_York\")",
		"time::from_parts(#{year:1970,month:1,day:1,offset_seconds:93600},\"UTC\")",
		"web::response(200,\"hello\",\"text/plain\",#{\"X-Test\":[\"a\",\"b\"]})",
		"web::response(199,\"x\",\"text/plain\",#{})",
		"web::response(600,\"x\",\"text/plain\",#{})",
		"web::response(204,\"x\",\"text/plain\",#{})",
		"web::response(304,\"\",\"text/plain\",#{})",
		"web::response(200,42,\"text/plain\",#{})",
		"web::response(200,\"x\",\"bad\\n\",#{})",
		"web::response(200,\"x\",\"text/plain\",#{\"Content-Type\":\"x\"})",
		"web::response(200,\"x\",\"text/plain\",#{\"content-length\":\"1\"})",
		"web::response(200,\"x\",\"text/plain\",#{\"bad name\":\"x\"})",
		"web::response(200,\"x\",\"text/plain\",#{\"x\":1})",
		"web::response(200,\"x\",\"text/plain\",#{\"x\":[\"bad\\n\"]})",
		"web::response(200,\"x\",\"text/plain\",#{\"x\":[]})",
		"let body=String::from(\"body\");let headers=#{\"x\":[\"v\"]};let response=web::response(200,body,\"text/plain\",headers).unwrap();headers[\"x\"].push(\"later\");(response.body,response.headers[\"x\"],body)",
	] {
		let (a, b) = parity_observe(&mut old, &mut new, input);
		assert_eq!(a["outcome"], "ok", "stock must execute: {input}: {a}");
		assert_eq!(a, b, "{input}");
	}
	old.close().unwrap();
	new.close().unwrap();
}
#[test]
fn moved_descendants_stop_before_handler_even_in_ignored_fields() {
	for (input, host) in [
		(
			"let v=[1];let p=#{year:1970,month:1,day:1,ignored:v};drop(v);time::from_parts(p,\"UTC\")",
			"time::from_parts",
		),
		(
			"let v=[1];let p=#{year:v,month:1,day:1};drop(v);time::from_parts(p,\"UTC\")",
			"time::from_parts",
		),
		(
			"let v=[1];let p=#{unknown:v};drop(v);process::run(\"\",[],p)",
			"process::run",
		),
		(
			"let v=String::from(\"value\");let a=[v];drop(v);process::run(\"\",a,#{})",
			"process::run",
		),
		(
			"let v=[\"a\"];let h=#{\"x\":v};drop(v);web::response(199,\"body\",\"text/plain\",h)",
			"web::response",
		),
	] {
		let (mut old, mut new) = contexts();
		let (a, b) = parity_observe(&mut old, &mut new, input);
		assert_eq!(a["outcome"], "ok", "{input}: {a}");
		assert_eq!(b["outcome"], "runtime");
		let column = input.find(host).unwrap() + 1;
		assert_eq!(
			b["message"],
			format!(
				"Cannot read the value: it has been moved\n  at input 1, line 1, column {column}"
			)
		);
		assert_eq!(
			new.eval_rendered("7", &crate::format::Limits::default())
				.unwrap(),
			Some("7".into())
		);
		old.close().unwrap();
		new.close().unwrap();
	}
}

#[test]
fn wrong_root_is_a_runtime_type_error_not_a_calendar_result() {
	for input in [
		"time::from_parts(42,\"UTC\")",
		"time::from_parts([],\"UTC\")",
		"time::from_parts(None,\"UTC\")",
		"struct P{x} time::from_parts(P{x:1},\"UTC\")",
	] {
		let (mut old, mut new) = contexts();
		let (a, b) = parity_observe(&mut old, &mut new, input);
		assert_eq!(a["outcome"], "runtime", "{input}: {a}");
		assert_eq!(b["outcome"], "runtime", "{input}: {b}");
		assert!(a["message"].as_str().unwrap().contains("Expected type"));
		assert!(b["message"].as_str().unwrap().contains("Expected type"));
		old.close().unwrap();
		new.close().unwrap();
	}
}

#[cfg(unix)]
struct Private(std::path::PathBuf);
#[cfg(unix)]
impl Private {
	fn new() -> Self {
		static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
		let dir = std::env::temp_dir().join(format!(
			"rnx-0275-{}-{}",
			std::process::id(),
			NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
		));
		std::fs::create_dir(&dir).unwrap();
		let exe = dir.join("child");
		std::fs::hard_link(std::env::current_exe().unwrap(), &exe)
			.or_else(|_| std::fs::copy(std::env::current_exe().unwrap(), &exe).map(|_| ()))
			.unwrap();
		Self(dir)
	}
	fn input(&self, mode: &str, options: &str) -> String {
		format!(
			"process::run({:?},[\"--exact\",\"session::domain_session::snapshot_hosts::process_fixture\",\"--nocapture\"],#{{cwd:{:?},env_clear:true,env:#{{RNX_0275_CHILD:{mode:?},RNX_0275_VALUE:\"fixture\",RNX_0275_SOCKET:{:?}}},{options}}})",
			self.0.join("child").to_str().unwrap(),
			self.0.to_str().unwrap(),
			self.0.join("ready").to_str().unwrap()
		)
	}
}
#[cfg(unix)]
impl Drop for Private {
	fn drop(&mut self) {
		let _ = std::fs::remove_dir_all(&self.0);
	}
}

#[test]
#[cfg(unix)]
fn process_fixture() {
	let Ok(mode) = std::env::var("RNX_0275_CHILD") else {
		return;
	};
	std::fs::write("pid", std::process::id().to_string()).unwrap();
	if mode == "wait" {
		use std::io::{Read, Write};
		let mut socket =
			std::os::unix::net::UnixStream::connect(std::env::var("RNX_0275_SOCKET").unwrap())
				.unwrap();
		socket.write_all(&std::process::id().to_ne_bytes()).unwrap();
		let mut byte = [0];
		let _ = socket.read(&mut byte);
	} else if mode == "timeout" {
		std::thread::sleep(std::time::Duration::from_secs(30));
	} else {
		use std::io::Read;
		let mut input = String::new();
		std::io::stdin().read_to_string(&mut input).unwrap();
		assert_eq!(input, "fixed stdin");
		assert_eq!(std::env::var("RNX_0275_VALUE").unwrap(), "fixture");
		println!("fixed stdout");
		eprintln!("fixed stderr");
		std::process::exit(7);
	}
}

#[test]
#[cfg(unix)]
fn supervised_child_capture_timeout_and_bytes_refusal() {
	let dir = Private::new();
	let (mut old, mut new) = contexts();
	let input = format!(
		"let r={}.unwrap();(r.code,text::find(r.stdout,\"fixed stdout\").is_some(),text::find(r.stderr,\"fixed stderr\").is_some(),r.cancelled,r.timed_out)",
		dir.input("echo", "input:\"fixed stdin\",timeout_ms:3000")
	);
	let (a, b) = parity_observe(&mut old, &mut new, &input);
	assert_eq!(a, b, "{a} {b}");
	assert_eq!(b["value"], "(7, true, true, false, false)");
	let input = format!(
		"let r={}.unwrap();(r.timed_out,r.cancelled)",
		dir.input("timeout", "timeout_ms:100")
	);
	let (a, b) = parity_observe(&mut old, &mut new, &input);
	assert_eq!(a, b);
	assert_eq!(b["value"], "(true, false)");
	std::fs::remove_file(dir.0.join("pid")).unwrap();
	let input = dir.input("echo", "input:b\"bytes\",timeout_ms:100");
	let error = new.eval(&input).unwrap_err();
	assert!(matches!(error, Failure::Refused(_)));
	assert!(error.to_string().contains("byte"));
	assert!(!dir.0.join("pid").exists());
	assert_eq!(
		new.eval_rendered("7", &crate::format::Limits::default())
			.unwrap(),
		Some("7".into())
	);
	old.close().unwrap();
	new.close().unwrap();
}

#[test]
#[cfg(target_os = "linux")]
fn isolated_interrupt() {
	if std::env::var_os("RNX_0275_INTERRUPT_INNER").is_none() {
		return;
	}
	use std::io::Read;
	use std::time::{Duration, Instant};
	let dir = Private::new();
	let (old, mut new) = contexts();
	let listener = std::os::unix::net::UnixListener::bind(dir.0.join("ready")).unwrap();
	listener.set_nonblocking(true).unwrap();
	let pid = std::process::id();
	let (send, receive) = std::sync::mpsc::channel();
	let signal = std::thread::spawn(move || {
		let deadline = Instant::now() + Duration::from_secs(10);
		let mut socket = loop {
			match listener.accept() {
				Ok((socket, _)) => break socket,
				Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
					assert!(Instant::now() < deadline, "child readiness timeout");
					std::thread::sleep(Duration::from_millis(1));
				}
				Err(e) => panic!("{e}"),
			}
		};
		socket
			.set_read_timeout(Some(Duration::from_secs(5)))
			.unwrap();
		let mut bytes = [0; 4];
		socket.read_exact(&mut bytes).unwrap();
		let child = u32::from_ne_bytes(bytes);
		let started = Instant::now();
		let status = std::process::Command::new("/bin/kill")
			.args(["-INT", &pid.to_string()])
			.status()
			.unwrap();
		assert!(status.success());
		send.send((started, child)).unwrap();
		// Keep the ready connection open until the supervisor ends the child.
		let mut byte = [0];
		let _ = socket.read(&mut byte);
	});
	let error = new
		.eval(&format!(
			"let unpublished=1;{}",
			dir.input("wait", "timeout_ms:5000")
		))
		.unwrap_err();
	assert!(matches!(error, Failure::Interrupted), "{error:?}");
	let (started, child) = receive.recv_timeout(Duration::from_secs(10)).unwrap();
	let elapsed = started.elapsed();
	signal.join().unwrap();
	assert!(
		!std::path::Path::new(&format!("/proc/{child}")).exists(),
		"child not reaped"
	);
	assert!(new.eval("unpublished").is_err());
	assert_eq!(
		new.eval_rendered("7", &crate::format::Limits::default())
			.unwrap(),
		Some("7".into())
	);
	let error = new.eval("1/0").unwrap_err();
	assert!(
		matches!(error, Failure::Runtime { .. }),
		"stale interrupt: {error:?}"
	);
	// Late completion uses the same marker after an already-completed result.
	let mut context = Context::with_default_modules().unwrap();
	crate::install_core(&mut context).unwrap();
	let mut module = rune::Module::new();
	module
		.host_function_with_output(
			"late",
			|| {
				let completed = Ok(rune::to_value(()).unwrap());
				assert!(
					std::process::Command::new("/bin/kill")
						.args(["-INT", &std::process::id().to_string()])
						.status()
						.unwrap()
						.success()
				);
				crate::process::finish_execution(completed)
			},
			rune::host::OutputCertificate::JSON_RESULT,
		)
		.unwrap();
	context.install(module).unwrap();
	let mut late = DomainSession::new(context, DEFAULT_CEILING).unwrap();
	assert!(matches!(
		late.eval("let unpublished=1;late()"),
		Err(Failure::Interrupted)
	));
	assert!(late.eval("unpublished").is_err());
	assert_eq!(
		late.eval_rendered("9", &crate::format::Limits::default())
			.unwrap(),
		Some("9".into())
	);
	late.close().unwrap();
	println!(
		"interrupt latency after ready signal: {} ns",
		elapsed.as_nanos()
	);
	old.close().unwrap();
	new.close().unwrap();
}
#[test]
#[cfg(target_os = "linux")]
fn real_interrupt_is_terminal_reaped_and_does_not_poison_next_input() {
	let output = std::process::Command::new(std::env::current_exe().unwrap())
		.args([
			"--exact",
			"session::domain_session::snapshot_hosts::isolated_interrupt",
			"--nocapture",
		])
		.env("RNX_0275_INTERRUPT_INNER", "1")
		.output()
		.unwrap();
	assert!(
		output.status.success(),
		"{}\n{}",
		String::from_utf8_lossy(&output.stdout),
		String::from_utf8_lossy(&output.stderr)
	);
	print!("{}", String::from_utf8_lossy(&output.stdout));
}

#[test]
fn process_parser_and_response_limits_preserve_stock_errors() {
	let (mut old, mut new) = contexts();
	for input in [
		"process::run(\"\",[],#{})",
		"process::run(\"x\\0\",[],#{})",
		"process::run(\"missing\",42,#{})",
		"process::run(\"missing\",[42],#{})",
		"process::run(\"missing\",[\"x\\0\"],#{})",
		"process::run(\"missing\",[],42)",
		"process::run(\"missing\",[],#{timeout_ms:-1})",
		"process::run(\"missing\",[],#{timeout_ms:0})",
		"process::run(\"missing\",[],#{timeout_ms:90001})",
		"process::run(\"missing\",[],#{input:42})",
		"process::run(\"missing\",[],#{cwd:\"\"})",
		"process::run(\"missing\",[],#{cwd:42})",
		"process::run(\"missing\",[],#{env_clear:1})",
		"process::run(\"missing\",[],#{env:42})",
		"process::run(\"missing\",[],#{env:#{\"\":\"x\"}})",
		"process::run(\"missing\",[],#{env:#{\"x=y\":\"x\"}})",
		"process::run(\"missing\",[],#{env:#{\"x\":42}})",
		"process::run(\"missing\",[],#{unknown:||1})",
		"let large=\"x\";for _ in 0..21 {large=large+large;}web::response(200,large,\"text/plain\",#{})",
	] {
		let (a, b) = parity_observe(&mut old, &mut new, input);
		assert_eq!(a["outcome"], "ok", "{input}: {a}");
		assert_eq!(a, b, "{input}: {a} {b}");
		assert!(
			a["value"].as_str().unwrap().starts_with("Err("),
			"test must stop before launch: {input}: {a}"
		);
	}
	old.close().unwrap();
	new.close().unwrap();
}

#[test]
#[cfg(all(feature = "allocation-peak", feature = "count-allocations"))]
fn simultaneous_packets_near_individual_bounds() {
	use rune::experimental_core::{
		execute::Executor,
		frontend,
		heap::{Limits, Store},
	};
	use rune::{Diagnostics, Hash, Module, Source, Sources};
	use std::sync::{
		Arc,
		atomic::{AtomicUsize, Ordering},
	};
	let mut context = Context::new();
	let mut module = Module::new();
	let count = Arc::new(AtomicUsize::new(0));
	let hit = count.clone();
	module
		.host_function_tree_inputs(
			"measure",
			|_: &str, _: Value, _: Value| 0i64,
			move |_: &str, a: rune::host::OwnedTreeInput, b: rune::host::OwnedTreeInput| {
				for tree in [&a, &b] {
					let root = tree.node(0).unwrap();
					assert_eq!(root.len(), 2);
					for i in 0..2 {
						let id = root.child(i).unwrap().1;
						let rune::host::TreeKind::String(s) = tree.node(id).unwrap().kind() else {
							panic!("payload")
						};
						assert_eq!(s.len(), 24000)
					}
				}
				hit.fetch_add(1, Ordering::SeqCst);
				7i64
			},
			None,
		)
		.unwrap();
	context.install(module).unwrap();
	let text = "x".repeat(24000);
	let source = format!("pub fn main(){{let s={text:?};measure(\"fixture\",[s,s],[s,s])}}");
	let mut sources = Sources::new();
	sources.insert(Source::memory(source).unwrap()).unwrap();
	let p = frontend::compile(&mut sources, &context, &mut Diagnostics::new(), 1).unwrap();
	let mut executor = Executor::new().unwrap();
	executor
		.set_host_runtime(Some(context.execution_runtime().unwrap()))
		.unwrap();
	let mut store = Store::new(Limits {
		entries: 128,
		bytes: 65536,
	})
	.unwrap();
	let baseline = crate::memory::live().unwrap();
	crate::memory::reset_peak();
	let root = executor
		.invoke(
			&p,
			&mut store,
			p.function(Hash::type_hash(["main"])).unwrap(),
			&[],
			100,
		)
		.unwrap();
	let peak = crate::memory::peak().saturating_sub(baseline);
	assert_eq!(count.load(Ordering::SeqCst), 1);
	store.root_release(root).unwrap();
	assert_eq!((store.live_entries(), store.payload_bytes()), (0, 0));
	println!(
		"two individually bounded packets: input limit=65536 bytes each; copied string payload=48000 bytes each; measured allocator peak above invocation baseline={peak} bytes"
	);
}
