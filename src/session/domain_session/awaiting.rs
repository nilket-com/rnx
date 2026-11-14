//! The existing sliced invocation, outside Tokio only for a waiting entry.
use super::*;

pub(super) struct Locations<'a> {
	pub maps: &'a BTreeMap<u64, SourceMap>,
	pub inputs: &'a [String],
	pub numberings: &'a [usize],
	pub awaits: bool,
}

pub(super) fn run(
	domain: &mut ProgramDomain,
	key: &rune::experimental_core::domain::ProgramKey,
	entry: u32,
	restored: &DomainRoot,
	budget: usize,
	interrupted: &mut impl FnMut() -> bool,
	locations: Locations<'_>,
) -> std::result::Result<DomainRoot, Failure> {
	let mut invocation = domain
		.start(key, entry, &[restored], budget)
		.map_err(|e| fault(format!("{e:?}")))?;
	loop {
		if interrupted() {
			let _failure = invocation.cancel();
			return Err(Failure::Interrupted);
		}
		match invocation.resume(core::num::NonZeroUsize::new(1024).unwrap()) {
			CrossOutcome::Limited => {}
			CrossOutcome::Complete(root) => {
				return if locations.awaits && interrupted() {
					Err(Failure::Interrupted)
				} else {
					Ok(root)
				};
			}
			CrossOutcome::Failed(f) => {
				if crate::execute::host_wait_interrupted() {
					return Err(Failure::Interrupted);
				}
				if f.failure.error.kind == rune::experimental_core::error::ErrorKind::FuelExhausted
				{
					return Err(Failure::Budget(budget));
				}
				let origin = f
					.failure
					.error
					.location
					.and_then(|loc| {
						locations
							.maps
							.get(&loc.revision)
							.and_then(|m| m.locate(loc.start as usize))
					})
					.and_then(|(i, o)| origin_in(locations.inputs, locations.numberings, i, o));
				let message =
					frames::render(&f, locations.maps, locations.inputs, locations.numberings);
				return Err(Failure::Runtime { message, origin });
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn context() -> Context {
		let mut c = Context::with_default_modules().unwrap();
		crate::install_core(&mut c).unwrap();
		crate::time::install(&mut c).unwrap();
		c
	}
	fn session() -> DomainSession {
		DomainSession::for_frontend(context(), DEFAULT_CEILING, remedy::FrontEnd::Terminal).unwrap()
	}
	fn text(s: &mut DomainSession, input: &str) -> std::result::Result<Option<String>, Failure> {
		s.eval_rendered(input, &crate::format::Limits::default())
	}
	#[test]
	fn direct_host_waits_preserve_result_and_expression_semantics() {
		let mut fresh = session();
		let mut old = Session::new(context()).unwrap();
		for input in [
			"time::sleep(0).await",
			"time::sleep(-1).await",
			"time::sleep(68719476736).await",
			"let n=0;for _ in 0..3 {time::sleep({n+=1;0}).await?;}n",
			"if true {time::sleep(0).await?;7}else{9}",
		] {
			let before = old.eval(input).unwrap();
			let expected = if crate::runner::is_unit(&before) {
				None
			} else {
				Some(crate::format::render(
					&before,
					None,
					&crate::format::Limits::default(),
				))
			};
			assert_eq!(text(&mut fresh, input).unwrap(), expected, "{input}");
			assert_eq!(fresh.last_compile_times.len(), 2, "{input}");
		}
		assert_eq!(text(&mut fresh, "42").unwrap(), Some("42".into()));
		assert_eq!(fresh.last_compile_times.len(), 1);
		fresh.close().unwrap();
	}
	#[test]
	fn pending_time_does_not_consume_work_and_sleep_obeys_elapsed_lower_bound() {
		use std::time::{Duration, Instant};
		let mut minimum = None;
		for budget in 1..=128 {
			let mut s = session();
			s.budget = budget;
			match text(&mut s, "time::sleep(0).await?;7") {
				Ok(value) => {
					assert_eq!(value, Some("7".into()));
					minimum = Some(budget);
					s.close().unwrap();
					break;
				}
				Err(Failure::Budget(n)) => assert_eq!(n, budget),
				Err(error) => panic!("unexpected budget control error: {error}"),
			}
			s.close().unwrap();
		}
		let minimum = minimum.expect("zero wait exceeded the test work bound");
		for milliseconds in [0, 35] {
			for budget in [minimum - 1, minimum] {
				let mut s = session();
				s.budget = budget;
				let start = Instant::now();
				let result = text(&mut s, &format!("time::sleep({milliseconds}).await?;7"));
				if budget == minimum {
					assert_eq!(result.unwrap(), Some("7".into()));
					assert!(start.elapsed() >= Duration::from_millis(milliseconds));
				} else {
					assert!(matches!(result, Err(Failure::Budget(n)) if n == budget));
				}
				s.close().unwrap();
			}
		}
	}
	#[test]
	fn async_host_refusals_name_the_function_and_exact_call_location() {
		let mut fresh = session();
		for (input, message, column) in [
			(
				"time::sleep(0)",
				"time::sleep is asynchronous; await its call as time::sleep(...).await",
				1,
			),
			(
				"let f=time::sleep;0",
				"time::sleep is asynchronous; futures as values are not available; await its call as time::sleep(...).await",
				7,
			),
		] {
			let number = fresh.inputs.len() + 1;
			assert_eq!(
				text(&mut fresh, input).unwrap_err().to_string(),
				format!("refused: {message} (input {number}, line 1, column {column})")
			);
		}
		text(&mut fresh, "let a=[1];0").unwrap();
		let error = text(
			&mut fresh,
			"async fn main(){a.push(2);time::sleep(0).await}",
		)
		.unwrap_err();
		// Input declarations cannot replace the session's generated entry, even
		// when their calling convention matches its asynchronous retry wrapper.
		assert!(
			error
				.to_string()
				.contains("`main` is reserved by the session")
		);
		assert_eq!(text(&mut fresh, "a.len()").unwrap(), Some("1".into()));
	}
	#[test]
	fn ordinary_input_keeps_the_published_completion_boundary() {
		let mut s = session();
		let mut checks = 0;
		let result = s.eval_interruptible("7", || {
			checks += 1;
			checks >= 2
		});
		assert!(result.is_ok());
		assert_eq!(
			checks, 1,
			"ordinary completion must not introduce a new interrupt check"
		);
		drop(result);
		s.close().unwrap();
	}
	#[test]
	fn neighboring_forms_are_named_and_never_run_effects() {
		let mut fresh = session();
		text(&mut fresh, "let a=[1];0").unwrap();
		for (source, reason) in [
			("time::sleep(0)", "await its call"),
			("let f=time::sleep(0);0", "await its call"),
			("let f=time::sleep;0", "futures as values"),
			("async {7}.await", "an async block"),
			(
				"let f=async ||7;0",
				"a call with unsupported or unknown closure arity",
			),
			("select {_ = time::sleep(0)=>7}", "select"),
		] {
			let error = text(&mut fresh, &format!("a.push(2);{source}"))
				.unwrap_err()
				.to_string();
			assert!(error.contains(reason), "{source}: {error}");
			if matches!(reason, "an async block" | "an async function" | "select") {
				assert_eq!(
					error.split(" (input ").next().unwrap(),
					format!(
						"refused: {reason} is not available in an execution session; start `rnx repl` without --engine=execution"
					)
				);
			}
			assert_eq!(text(&mut fresh, "a.len()").unwrap(), Some("1".into()));
		}
	}
	#[test]
	fn ordinary_functions_keep_legacy_await_error_and_retry_diagnostics_are_located() {
		let mut fresh = session();
		let mut old = Session::new(context()).unwrap();
		for input in [
			"fn pause(){time::sleep(0).await}",
			"time::sleep(0).await;missing_name",
			"let await_word=\"await\";missing_name",
			"let await_word=\"async input\";missing_again",
		] {
			let legacy = old.eval(input).unwrap_err();
			let error = text(&mut fresh, input).unwrap_err();
			match (legacy, error) {
				(
					Failure::Compile {
						message: a,
						origin: oa,
					},
					Failure::Compile {
						message: b,
						origin: ob,
					},
				) => assert_eq!((a, oa), (b, ob), "{input}"),
				(a, b) => panic!("{input}: {a:?} vs {b:?}"),
			}
		}
	}
	#[test]
	fn interrupt_at_completed_input_does_not_publish_delta() {
		let mut s = session();
		text(&mut s, "let v=[1];let n=1;0").unwrap();
		let mut checks = 0;
		let result = s.eval_interruptible("v.push(2);n=2;time::sleep(0).await?;7", || {
			checks += 1;
			checks >= 2
		});
		assert!(matches!(result, Err(Failure::Interrupted)));
		assert_eq!(checks, 2);
		assert_eq!(text(&mut s, "(v,n)").unwrap(), Some("([1, 2], 1)".into()));
		assert_eq!(
			text(&mut s, "time::sleep(0).await?;9").unwrap(),
			Some("9".into())
		);
		s.close().unwrap();
	}
	#[test]
	fn runtime_context_matches_legacy_in_pure_and_awaiting_inputs() {
		fn c() -> Context {
			let mut c = context();
			let mut m = rune::Module::with_crate("ctx270").unwrap();
			m.host_function("present", || tokio::runtime::Handle::try_current().is_ok())
				.unwrap();
			m.host_function("spawn", || {
				let Ok(handle) = tokio::runtime::Handle::try_current() else {
					return false;
				};
				handle.spawn(async {});
				true
			})
			.unwrap();
			m.host_function("timer", || {
				if tokio::runtime::Handle::try_current().is_err() {
					return false;
				}
				drop(tokio::time::sleep(std::time::Duration::ZERO));
				true
			})
			.unwrap();
			c.install(m).unwrap();
			c
		}
		let mut fresh = DomainSession::new(c(), DEFAULT_CEILING).unwrap();
		let mut old = Session::new(c()).unwrap();
		for input in [
			"(ctx270::present(),ctx270::spawn(),ctx270::timer())",
			"time::sleep(0).await?;(ctx270::present(),ctx270::spawn(),ctx270::timer())",
			"(ctx270::present(),ctx270::spawn(),ctx270::timer())",
		] {
			let expected = crate::format::render(
				&old.eval(input).unwrap(),
				None,
				&crate::format::Limits::default(),
			);
			assert_eq!(text(&mut fresh, input).unwrap(), Some(expected));
		}
		fresh.close().unwrap();
		old.close().unwrap();
	}
	#[test]
	fn native_panic_and_script_error_restore_wait_runtime() {
		let mut c = context();
		let mut m = rune::Module::new();
		m.host_function("native_panic", || -> i64 { panic!("native control") })
			.unwrap();
		c.install(m).unwrap();
		let mut s = DomainSession::new(c, DEFAULT_CEILING).unwrap();
		for input in [
			"time::sleep(0).await?;1/0",
			"time::sleep(0).await?;native_panic()",
		] {
			assert!(
				matches!(text(&mut s, input), Err(Failure::Runtime { .. })),
				"{input}"
			);
			assert_eq!(text(&mut s, "7").unwrap(), Some("7".into()));
			assert_eq!(
				text(&mut s, "time::sleep(0).await?;9").unwrap(),
				Some("9".into())
			);
		}
		s.close().unwrap();
	}
	#[test]
	#[ignore = "native compilation phase measurement"]
	fn compile_retry_cost() {
		let mut s = session();
		let mut observations = Vec::new();
		for round in 0..20 {
			for input in if round % 2 == 0 {
				["7", "time::sleep(0).await"]
			} else {
				["time::sleep(0).await", "7"]
			} {
				text(&mut s, input).unwrap();
				if round > 1 {
					observations.push(serde_json::json!({"input":input,"nanoseconds":s.last_compile_times.iter().map(|d|d.as_nanos()).collect::<Vec<_>>()}));
				}
			}
		}
		println!(
			"AWAIT-COMPILE-JSON {}",
			serde_json::to_string(&observations).unwrap()
		);
	}
}

#[cfg(test)]
mod http_tests {
	use super::*;
	use std::{
		io::{Read, Write},
		net::TcpListener,
		time::Duration,
	};
	fn contexts() -> (DomainSession, Session) {
		fn make() -> (Context, crate::http::State, crate::lifecycle::Lifecycle) {
			let mut c = Context::with_default_modules().unwrap();
			crate::install_core(&mut c).unwrap();
			crate::time::install(&mut c).unwrap();
			let state = crate::http::State::default();
			let life = crate::lifecycle::Lifecycle::new(true).unwrap();
			crate::http::install(&mut c, &state, life.scope("http")).unwrap();
			(c, state, life)
		}
		let (c, h, l) = make();
		let fresh = DomainSession::new(c, DEFAULT_CEILING)
			.unwrap()
			.with_http(h)
			.with_lifecycle(l);
		let (c, h, l) = make();
		let old = Session::new(c).unwrap().with_http(h).with_lifecycle(l);
		(fresh, old)
	}
	fn equal(fresh: &mut DomainSession, old: &mut Session, input: &str) -> String {
		let before = old.eval(input).unwrap();
		let expected = crate::format::render(&before, None, &crate::format::Limits::default());
		let actual = fresh
			.eval_rendered(input, &crate::format::Limits::default())
			.unwrap()
			.unwrap();
		assert_eq!(actual, expected, "{input}");
		actual
	}
	fn server() -> (String, std::thread::JoinHandle<Vec<String>>) {
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		let url = format!(
			"http://127.0.0.1:{}/",
			listener.local_addr().unwrap().port()
		);
		let handle = std::thread::spawn(move || {
			let mut requests = Vec::new();
			for _ in 0..2 {
				let (mut stream, _) = listener.accept().unwrap();
				stream
					.set_read_timeout(Some(Duration::from_secs(3)))
					.unwrap();
				let mut bytes = Vec::new();
				loop {
					let mut byte = [0u8; 1];
					stream.read_exact(&mut byte).unwrap();
					bytes.push(byte[0]);
					if bytes.ends_with(b"\r\n\r\n") {
						break;
					}
					assert!(bytes.len() < 16384);
				}
				let headers = String::from_utf8(bytes).unwrap();
				let n = headers
					.lines()
					.find_map(|l| {
						l.to_lowercase()
							.strip_prefix("content-length:")
							.map(|v| v.trim().parse::<usize>().unwrap())
					})
					.unwrap_or(0);
				let mut body = vec![0; n];
				stream.read_exact(&mut body).unwrap();
				requests.push(format!("{headers}{}", String::from_utf8(body).unwrap()));
				stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\nX-Control: fixed\r\n\r\n{\"ok\":true}").unwrap();
			}
			requests
		});
		(url, handle)
	}
	#[test]
	fn get_request_headers_body_and_response_bounds_match_on_loopback() {
		let (mut fresh, mut old) = contexts();
		for kind in ["get", "request", "bound"] {
			let (url, server) = server();
			let input = match kind {
				"get" => format!("http::get({url:?}).await"),
				"request" => format!(
					"http::request(\"POST\",{url:?},#{{headers:#{{\"X-Probe\":\"yes\"}},body:\"payload\",timeout_ms:1000,body_limit:100}}).await"
				),
				_ => format!("http::request(\"GET\",{url:?},#{{body_limit:4}}).await"),
			};
			let result = equal(&mut fresh, &mut old, &input);
			let requests = server.join().unwrap();
			assert_eq!(requests.len(), 2);
			if kind == "request" {
				for r in requests {
					assert!(
						r.starts_with("POST ")
							&& r.ends_with("payload")
							&& r.to_lowercase().contains("x-probe: yes")
					);
				}
			}
			if kind == "bound" {
				assert!(result.contains("limit of 4 bytes"));
			} else {
				assert!(result.starts_with("Ok("));
			}
		}
		fresh.close().unwrap();
		old.close().unwrap();
	}
	#[test]
	fn explicit_embedding_snapshot_budget_fails_by_name_before_a_request() {
		let (mut fresh, old) = contexts();
		fresh.domain = ProgramDomain::new(
			rune::experimental_core::heap::Limits {
				bytes: 20000,
				..Default::default()
			},
			Default::default(),
			fresh.context.execution_runtime().unwrap(),
		)
		.unwrap();
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		listener.set_nonblocking(true).unwrap();
		let input = format!(
			"http::request(\"POST\",\"http://{}/\",#{{body:{:?}}}).await",
			listener.local_addr().unwrap(),
			"x".repeat(16000)
		);
		let error = fresh
			.eval_rendered(&input, &crate::format::Limits::default())
			.unwrap_err();
		assert!(
			error
				.to_string()
				.contains("owned host input bytes: StoreCapacity"),
			"{error}"
		);
		assert!(matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock));
		assert_eq!(
			fresh
				.eval_rendered("7", &crate::format::Limits::default())
				.unwrap(),
			Some("7".into())
		);
		fresh.close().unwrap();
		old.close().unwrap();
	}
	#[test]
	fn malformed_inputs_and_options_errors_match_without_an_external_host() {
		let (mut fresh, mut old) = contexts();
		let socket = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
		let port = socket.local_addr().unwrap().port();
		drop(socket);
		for input in [
			format!("http::get(\"http://127.0.0.1:{port}/\").await"),
			"http::get(\"not a url\").await".into(),
			"http::request(\"GET\",\"http://127.0.0.1/\",#{timeout_ms:0}).await".into(),
			"http::request(\"GET\",\"http://127.0.0.1/\",#{body_limit:67108865}).await".into(),
			"http::request(\"GET\",\"http://127.0.0.1/\",42).await".into(),
			"http::request(\"GET\",\"http://127.0.0.1/\",#{headers:#{a:7}}).await".into(),
			"let o=#{};o[\"headers\"]=o;http::request(\"GET\",\"http://127.0.0.1/\",o).await"
				.into(),
		] {
			let r = equal(&mut fresh, &mut old, &input);
			assert!(r.starts_with("Err("), "{input}: {r}");
		}
		for input in [
			"http::get_bytes(\"\").await",
			"http::request_bytes(\"GET\",\"\",#{}).await",
		] {
			let error = fresh
				.eval_rendered(input, &crate::format::Limits::default())
				.unwrap_err()
				.to_string();
			assert!(error.contains("bytes"), "{error}");
		}
	}
	#[test]
	fn unenrolled_async_hosts_never_recommend_an_unavailable_await() {
		let (mut fresh, old) = contexts();
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		listener.set_nonblocking(true).unwrap();
		let url = format!("http://{}/", listener.local_addr().unwrap());
		fresh
			.eval_rendered("let unchanged=[1];0", &crate::format::Limits::default())
			.unwrap();
		for (path, arguments, reason) in [
			("std::future::join", "()".to_owned(), "host enrollment"),
			("http::get_bytes", format!("{url:?}"), "bytes"),
			(
				"http::request_bytes",
				format!("\"GET\",{url:?},#{{}}"),
				"bytes",
			),
		] {
			for expression in [
				format!("{path}({arguments})"),
				path.to_owned(),
				format!("{path}({arguments}).await"),
			] {
				let input = format!("unchanged.push(2);{expression}");
				let result = fresh.eval_rendered(&input, &crate::format::Limits::default());
				let error = result.unwrap_err().to_string();
				assert!(error.contains(path), "{input}: {error}");
				assert!(error.contains(reason), "{input}: {error}");
				assert!(
					!error.contains("await its call") && !error.contains("as a future value"),
					"{input}: {error}"
				);
				assert_eq!(
					fresh
						.eval_rendered("unchanged.len()", &crate::format::Limits::default())
						.unwrap(),
					Some("1".into()),
					"{input}: effect before refusal"
				);
				assert!(
					matches!(listener.accept(), Err(e) if e.kind() == std::io::ErrorKind::WouldBlock)
				);
			}
		}
		assert_eq!(
			fresh
				.eval_rendered("7", &crate::format::Limits::default())
				.unwrap(),
			Some("7".into())
		);
		fresh.close().unwrap();
		old.close().unwrap();
	}
	#[test]
	fn moved_options_stop_at_the_call_without_effect_or_binding_loss() {
		let (mut fresh, mut old) = contexts();
		fresh
			.eval_rendered(
				"let earlier=[1];let n=7;0",
				&crate::format::Limits::default(),
			)
			.unwrap();
		let listener = TcpListener::bind("127.0.0.1:0").unwrap();
		listener.set_nonblocking(true).unwrap();
		let local = format!("http://{}/", listener.local_addr().unwrap());
		for url in ["not a url", local.as_str()] {
			let input = format!(
				"let moved=[1];let options=#{{unknown:moved}};drop(moved);http::request(\"GET\",{url:?},options).await"
			);
			let legacy = old.eval(&input).unwrap();
			let rendered = crate::format::render(&legacy, None, &crate::format::Limits::default());
			assert!(rendered.starts_with("Err(\"cannot get "), "{rendered}");
			let number = fresh.inputs.len() + 1;
			let column = input.find("http::request").unwrap() + 1;
			let error = fresh
				.eval_rendered(&input, &crate::format::Limits::default())
				.unwrap_err();
			let Failure::Runtime {
				message,
				origin: Some(origin),
			} = error
			else {
				panic!("wrong moved-value failure: {error:?}")
			};
			assert_eq!(
				message,
				format!(
					"Cannot read the value: it has been moved\n  at input {number}, line 1, column {column}"
				)
			);
			assert_eq!(
				(origin.input, origin.line, origin.column),
				(number, 1, column)
			);
			assert!(
				matches!(listener.accept(),Err(e) if e.kind()==std::io::ErrorKind::WouldBlock),
				"moved snapshot must send no request"
			);
			assert_eq!(
				fresh
					.eval_rendered("(earlier,n)", &crate::format::Limits::default())
					.unwrap(),
				Some("([1], 7)".into())
			);
			assert_eq!(
				fresh
					.eval_rendered("time::sleep(0).await?;7", &crate::format::Limits::default())
					.unwrap(),
				Some("7".into())
			);
		}
		fresh.close().unwrap();
		old.close().unwrap();
	}
	#[test]
	fn live_unknown_options_preserve_the_parsers_error_selection() {
		let (mut fresh, mut old) = contexts();
		for url in ["not a url", "http://127.0.0.1/"] {
			for value in ["[1]", "#{nested:[1]}", "||7"] {
				let input = format!("http::request(\"GET\",{url:?},#{{unknown:{value}}}).await");
				assert!(equal(&mut fresh, &mut old, &input).starts_with("Err("));
			}
		}
		fresh.close().unwrap();
		old.close().unwrap();
	}
}
