//! Record 0034: a whole, bounded HTTP response; JSON stays in crate::json.
use reqwest::{
	Client, Method, Url,
	header::{HeaderMap, HeaderName, HeaderValue},
};
use rune::{
	Context, Module,
	runtime::{Bytes, Object, Value},
};
use std::{
	sync::{Arc, Mutex},
	time::Duration,
};

const DEFAULT_LIMIT: usize = 8 * 1024 * 1024;
const MAX_LIMIT: usize = 64 * 1024 * 1024;

#[derive(Clone, Default)]
pub struct State(Arc<Mutex<Inner>>);
#[derive(Default)]
struct Inner {
	client: Option<Client>,
}

impl State {
	/// Release this owner's cached client. Request revocation belongs to its
	/// lifecycle; socket closure still needs progress from the owning runtime.
	/// This method is safe to call inside a running runtime and never drains it.
	pub fn cancel(&self) {
		let client = self.0.lock().unwrap().client.take();
		drop(client);
	}

	fn client(&self) -> Result<Client, String> {
		let mut inner = self.0.lock().unwrap();
		if inner.client.is_none() {
			let builder = Client::builder()
				.user_agent(concat!("rnx/", env!("CARGO_PKG_VERSION")))
				.redirect(reqwest::redirect::Policy::limited(10));
			#[cfg(feature = "test-support")]
			let builder = builder.dns_resolver(Arc::new(FixtureResolver));
			inner.client = Some(
				builder
					.build()
					.map_err(|e| format!("cannot build HTTP client: {e}"))?,
			);
		}
		Ok(inner.client.as_ref().unwrap().clone())
	}

	async fn request(
		&self,
		url: String,
		request: Result<Request, String>,
		bytes: bool,
	) -> Result<Value, String> {
		let outcome = async {
			let request = request?;
			let client = self.client()?;
			fetch(client, request).await?.value(bytes)
		}
		.await;
		outcome.map_err(|e| format!("cannot get {url}: {e}"))
	}
}

fn string(value: &Value) -> Result<String, String> {
	value
		.borrow_string_ref()
		.map(|s| s.to_string())
		.map_err(|_| "expected a String".into())
}
fn number(value: &Value) -> Result<u64, String> {
	rune::from_value::<u64>(value.clone()).map_err(|_| "expected a non-negative integer".into())
}
fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}

struct Request {
	method: Method,
	url: Url,
	headers: HeaderMap,
	body: Vec<u8>,
	timeout: u64,
	limit: usize,
}
impl Request {
	fn parse(method: &str, url: &str, options: Option<Value>) -> Result<Self, String> {
		let mut request = Self {
			method: Method::from_bytes(method.as_bytes()).map_err(|_| "invalid HTTP method")?,
			url: Url::parse(url).map_err(|e| format!("invalid URL: {e}"))?,
			headers: HeaderMap::new(),
			body: Vec::new(),
			timeout: 30_000,
			limit: DEFAULT_LIMIT,
		};
		if !matches!(request.url.scheme(), "http" | "https") {
			return Err("URL scheme must be http or https".into());
		}
		if let Some(options) = options {
			let object = options
				.borrow_ref::<Object>()
				.map_err(|_| "options must be an object")?;
			for (key, value) in object.iter() {
				request.option(key.as_str(), OptionValue::Legacy(value))?;
			}
		}
		Ok(request)
	}
}

// A bounded view over the legacy object or the existing owned input packet.
// Only fields the request parser reads are interpreted; no second tree is built.
#[derive(Clone, Copy)]
enum OptionValue<'a> {
	Legacy(&'a Value),
	Owned(&'a rune::host::OwnedTreeInput, usize),
}
enum FieldsError {
	NotObject,
	Invalid(&'static str),
	Handler(String),
}
impl FieldsError {
	fn message(self, not_object: &str) -> String {
		match self {
			Self::NotObject => not_object.to_owned(),
			Self::Invalid(message) => message.to_owned(),
			Self::Handler(message) => message,
		}
	}
}
impl<'a> OptionValue<'a> {
	fn string(self) -> Result<String, String> {
		match self {
			Self::Legacy(v) => string(v),
			Self::Owned(tree, id) => match tree.node(id).map(|n| n.kind()) {
				Some(rune::host::TreeKind::String(s)) => Ok(s.as_str().to_owned()),
				Some(rune::host::TreeKind::Scalar(v)) => string(v),
				_ => Err("expected a String".into()),
			},
		}
	}
	fn number(self) -> Result<u64, String> {
		match self {
			Self::Legacy(v) => number(v),
			Self::Owned(tree, id) => match tree.node(id).map(|n| n.kind()) {
				Some(rune::host::TreeKind::Scalar(v)) => number(v),
				_ => Err("expected a non-negative integer".into()),
			},
		}
	}
	fn body(self) -> Result<Vec<u8>, String> {
		if let Ok(text) = self.string() {
			return Ok(text.into_bytes());
		}
		if let Self::Legacy(v) = self {
			return v
				.borrow_ref::<Bytes>()
				.map(|v| v.as_slice().to_vec())
				.map_err(|_| "body must be String or Bytes".into());
		}
		Err("body must be String or Bytes".into())
	}
	fn fields(
		self,
		mut each: impl FnMut(&str, OptionValue<'_>) -> Result<(), String>,
	) -> Result<(), FieldsError> {
		match self {
			Self::Legacy(v) => {
				let object = v
					.borrow_ref::<Object>()
					.map_err(|_| FieldsError::NotObject)?;
				for (key, value) in object.iter() {
					each(key.as_str(), OptionValue::Legacy(value)).map_err(FieldsError::Handler)?;
				}
			}
			Self::Owned(tree, mut id) => {
				let mut node = tree
					.node(id)
					.ok_or(FieldsError::Invalid("invalid owned request options"))?;
				if !matches!(node.kind(), rune::host::TreeKind::Object) {
					return Err(FieldsError::NotObject);
				}
				// The parser only traverses options -> headers. At this depth a
				// back-edge can name only the root options object. Reuse its
				// preserved fields rather than inventing a cycle/type error.
				if node.cycle() {
					id = 0;
					node = tree
						.node(id)
						.ok_or(FieldsError::Invalid("invalid owned request options"))?;
				}
				for n in 0..node.len() {
					let (Some(key), child) = node
						.child(n)
						.ok_or(FieldsError::Invalid("owned request options frontier"))?
					else {
						return Err(FieldsError::Invalid("invalid owned request key"));
					};
					each(key, Self::Owned(tree, child)).map_err(FieldsError::Handler)?;
				}
			}
		}
		Ok(())
	}
}
impl Request {
	fn option(&mut self, key: &str, value: OptionValue<'_>) -> Result<(), String> {
		match key {
			"timeout_ms" => {
				self.timeout = value.number().map_err(|e| format!("timeout_ms: {e}"))?;
				if !(1..=90_000).contains(&self.timeout) {
					return Err("the deadline must be between 1 and 90000 ms".into());
				}
			}
			"body_limit" => {
				let limit = value.number().map_err(|e| format!("body_limit: {e}"))?;
				if !(1..=MAX_LIMIT as u64).contains(&limit) {
					return Err("body_limit must be between 1 and 67108864 bytes".into());
				}
				self.limit = limit as usize;
			}
			"body" => self.body = value.body()?,
			"headers" => value
				.fields(|name, value| {
					let parsed = HeaderName::from_bytes(name.as_bytes())
						.map_err(|_| format!("invalid header name {name:?}"))?;
					let text = value
						.string()
						.map_err(|_| format!("header {name:?} must be a String"))?;
					let value = HeaderValue::from_str(&text)
						.map_err(|_| format!("invalid value for header {name:?}"))?;
					if self.headers.contains_key(&parsed) {
						return Err(format!("duplicate case-insensitive header {name:?}"));
					}
					self.headers.insert(parsed, value);
					Ok(())
				})
				.map_err(|e| e.message("headers must be an object"))?,
			_ => return Err(format!("unknown option {key:?}")),
		}
		Ok(())
	}
	fn parse_owned(
		method: &str,
		url: &str,
		options: &rune::host::OwnedTreeInput,
	) -> Result<Self, String> {
		let mut request = Self::parse(method, url, None)?;
		let value = match options.leaf() {
			Some(v) => OptionValue::Legacy(v),
			None => OptionValue::Owned(options, 0),
		};
		value
			.fields(|key, value| request.option(key, value))
			.map_err(|e| e.message("options must be an object"))?;
		Ok(request)
	}
}

struct Response {
	status: u16,
	url: String,
	headers: Vec<(String, Vec<String>)>,
	body: Vec<u8>,
}
impl Response {
	fn value(self, bytes: bool) -> Result<Value, String> {
		let mut object = Object::new();
		let mut headers = Object::new();
		for (name, values) in self.headers {
			headers
				.insert(
					rune::alloc::String::try_from(name.as_str()).map_err(error)?,
					rune::to_value(values).map_err(error)?,
				)
				.map_err(error)?;
		}
		let body = if bytes {
			rune::to_value(Bytes::from_vec(
				rune::alloc::Vec::try_from(self.body).map_err(error)?,
			))
			.map_err(error)?
		} else {
			let text = String::from_utf8(self.body).map_err(|e| {
				format!(
					"its body is not UTF-8 at byte {}; use http::get_bytes to read it",
					e.utf8_error().valid_up_to()
				)
			})?;
			rune::to_value(text).map_err(error)?
		};
		for (name, value) in [
			("status", Value::from(self.status as i64)),
			("url", rune::to_value(self.url).map_err(error)?),
			("headers", rune::to_value(headers).map_err(error)?),
			("body", body),
		] {
			object
				.insert(rune::alloc::String::try_from(name).map_err(error)?, value)
				.map_err(error)?;
		}
		rune::to_value(object).map_err(error)
	}
}
fn transport(error: reqwest::Error, timeout: u64) -> String {
	if error.is_timeout() {
		return format!("the deadline of {timeout} ms was exceeded");
	}
	if error.is_redirect() {
		return "too many redirects".into();
	}
	// Display alone omits the connection cause. Keep its error chain, without
	// repeating request URLs supplied by reqwest.
	let error = error.without_url();
	let mut message = error.to_string();
	let mut source = std::error::Error::source(&error);
	while let Some(cause) = source {
		message.push_str(": ");
		message.push_str(&cause.to_string());
		source = cause.source();
	}
	message
}
async fn fetch(client: Client, request: Request) -> Result<Response, String> {
	let mut response = client
		.request(request.method.clone(), request.url)
		.headers(request.headers)
		.body(request.body)
		.timeout(Duration::from_millis(request.timeout))
		.send()
		.await
		.map_err(|e| transport(e, request.timeout))?;
	let mut headers = Vec::new();
	for name in response.headers().keys() {
		let mut values = Vec::new();
		for value in response.headers().get_all(name) {
			values.push(
				value
					.to_str()
					.map_err(|_| format!("header {name} is not visible ASCII"))?
					.to_owned(),
			);
		}
		headers.push((name.to_string(), values));
	}
	let mut result = Response {
		status: response.status().as_u16(),
		url: response.url().to_string(),
		headers,
		body: Vec::new(),
	};
	let declared = response.content_length();
	if request.method == Method::HEAD {
		return Ok(result);
	}
	loop {
		match response.chunk().await {
			Ok(Some(chunk)) => {
				if chunk.len() > request.limit - result.body.len() {
					return Err(format!(
						"the body exceeds the limit of {} bytes",
						request.limit
					));
				}
				result.body.extend_from_slice(&chunk);
			}
			Ok(None) => return Ok(result),
			Err(e) => {
				if !e.is_timeout()
					&& let Some(length) = declared
					&& (result.body.len() as u64) < length
				{
					return Err(format!(
						"the body ended after {} of {length} bytes: {}",
						result.body.len(),
						transport(e, request.timeout)
					));
				}
				return Err(transport(e, request.timeout));
			}
		}
	}
}

// Test-only deterministic DNS failure. Other names use the same system
// lookup behavior; fixture tests use IP literals except for this name.
#[cfg(feature = "test-support")]
#[derive(Debug)]
struct FixtureResolver;
#[cfg(feature = "test-support")]
impl reqwest::dns::Resolve for FixtureResolver {
	fn resolve(&self, name: reqwest::dns::Name) -> reqwest::dns::Resolving {
		use std::net::ToSocketAddrs;
		Box::pin(async move {
			if name.as_str() == "rnx-dns-failure.invalid" {
				return Err(std::io::Error::other("injected DNS resolution failure").into());
			}
			if name.as_str() == "rnx-dns-stall.invalid" {
				return tokio::task::spawn_blocking(|| {
					std::thread::sleep(Duration::from_secs(2));
					Err(std::io::Error::other("injected stalled DNS lookup").into())
				})
				.await?;
			}
			let host = name.as_str().to_owned();
			let addresses = tokio::task::spawn_blocking(move || {
				(host.as_str(), 0)
					.to_socket_addrs()
					.map(|a| a.collect::<Vec<_>>())
			})
			.await??;
			Ok(Box::new(addresses.into_iter()) as reqwest::dns::Addrs)
		})
	}
}

pub fn install(
	context: &mut Context,
	state: &State,
	scope: crate::Scope,
) -> crate::Result<Vec<crate::host::HostFunction>> {
	let mut module = Module::with_crate("http")?;
	let mut registered = Vec::new();
	{
		let legacy_state = state.clone();
		let legacy_scope = scope.clone();
		let fresh_state = state.clone();
		let fresh_scope = scope.clone();
		module.host_function_await(
			"get",
			move |url: &str| {
				let url = url.to_owned();
				let request = Request::parse("GET", &url, None);
				let state = legacy_state.clone();
				legacy_scope.track(async move { state.request(url, request, false).await })
			},
			move |url: &str| {
				let url = url.to_owned();
				let request = Request::parse("GET", &url, None);
				let state = fresh_state.clone();
				let future =
					fresh_scope.track(async move { state.request(url, request, false).await });
				crate::execute::result_value(crate::execute::with_native_wait(future)?)
			},
			rune::host::OutputCertificate::JSON_RESULT,
		)?;
	}
	{
		let legacy_state = state.clone();
		let legacy_scope = scope.clone();
		let fresh_state = state.clone();
		let fresh_scope = scope.clone();
		module.host_function_await(
			"request",
			move |method: &str, url: &str, options: Value| {
				let url = url.to_owned();
				let request = Request::parse(method, &url, Some(options));
				let state = legacy_state.clone();
				legacy_scope.track(async move { state.request(url, request, false).await })
			},
			move |method: &str, url: &str, options: rune::host::OwnedTreeInput| {
				let url = url.to_owned();
				let request = Request::parse_owned(method, &url, &options);
				let state = fresh_state.clone();
				drop(options);
				let future =
					fresh_scope.track(async move { state.request(url, request, false).await });
				crate::execute::result_value(crate::execute::with_native_wait(future)?)
			},
			rune::host::OutputCertificate::JSON_RESULT,
		)?;
	}
	{
		let state = state.clone();
		let scope = scope.clone();
		module
			.function("get_bytes", move |url: &str| {
				let url = url.to_owned();
				let request = Request::parse("GET", &url, None);
				let state = state.clone();
				scope.track(async move { state.request(url, request, true).await })
			})
			.build()?;
	}
	{
		let state = state.clone();
		let scope = scope.clone();
		module
			.function(
				"request_bytes",
				move |method: &str, url: &str, options: Value| {
					let url = url.to_owned();
					let request = Request::parse(method, &url, Some(options));
					let state = state.clone();
					scope.track(async move { state.request(url, request, true).await })
				},
			)
			.build()?;
	}
	for (name, doc) in [
		("get", "get(url).await -> Result<object>: bounded HTTP GET"),
		(
			"get_bytes",
			"get_bytes(url).await -> Result<object>: bounded HTTP GET",
		),
		(
			"request",
			"request(method, url, options).await -> Result<object>: bounded HTTP request",
		),
		(
			"request_bytes",
			"request_bytes(method, url, options).await -> Result<object>: bounded HTTP request",
		),
	] {
		registered.push(crate::host::HostFunction {
			path: format!("http::{name}"),
			doc,
		});
	}
	context.install(module)?;
	Ok(registered)
}

#[cfg(test)]
mod tests {
	#[test]
	fn request_parser_accepts_a_body_larger_than_a_small_embedding_snapshot_budget() {
		let mut options = rune::runtime::Object::new();
		options
			.insert(
				rune::alloc::String::try_from("body").unwrap(),
				rune::to_value("x".repeat(16000)).unwrap(),
			)
			.unwrap();
		let request = super::Request::parse(
			"POST",
			"http://127.0.0.1/",
			Some(rune::to_value(options).unwrap()),
		)
		.unwrap();
		assert_eq!(request.body.len(), 16000);
	}
	#[test]
	fn the_module_registers_exactly_four_functions_and_no_json_reader() {
		let mut context = rune::Context::with_default_modules().unwrap();
		let lifecycle = crate::lifecycle::Lifecycle::new(true).unwrap();
		let functions = super::install(
			&mut context,
			&super::State::default(),
			lifecycle.scope("http"),
		)
		.unwrap();
		let names: Vec<_> = functions.iter().map(|f| f.path.as_str()).collect();
		assert_eq!(
			names,
			[
				"http::get",
				"http::get_bytes",
				"http::request",
				"http::request_bytes"
			]
		);
	}
	// This wrapper observes the real inline HTTP request's lifetime, not socket
	// EOF (hyper is entitled to need another runtime turn to close its socket).
	#[test]
	fn retained_rune_http_inner_drops_synchronously_on_failure_and_reset() {
		use std::io::{Read, Write};
		use std::sync::Arc;
		use std::sync::atomic::{AtomicUsize, Ordering};
		struct Guard(Arc<AtomicUsize>);
		impl Drop for Guard {
			fn drop(&mut self) {
				self.0.fetch_sub(1, Ordering::SeqCst);
			}
		}
		for ending in ["panic!(\"failed\")", "loop {}", "reset", "close"] {
			let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
			let url = format!("http://{}/", listener.local_addr().unwrap());
			let (tx, rx) = std::sync::mpsc::channel();
			let server = std::thread::spawn(move || {
				let (mut stream, _) = listener.accept().unwrap();
				stream
					.set_read_timeout(Some(std::time::Duration::from_secs(5)))
					.unwrap();
				let mut request = Vec::new();
				let mut byte = [0];
				while !request.ends_with(b"\r\n\r\n") {
					assert_eq!(stream.read(&mut byte).unwrap(), 1);
					request.push(byte[0]);
				}
				tx.send(()).unwrap();
				assert_eq!(stream.read(&mut byte).unwrap(), 0);
				let _ = stream.flush();
			});
			let life = crate::lifecycle::Lifecycle::new(true).unwrap();
			let scope = life.scope("http");
			let state = super::State::default();
			state.0.lock().unwrap().client =
				Some(reqwest::Client::builder().no_proxy().build().unwrap());
			let live = Arc::new(AtomicUsize::new(0));
			let count = live.clone();
			let owned = state.clone();
			let mut module = rune::Module::with_crate("observed").unwrap();
			module
				.function("get", move || {
					let (state, count, url) = (owned.clone(), count.clone(), url.clone());
					let request = super::Request::parse("GET", &url, None);
					scope.track(async move {
						count.fetch_add(1, Ordering::SeqCst);
						let _guard = Guard(count);
						state.request(url, request, false).await
					})
				})
				.build()
				.unwrap();
			let mut context = rune::Context::with_default_modules().unwrap();
			crate::time::install(&mut context).unwrap();
			context.install(module).unwrap();
			let mut session = crate::session::Session::new(context)
				.unwrap()
				.with_http(state)
				.with_lifecycle(life);
			session.set_budget(10000);
			session.eval("let q = observed::get();").unwrap();
			assert_eq!(live.load(Ordering::SeqCst), 0);
			const SELECT: &str = "select { _ = q => (), _ = time::sleep(20) => () };";
			session.eval(SELECT).unwrap();
			rx.recv_timeout(std::time::Duration::from_secs(5)).unwrap();
			assert_eq!(live.load(Ordering::SeqCst), 1);
			if ending == "close" {
				session.close().unwrap();
				assert_eq!(live.load(Ordering::SeqCst), 0);
				server.join().unwrap();
				continue;
			}
			if ending == "reset" {
				session.reset_fallible().unwrap();
				assert_eq!(
					live.load(Ordering::SeqCst),
					0,
					"reset needs no runtime turn"
				);
				session.eval("42").unwrap();
				session.close().unwrap();
				server.join().unwrap();
				continue;
			}
			assert!(session.eval(ending).is_err());
			assert_eq!(
				live.load(Ordering::SeqCst),
				1,
				"unrelated failure must preserve q"
			);
			assert!(session.eval("let broken = ;").is_err());
			assert_eq!(live.load(Ordering::SeqCst), 1);
			assert!(session.eval(&format!("{SELECT} {ending}")).is_err());
			assert_eq!(
				live.load(Ordering::SeqCst),
				0,
				"no runtime turn since finish"
			);
			let value = session.eval("q.await").unwrap();
			assert_eq!(
				crate::format::render(&value, None, &Default::default()),
				"Err(\"operation cancelled\")"
			);
			session.reset_fallible().unwrap();
			assert_eq!(
				crate::format::render(&session.eval("42").unwrap(), None, &Default::default()),
				"42"
			);
			session.close().unwrap();
			server.join().unwrap();
		}
	}
	#[test]
	fn unpolled_native_calls_construct_no_client() {
		let life = crate::lifecycle::Lifecycle::new(true).unwrap();
		let state = super::State::default();
		let mut context = rune::Context::with_default_modules().unwrap();
		super::install(&mut context, &state, life.scope("http")).unwrap();
		let mut session = crate::session::Session::new(context)
			.unwrap()
			.with_http(state.clone())
			.with_lifecycle(life);
		for name in ["get", "get_bytes", "request", "request_bytes"] {
			let args = if name.starts_with("request") {
				"\"GET\", \"http://127.0.0.1:1/\", #{}"
			} else {
				"\"http://127.0.0.1:1/\""
			};
			session
				.eval(&format!("let q = http::{name}({args});"))
				.unwrap();
			assert!(state.0.lock().unwrap().client.is_none());
		}
		session
			.eval("let q = http::request(\"GET\", \"http://127.0.0.1:1/\", #{timeout_ms: 0});")
			.unwrap();
		let value = session.eval("q.await").unwrap();
		let text = crate::format::render(&value, None, &Default::default());
		assert!(
			text.contains("cannot get http://127.0.0.1:1/") && text.contains("deadline must"),
			"{text}"
		);
		assert!(state.0.lock().unwrap().client.is_none());
		session.close().unwrap();
	}
	#[test]
	fn lifecycle_failure_releases_the_cached_client_before_returning() {
		struct BadDrop;
		impl Drop for BadDrop {
			fn drop(&mut self) {
				panic!("injected destructor failure");
			}
		}
		let life = crate::lifecycle::Lifecycle::new(true).unwrap();
		let scope = life.scope("fixture");
		let state = super::State::default();
		state.0.lock().unwrap().client =
			Some(reqwest::Client::builder().no_proxy().build().unwrap());
		let mut context = rune::Context::with_default_modules().unwrap();
		crate::time::install(&mut context).unwrap();
		let mut module = rune::Module::with_crate("fixture").unwrap();
		module
			.function("bad", move || {
				scope.track(async {
					let _guard = BadDrop;
					std::future::pending::<Result<i64, String>>().await
				})
			})
			.build()
			.unwrap();
		context.install(module).unwrap();
		let mut session = crate::session::Session::new(context)
			.unwrap()
			.with_http(state.clone())
			.with_lifecycle(life);
		let error = session
			.eval(
				"let q = fixture::bad(); select { _ = q => (), _ = time::sleep(1) => () }; panic!(\"fail\")",
			)
			.unwrap_err();
		assert!(
			error.to_string().contains("lifecycle cleanup panicked"),
			"{error}"
		);
		assert!(session.lifecycle_failed());
		assert!(
			state.0.lock().unwrap().client.is_none(),
			"retirement releases pool before settlement/ack"
		);
	}
}
