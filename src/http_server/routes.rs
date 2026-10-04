//! Startup-only Rune table decoding and bounded matching over owned Rust data.
use super::{log::Log, options::Options, worker::Catch};
use crate::{
	Extensions,
	rune::{self, Value},
	server::Program,
};
use std::collections::{BTreeMap, BTreeSet};
const MAX_ROUTES: usize = 256;
const MAX_TEXT: usize = 64 << 10;
const HOOKS: [&str; 3] = ["not_found", "method_not_allowed", "bad_request"];
#[derive(Debug)]
enum Segment {
	Literal(String),
	Param(String),
}
#[derive(Debug)]
struct Entry {
	method: String,
	pattern: String,
	handler: String,
	segments: Vec<Segment>,
}
#[derive(Debug)]
pub(super) struct Routes {
	entries: Vec<Entry>,
	hooks: [Option<String>; 3],
}
pub(super) struct Selection {
	pub handler: Option<String>,
	pub params: BTreeMap<String, String>,
	pub status: Option<u16>,
	pub allow: Option<String>,
}
fn identifier(s: &str) -> bool {
	let mut parser = rune::parse::Parser::new(s, rune::SourceId::empty(), false);
	let Ok(token) = parser.parse::<rune::ast::Token>() else {
		return false;
	};
	matches!(token.kind, rune::ast::Kind::Ident(..))
		&& token.span.range() == (0..s.len())
		&& matches!(parser.is_eof(), Ok(true))
}
fn handler_name(s: &str) -> bool {
	!s.is_empty() && s.split("::").all(identifier)
}
fn percent(s: &str) -> Result<String, String> {
	let b = s.as_bytes();
	let mut out = Vec::with_capacity(b.len());
	let mut i = 0;
	while i < b.len() {
		if b[i] == b'%' {
			let hi = b.get(i + 1).and_then(|b| (*b as char).to_digit(16));
			let lo = b.get(i + 2).and_then(|b| (*b as char).to_digit(16));
			let (Some(hi), Some(lo)) = (hi, lo) else {
				return Err("parameter has a malformed percent escape".into());
			};
			out.push((hi * 16 + lo) as u8);
			i += 3;
		} else {
			out.push(b[i]);
			i += 1;
		}
	}
	String::from_utf8(out).map_err(|_| "parameter is not UTF-8".into())
}
fn pattern(s: &str) -> Result<Vec<Segment>, String> {
	if !s.starts_with('/')
		|| !s.is_ascii()
		|| s.bytes()
			.any(|b| b <= 32 || b == 127 || matches!(b, b'?' | b'#' | b'\\'))
	{
		return Err("pattern wants an absolute ASCII URI path without query, fragment, whitespace or backslash".into());
	}
	let parts = s[1..].split('/');
	if parts.clone().count() > 64 {
		return Err("pattern exceeds 64 segments".into());
	}
	let mut names = BTreeSet::new();
	let mut out = Vec::new();
	for p in parts {
		if p.starts_with('{') && p.ends_with('}') {
			let name = &p[1..p.len() - 1];
			if name.len() > 64 || !identifier(name) || !names.insert(name) {
				return Err(
					"parameter names want distinct Rune identifiers of at most 64 bytes".into(),
				);
			}
			if names.len() > 16 {
				return Err("pattern exceeds 16 parameters".into());
			}
			out.push(Segment::Param(name.to_owned()));
		} else {
			if p.contains(['{', '}', '*']) {
				return Err("pattern has partial braces or a wildcard".into());
			}
			if !p
				.bytes()
				.all(|b| b.is_ascii_alphanumeric() || b"-._~!$&'()+,;=:@%".contains(&b))
			{
				return Err("literal has an invalid URI path character".into());
			}
			// Validate escapes without requiring that literal escaped bytes form UTF-8.
			let b = p.as_bytes();
			let mut i = 0;
			while i < b.len() {
				if b[i] == b'%' {
					if i + 2 >= b.len()
						|| !b[i + 1].is_ascii_hexdigit()
						|| !b[i + 2].is_ascii_hexdigit()
					{
						return Err("literal has a malformed percent escape".into());
					}
					i += 3;
				} else {
					i += 1;
				}
			}
			out.push(Segment::Literal(p.to_owned()));
		}
	}
	Ok(out)
}
fn same_shape(a: &[Segment], b: &[Segment]) -> bool {
	a.len() == b.len()
		&& a.iter().zip(b).all(|(a, b)| match (a, b) {
			(Segment::Literal(a), Segment::Literal(b)) => a == b,
			(Segment::Param(_), Segment::Param(_)) => true,
			_ => false,
		})
}
fn more_specific(a: &[Segment], b: &[Segment]) -> bool {
	for (a, b) in a.iter().zip(b) {
		match (a, b) {
			(Segment::Literal(_), Segment::Param(_)) => return true,
			(Segment::Param(_), Segment::Literal(_)) => return false,
			_ => {}
		}
	}
	false
}
fn borrowed_string(v: &Value, max: usize) -> Result<rune::runtime::BorrowRef<'_, str>, String> {
	let s = v
		.borrow_string_ref()
		.map_err(|_| "route slots want strings".to_owned())?;
	if s.len() > max {
		return Err(format!("route string has {} bytes, limit {max}", s.len()));
	}
	Ok(s)
}
impl Routes {
	fn decode(value: Value, program: &Program) -> Result<Self, String> {
		let rows = value
			.borrow_ref::<rune::runtime::Vec>()
			.map_err(|_| "routes() wants a vector".to_owned())?;
		if rows.len() > MAX_ROUTES {
			return Err(format!(
				"routes() has {} rows, limit {MAX_ROUTES}",
				rows.len()
			));
		}
		let mut entries: Vec<Entry> = Vec::with_capacity(rows.len());
		let mut text = 0usize;
		for (i, row) in rows.iter().enumerate() {
			let decoded = (|| {
				let row = row
					.borrow_tuple_ref()
					.map_err(|_| "route row wants a tuple".to_owned())?;
				if row.len() != 3 {
					return Err("route row wants exactly method, pattern, handler".into());
				}
				let method = borrowed_string(&row[0], 32)?;
				let path = borrowed_string(&row[1], 1024)?;
				let handler = borrowed_string(&row[2], 256)?;
				if method.is_empty()
					|| !method.bytes().all(|b| {
						b.is_ascii_uppercase()
							|| b.is_ascii_digit() || b"!#$%&'*+-.^_`|~".contains(&b)
					}) {
					return Err("method wants an uppercase HTTP token".into());
				}
				if !handler_name(&handler) {
					return Err("handler wants a Rune item path".into());
				}
				if HOOKS.contains(&&*handler) {
					return Err("error-hook names are reserved as route handlers".into());
				}
				let new_text = text
					.checked_add(method.len() + path.len() + handler.len())
					.ok_or("route text overflows")?;
				// Parameter name storage duplicates at most all pattern bytes.
				let new_text = new_text
					.checked_add(path.len())
					.ok_or("route text overflows")?;
				if new_text > MAX_TEXT {
					return Err("route text exceeds 64 KiB".into());
				}
				let segments = pattern(&path)?;
				for old in &entries {
					if same_shape(&old.segments, &segments) {
						if old.pattern != *path {
							return Err("same-shape parameter renamings conflict".into());
						}
						if old.method == *method {
							return Err("duplicate method/pattern".into());
						}
					}
				}
				if program.http_function(&handler).map_err(|e| e.to_string())? != Some(1) {
					return Err(format!(
						"handler {} wants a compiled one-argument Rune function",
						&*handler
					));
				}
				text = new_text;
				Ok(Entry {
					method: method.to_string(),
					pattern: path.to_string(),
					handler: handler.to_string(),
					segments,
				})
			})();
			entries.push(decoded.map_err(|e: String| format!("route row {}: {e}", i + 1))?);
		}
		let mut hooks = [None, None, None];
		for (i, name) in HOOKS.iter().enumerate() {
			match program.http_function(name).map_err(|e| e.to_string())? {
				None => {}
				Some(1) => hooks[i] = Some((*name).into()),
				Some(_) => return Err(format!("hook {name} wants one argument")),
			}
		}
		Ok(Self { entries, hooks })
	}
	pub async fn load(
		program: &Program,
		options: &Options,
		log: &Log,
	) -> Result<Option<Self>, String> {
		Self::load_with_extensions(program, options, log, Extensions::none()).await
	}
	pub(super) async fn load_with_extensions(
		program: &Program,
		options: &Options,
		log: &Log,
		extensions: Extensions,
	) -> Result<Option<Self>, String> {
		let located = |message: String| {
			let e = program.http_route_error(message);
			match (e.path(), e.position()) {
				(Some(p), Some((l, c))) => format!("{}:{l}:{c}: {}", p.display(), e.message()),
				_ => e.to_string(),
			}
		};
		match program
			.http_function("routes")
			.map_err(|e| located(e.to_string()))?
		{
			None => return Ok(None),
			Some(0) => {}
			Some(_) => return Err(located("routes wants no arguments".into())),
		}
		let future = async {
			let mut call = program
				.prepare_with(extensions, "routes", vec![], options.budget)
				.map_err(|e| e.to_string())?;
			let result = tokio::time::timeout(options.timeout, call.run())
				.await
				.map_err(|_| "routes startup timed out".to_owned())
				.and_then(|r| r.map_err(|e| e.to_string()))
				.and_then(|v| Self::decode(v, program));
			match call.close() {
				Err(e) if e.category() == "cleanup" || result.is_ok() => Err(e.to_string()),
				_ => result,
			}
		};
		let result = Catch(Box::pin(future))
			.await
			.map_err(|_| located("routes startup panicked; invocation disposed".into()))?
			.map_err(located)?;
		if program.http_function("main").is_ok_and(|f| f.is_some()) {
			log.event("routes_mode", "main is unused");
		}
		Ok(Some(result))
	}
	pub fn select(&self, method: &str, path: &str) -> Selection {
		if path.split('/').count() > 65 {
			return self.error(0, 404, None);
		}
		let parts: Vec<_> = path.strip_prefix('/').unwrap_or(path).split('/').collect();
		let best = self
			.entries
			.iter()
			.filter(|e| {
				e.segments.len() == parts.len()
					&& e.segments.iter().zip(&parts).all(|(s, p)| match s {
						Segment::Literal(l) => l == p,
						Segment::Param(_) => !p.is_empty(),
					})
			})
			.reduce(|best, e| {
				if more_specific(&e.segments, &best.segments) {
					e
				} else {
					best
				}
			});
		let Some(best) = best else {
			return self.error(0, 404, None);
		};
		let mut params = BTreeMap::new();
		for (s, p) in best.segments.iter().zip(parts) {
			if let Segment::Param(name) = s {
				match percent(p) {
					Ok(p) => {
						params.insert(name.clone(), p);
					}
					Err(_) => return self.error(2, 400, None),
				}
			}
		}
		let group: Vec<_> = self
			.entries
			.iter()
			.filter(|e| e.pattern == best.pattern)
			.collect();
		let selected = group
			.iter()
			.find(|e| e.method == method)
			.copied()
			.or_else(|| {
				if method == "HEAD" {
					group.iter().find(|e| e.method == "GET").copied()
				} else {
					None
				}
			});
		if let Some(e) = selected {
			return Selection {
				handler: Some(e.handler.clone()),
				params,
				status: None,
				allow: None,
			};
		}
		let mut methods: BTreeSet<&str> = group.iter().map(|e| e.method.as_str()).collect();
		if methods.contains("GET") {
			methods.insert("HEAD");
		}
		let mut ordered = vec![];
		for m in [
			"GET", "HEAD", "POST", "PUT", "PATCH", "DELETE", "OPTIONS", "CONNECT", "TRACE",
		] {
			if methods.remove(m) {
				ordered.push(m);
			}
		}
		ordered.extend(methods);
		self.error(1, 405, Some(ordered.join(", ")))
	}
	fn error(&self, i: usize, status: u16, allow: Option<String>) -> Selection {
		Selection {
			handler: self.hooks[i].clone(),
			params: BTreeMap::new(),
			status: Some(status),
			allow,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	fn source(rows: &str, more: &str) -> Program {
		Program::compile_source(
			"<routes-control>",
			&format!("pub fn routes() {{ {rows} }}\npub fn handler(request) {{ request }}\n{more}"),
			Extensions::none(),
		)
		.unwrap()
	}
	fn load(program: &Program, timeout_ms: u64, budget: usize) -> Result<Option<Routes>, String> {
		let mut options = Options::parse(&["control.rn".into()]).unwrap();
		options.timeout = std::time::Duration::from_millis(timeout_ms);
		options.budget = budget;
		let logger = super::super::log::Logger::new(std::io::sink(), false).unwrap();
		let result = tokio::runtime::Builder::new_current_thread()
			.enable_all()
			.build()
			.unwrap()
			.block_on(Routes::load(program, &options, &logger.log));
		logger.shutdown();
		result
	}
	#[test]
	fn compiled_metadata_proves_arity_and_rejects_constructors_without_execution() {
		let p = source(
			"[]",
			"pub fn zero() { panic!(\"must not run\") }\npub fn two(a,b) {}\npub struct Constructor(i64);\nmod nested {pub async fn one(a) {a}}\npub fn keep() { nested::one(0) }",
		);
		assert_eq!(p.http_function("routes").unwrap(), Some(0));
		assert_eq!(p.http_function("handler").unwrap(), Some(1));
		assert_eq!(p.http_function("zero").unwrap(), Some(0));
		assert_eq!(p.http_function("two").unwrap(), Some(2));
		assert_eq!(p.http_function("nested::one").unwrap(), Some(1));
		assert_eq!(p.http_function("missing").unwrap(), None);
		assert_eq!(p.http_function("fs::read").unwrap(), None);
		assert!(p.http_function("Constructor").is_err());
	}
	#[test]
	fn legacy_is_absent_and_startup_errors_are_source_attributed() {
		let p =
			Program::compile_source("<legacy>", "pub fn main(r) {r}", Extensions::none()).unwrap();
		assert!(load(&p, 100, 10000).unwrap().is_none());
		let p = source("[(\"GET\",\"/\",\"absent\")]", "");
		let e = load(&p, 100, 10000).unwrap_err();
		assert!(
			e.contains("<routes-control>:") && e.contains("route row 1") && e.contains("absent"),
			"{e}"
		);
	}
	#[test]
	fn invalid_tables_patterns_names_and_arities_fail_at_startup() {
		for (rows, more, want) in [
			("#{routes:[]}", "", "vector"),
			("[[\"GET\",\"/\",\"handler\"]]", "", "tuple"),
			("[(\"GET\",\"/\")]", "", "exactly"),
			("[(1,\"/\",\"handler\")]", "", "strings"),
			("[(\"get\",\"/\",\"handler\")]", "", "uppercase"),
			("[(\"GET\",\"relative\",\"handler\")]", "", "absolute"),
			("[(\"GET\",\"/?q\",\"handler\")]", "", "absolute"),
			("[(\"GET\",\"/{x}/{x}\",\"handler\")]", "", "distinct"),
			("[(\"GET\",\"/{while}\",\"handler\")]", "", "identifiers"),
			("[(\"GET\",\"/{x}tail\",\"handler\")]", "", "partial"),
			("[(\"GET\",\"/*\",\"handler\")]", "", "wildcard"),
			("[(\"GET\",\"/%ZZ\",\"handler\")]", "", "escape"),
			("[(\"GET\",\"/\",\"handler::\")]", "", "item path"),
			(
				"[(\"GET\",\"/\",\"not_found\")]",
				"pub fn not_found(r) {r}",
				"reserved",
			),
			(
				"[(\"GET\",\"/\",\"zero\")]",
				"pub fn zero() {}",
				"one-argument",
			),
			(
				"[(\"GET\",\"/\",\"Constructor\")]",
				"pub struct Constructor(i64);",
				"compiled",
			),
			("[]", "pub fn bad_request() {}", "one argument"),
			(
				"[(\"GET\",\"/\",\"handler\"),(\"GET\",\"/\",\"handler\")]",
				"",
				"duplicate",
			),
			(
				"[(\"GET\",\"/{x}\",\"handler\"),(\"POST\",\"/{y}\",\"handler\")]",
				"",
				"renamings",
			),
		] {
			let e = load(&source(rows, more), 100, 100000).unwrap_err();
			assert!(e.contains(want), "{rows}: {e}");
		}
	}
	#[test]
	fn parameter_decoding_and_raw_literal_rules() {
		let r=load(&source("[(\"GET\",\"/hello/{name}\",\"handler\"),(\"GET\",\"/literal/%2f\",\"handler\"),(\"GET\",\"/a//\",\"handler\")]",""),100,100000).unwrap().unwrap();
		for (path, want) in [
			("/hello/J%C3%BCrgen", "Jürgen"),
			("/hello/a+b%20c", "a+b c"),
			("/hello/%2F", "/"),
			("/hello/%252F", "%2F"),
		] {
			let s = r.select("GET", path);
			assert!(s.status.is_none());
			assert_eq!(s.params["name"], want);
		}
		for path in [
			"/hello/",
			"/hello/a/b",
			"/Hello/a",
			"/hello/a/",
			"/literal/%2F",
			"/a/",
		] {
			assert_eq!(r.select("GET", path).status, Some(404), "{path}");
		}
		for path in ["/hello/%FF", "/hello/%", "/hello/%Q0"] {
			assert_eq!(r.select("GET", path).status, Some(400));
			assert_eq!(r.select("DELETE", path).status, Some(400));
		}
		assert!(r.select("GET", "/literal/%2f").status.is_none());
		assert!(r.select("GET", "/a//").status.is_none());
	}
	#[test]
	fn specificity_is_full_path_first_then_method_independent_of_order() {
		let rows = [
			("GET", "/posts/{id}", "handler"),
			("POST", "/posts/new", "other"),
			("GET", "/a/{x}", "handler"),
			("GET", "/{y}/b", "other"),
			("GET", "/a/literal/extra", "other"),
		];
		for reverse in [false, true] {
			let p = source("[]", "pub fn other(r) {r}");
			let mut rows = rows.to_vec();
			if reverse {
				rows.reverse();
			}
			let r = Routes::decode(rune::to_value(rows).unwrap(), &p).unwrap();
			assert_eq!(r.select("GET", "/posts/new").status, Some(405));
			assert_eq!(
				r.select("POST", "/posts/new").handler.as_deref(),
				Some("other")
			);
			assert_eq!(r.select("GET", "/a/b").handler.as_deref(), Some("handler"));
			assert_eq!(
				r.select("GET", "/a/literal").handler.as_deref(),
				Some("handler")
			);
			assert_eq!(
				r.select("GET", "/a/literal/extra").handler.as_deref(),
				Some("other")
			);
		}
	}
	#[test]
	fn explicit_head_implicit_head_and_allow_hooks_are_frozen() {
		let p = source(
			"[]",
			"pub fn other(r) {r}\npub fn not_found(r) {r}\npub fn method_not_allowed(r) {r}\npub fn bad_request(r) {r}",
		);
		let r = Routes::decode(
			rune::to_value(vec![
				("POST", "/", "handler"),
				("GET", "/", "handler"),
				("HEAD", "/", "other"),
				("HEAD", "/head", "handler"),
				("GET", "/get", "handler"),
				("ZED", "/", "handler"),
			])
			.unwrap(),
			&p,
		)
		.unwrap();
		assert_eq!(r.select("HEAD", "/").handler.as_deref(), Some("other"));
		assert_eq!(r.select("HEAD", "/get").handler.as_deref(), Some("handler"));
		assert_eq!(r.select("GET", "/head").allow.as_deref(), Some("HEAD"));
		let s = r.select("DELETE", "/");
		assert_eq!(s.handler.as_deref(), Some("method_not_allowed"));
		assert_eq!(s.status, Some(405));
		assert_eq!(s.allow.as_deref(), Some("GET, HEAD, POST, ZED"));
		assert!(s.params.is_empty());
		assert_eq!(
			r.select("GET", "/missing").handler.as_deref(),
			Some("not_found")
		);
	}
	#[test]
	fn startup_budget_timeout_and_panic_are_failures_not_partial_tables() {
		for (text, want) in [
			("pub fn routes() {loop {}}", "budget"),
			(
				"pub async fn routes() {time::sleep(100).await?; []}",
				"timed out",
			),
			("pub fn routes() {panic!(\"fixture\");}", "fixture"),
		] {
			let p = Program::compile_source("<startup>", text, Extensions::none()).unwrap();
			let e = load(&p, 5, 1000).unwrap_err();
			assert!(e.contains(want), "{e}");
		}
	}
	#[test]
	fn borrowed_decode_keeps_the_value_and_checks_all_table_bounds() {
		let p = source("[]", "");
		let v = rune::to_value(vec![("GET", "/", "handler")]).unwrap();
		assert!(Routes::decode(v.clone(), &p).is_ok());
		assert_eq!(v.borrow_ref::<rune::runtime::Vec>().unwrap().len(), 1);
		type Row = (String, String, String);
		let cases: [(Vec<Row>, &str); 6] = [
			(
				vec![("GET".into(), "/".into(), "handler".into()); 257],
				"rows",
			),
			(
				vec![(
					"GET".into(),
					format!("/{}", "a".repeat(1024)),
					"handler".into(),
				)],
				"bytes",
			),
			(
				vec![(
					"GET".into(),
					format!("/{}", ["x"; 65].join("/")),
					"handler".into(),
				)],
				"segments",
			),
			(
				vec![(
					"GET".into(),
					format!(
						"/{}",
						(0..17)
							.map(|i| format!("{{p{i}}}"))
							.collect::<Vec<_>>()
							.join("/")
					),
					"handler".into(),
				)],
				"parameters",
			),
			(
				vec![(
					"GET".into(),
					format!("/{{{}}}", "x".repeat(65)),
					"handler".into(),
				)],
				"64 bytes",
			),
			(
				(0..100)
					.map(|i| {
						(
							"GET".into(),
							format!("/{i}/{}", "a".repeat(900)),
							"handler".into(),
						)
					})
					.collect(),
				"64 KiB",
			),
		];
		for (rows, want) in cases {
			let v = rune::to_value(rows).unwrap();
			let len = v.borrow_ref::<rune::runtime::Vec>().unwrap().len();
			let e = Routes::decode(v.clone(), &p).unwrap_err();
			assert!(e.contains(want), "{e}");
			assert_eq!(v.borrow_ref::<rune::runtime::Vec>().unwrap().len(), len);
		}
	}
}

#[cfg(all(test, feature = "allocation-peak"))]
mod allocation_controls {
	use super::*;
	#[test]
	#[ignore = "run alone: process-wide peak accounting"]
	fn decoder_refuses_huge_preexisting_values_before_copying() {
		let p = Program::compile_source(
			"<allocation>",
			"pub fn routes() {[]} pub fn handler(r) {r}",
			Extensions::none(),
		)
		.unwrap();
		let row = rune::to_value(("GET", "/", "handler")).unwrap();
		let mut outer = rune::runtime::Vec::new();
		for _ in 0..131072 {
			outer.push(row.clone()).unwrap();
		}
		let large = rune::to_value(outer).unwrap();
		let long_handler = rune::to_value(vec![(
			"GET".to_owned(),
			"/".to_owned(),
			"x".repeat(10 << 20),
		)])
		.unwrap();
		let long_pattern = rune::to_value(vec![(
			"GET".to_owned(),
			format!("/{}", "x".repeat(10 << 20)),
			"handler".to_owned(),
		)])
		.unwrap();
		for (name, value) in [
			("outer rows", large),
			("handler bytes", long_handler),
			("pattern bytes", long_pattern),
		] {
			let base = crate::memory::live().unwrap();
			crate::memory::reset_peak();
			let e = Routes::decode(value.clone(), &p).unwrap_err();
			let peak = crate::memory::peak().saturating_sub(base);
			println!("{name}: {peak} bytes, {e}");
			assert!(peak < 65536, "{name}: {peak}");
		}
	}
}

#[cfg(test)]
mod measurements {
	use super::*;
	#[test]
	#[ignore = "isolated matcher timing; not an end-to-end performance gate"]
	fn bounded_matcher_cost() {
		let p = Program::compile_source(
			"<matcher>",
			"pub fn routes() {[]} pub fn handler(r) {r}",
			Extensions::none(),
		)
		.unwrap();
		for n in [1, 256] {
			let rows = (0..n)
				.map(|i| {
					(
						"GET".to_owned(),
						format!("/r{i}/{{name}}"),
						"handler".to_owned(),
					)
				})
				.collect::<Vec<_>>();
			let r = Routes::decode(rune::to_value(rows).unwrap(), &p).unwrap();
			let path = format!("/r{}/a%2Fb", n - 1);
			let mut times = vec![];
			for _ in 0..5 {
				let start = std::time::Instant::now();
				for _ in 0..100000 {
					std::hint::black_box(
						r.select(std::hint::black_box("GET"), std::hint::black_box(&path)),
					);
				}
				times.push(start.elapsed().as_nanos() as f64 / 100000.0);
			}
			times.sort_by(f64::total_cmp);
			println!(
				"{n} routes, decoded parameter, 100000 matches x5: median {:.1} ns, range {:.1}..{:.1} ns",
				times[2], times[0], times[4]
			);
		}
	}
}
