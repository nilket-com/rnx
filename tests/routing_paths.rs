//! Live file/module/source-map routing through the actual rnx executable.
#![cfg(unix)]
use std::{
	fs,
	path::{Path, PathBuf},
	process::{Command, Output, Stdio},
	time::{SystemTime, UNIX_EPOCH},
};
fn scratch(name: &str) -> PathBuf {
	let nonce = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
		"target/routing-paths/{name}-{}-{nonce}",
		std::process::id()
	));
	fs::create_dir_all(&root).unwrap();
	fs::write(root.join("RESULT.json"), b"{\"status\":\"FAIL\"}\n").unwrap();
	root
}
fn run(
	root: &Path,
	extra: &[&Path],
	legacy: bool,
	native: bool,
	debug: &[&str],
	label: &str,
) -> Output {
	let mut c = Command::new("/usr/bin/timeout");
	c.args([
		"--kill-after=2s",
		"60s",
		env!("CARGO_BIN_EXE_rnx"),
		"--color=never",
		"run",
	])
	.arg(if native {
		"--native=on"
	} else {
		"--native=off"
	});
	if legacy {
		c.arg("--engine=legacy");
	}
	c.args(debug);
	if !extra.is_empty() {
		c.arg("--source-map").arg(extra[0]);
	}
	c.arg(root.join("main.rn"))
		.args(["sentinel", "--engine=legacy"])
		.stdin(Stdio::null())
		.env("RNX_CONFIG", "")
		.current_dir(root);
	let output = c.output().unwrap();
	fs::write(root.join(format!("{label}.stdout")), &output.stdout).unwrap();
	fs::write(root.join(format!("{label}.stderr")), &output.stderr).unwrap();
	fs::write(
		root.join(format!("{label}.status")),
		format!("{:?}\n", output.status.code()),
	)
	.unwrap();
	assert!(
		!matches!(output.status.code(), None | Some(124 | 137)),
		"deadline/signal: {}",
		root.display()
	);
	output
}
fn verify(root: &Path, extra: &[&Path], route: &str) {
	verify_outcome(root, extra, route, true);
}

fn verify_outcome(root: &Path, extra: &[&Path], route: &str, success: bool) {
	let mut routes = Vec::new();
	for native in [false, true] {
		let mode = if native { "on" } else { "off" };
		let old = run(root, extra, true, native, &[], &format!("{mode}-legacy"));
		let new = run(root, extra, false, native, &[], &format!("{mode}-auto"));
		assert_eq!(
			old.status.success(),
			success,
			"{}",
			String::from_utf8_lossy(&old.stderr)
		);
		assert_eq!(
			(old.status.code(), old.stdout, old.stderr),
			(new.status.code(), new.stdout, new.stderr)
		);
		let evidence = run(
			root,
			extra,
			false,
			native,
			&["--debug-engine"],
			&format!("{mode}-route"),
		);
		assert_eq!(evidence.status.success(), success);
		let text = String::from_utf8(evidence.stderr).unwrap();
		let selected: Vec<_> = text
			.lines()
			.filter(|line| line.starts_with("rnx: engine="))
			.collect();
		assert_eq!(selected.len(), 1, "{text}");
		assert!(selected[0].starts_with(route), "{text}");
		routes.push(
			serde_json::json!({"native": mode, "route": selected[0], "exit": evidence.status.code()}),
		);
	}
	fs::write(
		root.join("RESULT.json"),
		serde_json::json!({"status":"PASS","native_modes":["off","on"],"auto_routes":routes})
			.to_string(),
	)
	.unwrap();
	println!(
		"PASS routing path; receipt {}",
		root.join("RESULT.json").display()
	);
}
/// The decoy that layout precedence must not compile when `helper/mod.rn`
/// exists. Checked by its whole text: the debug output also contains the
/// scratch path, whose process id and timestamp can contain any digits.
const DECOY: &str = "pub fn value(){999}";

#[test]
fn nested_module_files_use_execution_and_keep_layout_precedence() {
	// "999" in the directory name keeps the path-collision case exercised.
	let root = scratch("modules-999");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");[helper::value(),args]}",
	)
	.unwrap();
	fs::create_dir(root.join("helper")).unwrap();
	fs::write(root.join("helper.rn"), DECOY).unwrap();
	fs::write(
		root.join("helper/mod.rn"),
		"mod nested;pub fn value(){nested::value()}",
	)
	.unwrap();
	fs::write(root.join("helper/nested.rn"), "pub fn value(){42}").unwrap();
	verify(&root, &[], "rnx: engine=execution ");
	let debug = run(&root, &[], false, false, &["--debug-source"], "source");
	let text = String::from_utf8(debug.stderr).unwrap();
	for file in ["main.rn", "helper/mod.rn", "helper/nested.rn"] {
		let mark = format!("--- compiled source of {}", root.join(file).display());
		assert_eq!(text.matches(&mark).count(), 1, "{text}");
	}
	assert!(text.contains("999"), "the scratch path should contain 999");
	assert!(!text.contains(DECOY), "{text}");
}

#[test]
fn the_layout_check_sees_the_decoy_when_it_is_compiled() {
	// The control for the check above: with no `helper/` directory the
	// decoy is the module, so its text must appear in the debug output.
	let root = scratch("modules-999-flat");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");[helper::value(),args]}",
	)
	.unwrap();
	fs::write(root.join("helper.rn"), DECOY).unwrap();
	let debug = run(&root, &[], false, false, &["--debug-source"], "source");
	let text = String::from_utf8(debug.stderr).unwrap();
	assert_eq!(text.matches(DECOY).count(), 1, "{text}");
}

#[test]
fn refused_module_falls_back_before_side_effects_and_shows_each_source_once() {
	let root = scratch("module-refusal");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");helper::value()}",
	)
	.unwrap();
	fs::write(root.join("helper.rn"), "pub fn value(){let x=b\"abc\";42}").unwrap();
	verify(&root, &[], "rnx: engine=legacy refusal=ByteStr");
	let debug = run(&root, &[], false, false, &["--debug-source"], "source");
	assert_eq!(debug.stdout, b"once\n42\n");
	let text = String::from_utf8(debug.stderr).unwrap();
	for file in ["main.rn", "helper.rn"] {
		let mark = format!("--- compiled source of {}", root.join(file).display());
		assert_eq!(text.matches(&mark).count(), 1, "{text}");
	}
}

#[test]
fn missing_read_and_allowance_errors_keep_the_declaring_module_location() {
	for error in ["missing", "read", "allowance"] {
		let root = scratch(error);
		fs::write(
			root.join("main.rn"),
			"mod helper;pub fn main(args){println!(\"unexpected\");helper::value()}",
		)
		.unwrap();
		fs::write(
			root.join("helper.rn"),
			"mod child;pub fn value(){child::value()}",
		)
		.unwrap();
		fs::create_dir(root.join("helper")).unwrap();
		match error {
			"read" => fs::write(root.join("helper/child.rn"), [0xff]).unwrap(),
			"allowance" => {
				fs::write(root.join("helper/child.rn"), vec![b' '; 8 * 1024 * 1024]).unwrap()
			}
			_ => {}
		}
		verify_outcome(
			&root,
			&[],
			"rnx: engine=legacy refusal=FrontendDiagnostic",
			false,
		);
		let result = run(&root, &[], false, false, &[], "diagnostic");
		assert!(result.stdout.is_empty());
		let text = String::from_utf8(result.stderr).unwrap();
		assert!(
			text.contains(&format!(
				"{}, line 1, column 1:",
				root.join("helper.rn").display()
			)),
			"{text}"
		);
		assert!(text.contains("mod child;"), "{text}");
	}
}

#[test]
fn a_runtime_failure_in_a_module_is_located_and_terminal() {
	let root = scratch("module-runtime");
	fs::write(
		root.join("main.rn"),
		"mod helper;pub fn main(args){println!(\"once\");helper::value()}",
	)
	.unwrap();
	fs::write(
		root.join("helper.rn"),
		"pub fn value(){\n  panic!(\"module failure\")\n}",
	)
	.unwrap();
	verify_outcome(&root, &[], "rnx: engine=execution ", false);
	let result = run(&root, &[], false, false, &[], "diagnostic");
	assert_eq!(result.stdout, b"once\n");
	let text = String::from_utf8(result.stderr).unwrap();
	assert!(
		text.contains(&format!(
			"{}, line 2, column 3:",
			root.join("helper.rn").display()
		)),
		"{text}"
	);
}

#[test]
#[cfg(feature = "project-sources")]
fn explicit_source_map_preserves_admission_and_argument_boundaries() {
	let root = scratch("source-map");
	fs::write(
		root.join("main.rn"),
		"pub fn main(args){let a=[];a.push(a);[a,args]}",
	)
	.unwrap();
	let map = root.join("map.json");
	fs::write(
		&map,
		serde_json::to_vec(
			&serde_json::json!({"format":1,"entry":root.join("main.rn"),"mounts":[]}),
		)
		.unwrap(),
	)
	.unwrap();
	verify(&root, &[&map], "rnx: engine=execution native-requested=");
}

#[test]
#[cfg(feature = "project-sources")]
fn explicit_source_map_loads_an_external_module_and_keeps_entry_admission() {
	let root = scratch("source-map");
	fs::write(
		root.join("main.rn"),
		"mod pkg;pub fn main(args){[pkg::value(),args]}",
	)
	.unwrap();
	let package = scratch("mapped-module");
	fs::write(package.join("mod.rn"), "pub fn value(){42}").unwrap();
	let map = root.join("map.json");
	fs::write(
		&map,
		serde_json::to_vec(
			&serde_json::json!({"format":1,"entry":root.join("main.rn"),"mounts":[{"prefix":["pkg"],"root":package}]}),
		)
		.unwrap(),
	)
	.unwrap();
	verify(&root, &[&map], "rnx: engine=execution native-requested=");

	fs::write(
		&map,
		serde_json::json!({"format":1,"entry":root.join("other.rn"),"mounts":[]}).to_string(),
	)
	.unwrap();
	let legacy = run(&root, &[&map], true, false, &[], "bad-map-legacy");
	let auto = run(
		&root,
		&[&map],
		false,
		false,
		&["--debug-engine"],
		"bad-map-auto",
	);
	assert_eq!(
		(legacy.status.code(), &legacy.stdout, &legacy.stderr),
		(auto.status.code(), &auto.stdout, &auto.stderr)
	);
	assert_eq!(auto.status.code(), Some(1));
	let text = String::from_utf8(auto.stderr).unwrap();
	assert!(
		text.contains("source map entry does not match the absolute run entry"),
		"{text}"
	);
}

#[test]
fn default_native_request_matches_on_and_off_stays_explicit_even_under_perf_environment() {
	let root = scratch("native-default");
	let input = root.join("main.rn");
	fs::write(
		&input,
		"pub fn main(args){println!(\"once\");let n=0;while n<10000{n+=1;}n}",
	)
	.unwrap();
	for environment in [false, true] {
		for (label, option, requested) in [
			("default", None, "on"),
			("on", Some("--native=on"), "on"),
			("off", Some("--native=off"), "off"),
		] {
			let mut command = Command::new("/usr/bin/timeout");
			command.args([
				"--kill-after=2s",
				"60s",
				env!("CARGO_BIN_EXE_rnx"),
				"--color=never",
				"run",
				"--debug-engine",
			]);
			if let Some(option) = option {
				command.arg(option);
			}
			command
				.arg(&input)
				.current_dir(&root)
				.stdin(Stdio::null())
				.env("RNX_CONFIG", "");
			if environment {
				command.env("PERF_BUILDID_DIR", root.join("perf"));
			} else {
				command.env_remove("PERF_BUILDID_DIR");
			}
			let out = command.output().unwrap();
			let label = format!("{label}-perf-env-{environment}");
			fs::write(root.join(format!("{label}.stdout")), &out.stdout).unwrap();
			fs::write(root.join(format!("{label}.stderr")), &out.stderr).unwrap();
			fs::write(
				root.join(format!("{label}.status")),
				format!("{:?}\n", out.status.code()),
			)
			.unwrap();
			assert_eq!(
				out.status.code(),
				Some(0),
				"{label}: {}",
				String::from_utf8_lossy(&out.stderr)
			);
			assert_eq!(out.stdout, b"once\n10000\n", "{label}");
			// This is a request report, including when the environment disables loading.
			assert_eq!(
				out.stderr,
				format!("rnx: engine=execution native-requested={requested}\n").as_bytes(),
				"{label}"
			);
		}
	}
	fs::write(root.join("RESULT.json"), b"{\"status\":\"PASS\"}\n").unwrap();
}

#[test]
fn match_receiver_runs_each_effect_once_on_its_selected_route() {
	for (route, method, caller) in [
		("execution", "m", "x.m()"),
		("legacy refusal=Method", "capacity", "through(x)"),
		("legacy refusal=Method", "capacity", "make().capacity()"),
	] {
		let root = scratch("match-receiver");
		for (v, hit) in [(1, true), (2, false)] {
			let pick = format!("match {v} {{1=>S{{}}}}");
			let (helper, init) = match caller {
				"through(x)" => ("fn through(x){x.capacity()}".to_owned(), pick),
				"make().capacity()" => (format!("fn make(){{{pick}}}"), "0".to_owned()),
				_ => (String::new(), pick),
			};
			let src = format!(
				"struct S{{}} impl S{{fn {method}(self){{println!(\"body\");7}}}}\n{helper}\npub fn main(args){{println!(\"entry\");let x={init};dbg!({caller});}}"
			);
			fs::write(root.join("main.rn"), &src).unwrap();
			for native in [false, true] {
				let output = run(&root, &[], false, native, &["--debug-engine"], "match");
				let stderr = String::from_utf8(output.stderr).unwrap();
				let selected = stderr.lines().next().unwrap();
				assert!(
					selected.starts_with(&format!("rnx: engine={route}")),
					"{stderr}"
				);
				assert_eq!(
					output.status.code(),
					Some(if hit { 0 } else { 1 }),
					"{stderr}"
				);
				assert_eq!(
					output.stdout,
					if hit {
						b"entry\nbody\n7\n".as_slice()
					} else {
						b"entry\n".as_slice()
					}
				);
				if !hit {
					let called = if caller == "through(x)" {
						"x.capacity()"
					} else {
						caller
					};
					let (line, text) = src
						.lines()
						.enumerate()
						.find(|(_, line)| line.contains(called))
						.unwrap();
					let column = text.find(called).unwrap() + 1;
					assert!(
						stderr.contains(&format!(
							"line {}, column {column}: no method `{method}` on `std::tuple::Tuple`",
							line + 1
						)),
						"{stderr}"
					);
				}
			}
		}
		fs::remove_dir_all(root).unwrap();
	}
}

#[test]
fn range_patterns_match_through_the_file_entry() {
	let root = scratch("range-patterns");
	let source = r#"
		use std::ops::{Range, RangeInclusive, RangeFull, RangeFrom, RangeTo, RangeToInclusive};
		struct Wrapper { value }
		fn bounds(Range { start, end }) { start + end }
		fn endpoint(n) { println!("{}", n); n }
		fn produced() { 3..8 }
		pub fn main(args) {
			assert_eq!(match 2..5 { _ => 7 }, 7);
			assert_eq!(match 2..5 { r => r.start }, 2);
			let first = |Range {start,..}| start;
			assert_eq!(first(2..5), 2);
			let sum=0;
			for Range {start,..} in [2..5, 3..7] { sum+=start; }
			assert_eq!(sum, 5);
			assert_eq!(match endpoint(2)..endpoint(5) {
				Range { start: 3, .. } => 0,
				std::ops::Range { start: a, end: b } if a < b => a+b, _ => 0
			}, 7);
			assert_eq!(match 2.. { RangeFrom {start} => start, _ => 0 }, 2);
			assert_eq!(match 2.. { RangeFrom {..} => 1, _ => 0 }, 1);
			assert_eq!(match .. { RangeFull => 1, _ => 0 }, 1);
			assert_eq!(match .. { RangeFull {..} => 1, _ => 0 }, 1);
			assert_eq!(match 2..=5 { RangeInclusive {start,end} => start+end, _ => 0 }, 7);
			assert_eq!(match 2..=5 { RangeInclusive {end,..} => end, _ => 0 }, 5);
			assert_eq!(match ..5 { RangeTo {end} => end, _ => 0 }, 5);
			assert_eq!(match ..5 { RangeTo {..} => 1, _ => 0 }, 1);
			assert_eq!(match ..=5 { RangeToInclusive {end} => end, _ => 0 }, 5);
			assert_eq!(match ..=5 { RangeToInclusive {..} => 1, _ => 0 }, 1);
			assert_eq!(match 2.. { Range {..} => 0, _ => 1 }, 1);
			assert_eq!(match .. { RangeFrom {..} => 0, _ => 1 }, 1);
			assert_eq!(match 2..=5 { RangeFull => 0, _ => 1 }, 1);
			assert_eq!(match ..5 { RangeInclusive {..} => 0, _ => 1 }, 1);
			assert_eq!(match ..=5 { RangeTo {..} => 0, _ => 1 }, 1);
			assert_eq!(match 2..5 { RangeToInclusive {..} => 0, _ => 1 }, 1);
			for x in [(), 5, [1,2], #{start:1,end:2}] {
				assert_eq!(match x { Range {start,..} => start, _ => 9 }, 9);
			}
			let p = Wrapper { value: Some([(("start", 7)..5,)]) };
			assert_eq!(match p.value {
				Some([(Range { start: ("start", n), .. },)]) => n, _ => 0
			}, 7);
			let Range {start, end} = produced();
			assert_eq!(start+end, 11);
			assert_eq!(bounds(produced()), 11);
			let r=produced(); let hits=0;
			if let Range {..} = r { hits+=1; }
			if let RangeFull = r { hits+=10; }
			while let RangeFull = r { hits+=100; break; }
			while let Range {..} = r { hits+=1; break; }
			assert_eq!(hits, 2);
		}
	"#;
	fs::write(root.join("main.rn"), source).unwrap();
	for native in [false, true] {
		let output = run(&root, &[], false, native, &["--debug-engine"], "ranges");
		let stderr = String::from_utf8(output.stderr).unwrap();
		assert!(output.status.success(), "{stderr}");
		assert!(stderr.starts_with("rnx: engine=execution "), "{stderr}");
		assert_eq!(output.stdout, b"2\n5\n");
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn range_patterns_keep_misses_and_diagnostics_at_the_source() {
	let root = scratch("range-pattern-errors");
	let cases = [
		(
			"pub fn main(args){let f=|std::ops::Range{start,..}|start;f(1..)}",
			"std::ops::Range",
			"Panicked: pattern did not match",
			"execution",
		),
		(
			"pub fn main(args){for std::ops::Range{start,..} in [2..] {println!(\"wrong\");}}",
			"std::ops::Range",
			"Panicked: pattern did not match",
			"execution",
		),
		(
			"pub fn main(args){let std::ops::Range{start,..}=1..=2;start}",
			"std::ops::Range",
			"Panicked: pattern did not match",
			"execution",
		),
		(
			"fn f(std::ops::Range{start,..}){start} pub fn main(args){f(1..)}",
			"std::ops::Range",
			"Panicked: pattern did not match",
			"execution",
		),
		(
			"struct S{} impl S{fn m(self){println!(\"wrong\");7}} pub fn main(args){let x=match .. {std::ops::Range{..}=>S{}};x.m()}",
			"x.m()",
			"no method `m` on `std::tuple::Tuple`",
			"execution",
		),
		(
			"pub fn main(args){match 1..2 {std::ops::Range{begin,..}=>begin}}",
			"std::ops::Range",
			"Field `begin` is not a field in `::std::ops::Range`",
			"legacy refusal=FrontendDiagnostic",
		),
		(
			"pub fn main(args){match 1..2 {std::ops::Range{start}=>start}}",
			"std::ops::Range",
			"Non-exhaustive pattern for `::std::ops::Range`",
			"legacy refusal=FrontendDiagnostic",
		),
		(
			"pub fn main(args){match 1..2 {std::ops::Range(a,b)=>a}}",
			"std::ops::Range",
			"Expected type that can be used in a tuple pattern",
			"legacy refusal=FrontendDiagnostic",
		),
		(
			"pub fn main(args){match .. {std::ops::RangeFull{start,..}=>start}}",
			"std::ops::RangeFull",
			"Expected type that can be used in a struct pattern",
			"legacy refusal=FrontendDiagnostic",
		),
	];
	for (source, at, diagnostic, route) in cases {
		fs::write(root.join("main.rn"), source).unwrap();
		let column = source.find(at).unwrap() + 1;
		for native in [false, true] {
			let output = run(&root, &[], false, native, &["--debug-engine"], "error");
			let stderr = String::from_utf8(output.stderr).unwrap();
			assert_eq!(output.status.code(), Some(1), "{stderr}");
			assert!(output.stdout.is_empty());
			assert!(
				stderr.starts_with(&format!("rnx: engine={route}")),
				"{stderr}"
			);
			assert!(
				stderr.contains(&format!("line 1, column {column}: {diagnostic}")),
				"{stderr}"
			);
		}
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn script_method_results_select_the_actual_returning_body() {
	let root = scratch("method-results");
	let defs = r#"
		struct A; struct B; struct X; struct Y;
		impl A {
			fn new(){ A }
			fn make(self){ println!("make A"); X }
			fn iter(self){ println!("iter"); self.make() }
			fn produce(self){ X }
			fn maybe(self,n){ match n {1=>X} }
			fn recurse(self,n){ if n == 0 { X } else { self.recurse(n-1) } }
			fn left(self,n){ if n == 0 { X } else { self.right(n-1) } }
			fn right(self,n){ self.left(n) }
		}
		impl B { fn make(self){ println!("make B"); Y } fn produce(self){ A.produce() } }
		impl X { fn next(self){ println!("next X"); 3 } fn len(self){ println!("len"); 17 } fn push(self){ println!("push"); 19 } }
		impl Y { fn next(self){ println!("next Y"); 9 } }
	"#;
	for (body, route, success) in [
		(
			"dbg!((A.make().next(), B.make().next()));",
			"execution",
			true,
		),
		("dbg!(B.produce().next());", "execution", true),
		("dbg!(A::new().iter().push());", "execution", true),
		("dbg!(A.maybe(1).next());", "legacy refusal=Method", true),
		(
			"let receiver = if args.len() == 0 { A } else { B }; dbg!(receiver.make().next());",
			"legacy refusal=Method",
			true,
		),
		("A.make(1).next();", "legacy refusal=Method", false),
		("dbg!(A.recurse(2).len());", "legacy refusal=Method", true),
		("dbg!(A.left(2).len());", "legacy refusal=Method", true),
		("dbg!(A.maybe(2).next());", "legacy refusal=Method", false),
		("dbg!(A.maybe(2).len());", "legacy refusal=Method", true),
	] {
		fs::write(
			root.join("main.rn"),
			format!("{defs} pub fn main(args){{{body}}}"),
		)
		.unwrap();
		verify_outcome(&root, &[], &format!("rnx: engine={route}"), success);
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn returning_constant_match_guard_preserves_running_programs() {
	let root = scratch("constant-match-guard");
	for (input, wildcard, expected) in [
		(0, "9", Some("7")),
		(1, "9", Some("9")),
		(0, "{1=2;9}", Some("7")),
		(1, "{1=2;9}", None),
	] {
		let source = format!(
			"const fn f(x){{match x {{0 if {{return 7;}}=>5,_=>{wildcard}}}}} const V=f({input});pub fn main(args){{println!(\"{{}}\",V);}}"
		);
		fs::write(root.join("main.rn"), source).unwrap();
		for native in [false, true] {
			let reference = run(&root, &[], true, native, &["--debug-engine"], "stock");
			assert!(reference.status.success(), "{reference:?}");
			let out = run(&root, &[], false, native, &["--debug-engine"], "auto");
			assert!(out.status.success(), "{out:?}");
			let route = String::from_utf8(out.stderr).unwrap();
			if let Some(value) = expected {
				assert!(route.starts_with("rnx: engine=execution"), "{route}");
				assert_eq!(out.stdout, format!("{value}\n").as_bytes());
			} else {
				assert!(
					route.starts_with("rnx: engine=legacy refusal=FrontendDiagnostic"),
					"{route}"
				);
				assert_eq!(out.stdout, reference.stdout);
			}
		}
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn const_calls_in_file_bodies_use_fresh_values_and_preserve_diagnostics() {
	let root = scratch("const-call");
	for (source, route, success) in [
		(
			r#"const fn list(n) { [n] }
			const N = 2;
			fn make() { list(N + 1) }
			pub fn main(args) { let a = make(); a.push(9); dbg!((a, make(), list(3))); }"#,
			"execution",
			true,
		),
		(
			r#"const fn add(a,b) {a+b} pub fn main(args) {
				let local=42;
				println!("{}",add({return 2; local},{return 3;}));
			}"#,
			"execution",
			true,
		),
		(
			r#"const fn add(a,b){a+b} pub fn main(args) {
				let local=42;
				println!("{}",add({return 2;},if false {local} else {3}));
			}"#,
			"legacy refusal=ConstCall",
			false,
		),
		(
			r#"const fn bad(){if false {1=2;}7} pub fn main(args){println!("{}",bad());}"#,
			"legacy refusal=ConstCall",
			false,
		),
		(
			r#"const fn noisy() { std::io::println("compiling"); 7 }
			pub fn main(args) { if false { noisy(); } println!("running"); }"#,
			"legacy refusal=ConstCall",
			true,
		),
		(
			r#"const fn id(x){x} pub fn main(args){let n=true; id(false && n)}"#,
			"legacy refusal=ConstCall",
			false,
		),
		(
			r#"const fn bad(n){n+1} pub fn main(args){if false {bad(9223372036854775807);} }"#,
			"legacy refusal=ConstCall",
			false,
		),
	] {
		fs::write(root.join("main.rn"), source).unwrap();
		verify_outcome(&root, &[], &format!("rnx: engine={route}"), success);
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn constant_format_templates_preserve_errors_and_compile_time_output() {
	let root = scratch("const-template");
	for (source, route, success) in [
		(
			r#"const T = "{{{v:04}}}"; pub fn main(args) {
			print!(const { "{"+"}" }, 1);
			println!(T, v=2);
			assert!(true,T,v=3); assert_eq!(1,1,T,v=4); assert_ne!(1,2,T,v=5);
			if false {panic!(T,v=6);} dbg!(format!(T,v=7));
		}"#,
			"execution",
			true,
		),
		(
			r#"const T = "{}"; pub fn main(args) { println!(T); }"#,
			"legacy refusal=FrontendDiagnostic",
			false,
		),
		(
			r#"pub fn main(args){println!(const {std::io::println("compiling"); "{}"}, 7)}"#,
			"legacy refusal=FrontendDiagnostic",
			true,
		),
	] {
		fs::write(root.join("main.rn"), source).unwrap();
		verify_outcome(&root, &[], &format!("rnx: engine={route}"), success);
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn returning_constant_guards_select_the_matched_body_at_each_root() {
	let root = scratch("constant-guard-roots");
	let defs = r#"const fn text(x) {
		match x {0 if {return "early";} => {std::io::println("guard-effect"); "left"}, _ => "right"}
	}"#;
	for (items, body) in [
		("", r#"println!("{}", text(1));"#),
		("", r#"println!(text(1));"#),
		("const T=text(1);", r#"println!(T);"#),
		(
			"const T=text(1); const fn id(x){x}",
			r#"id(3); println!("{}", T);"#,
		),
	] {
		fs::write(
			root.join("main.rn"),
			format!("{defs}{items}pub fn main(args){{{body}}}"),
		)
		.unwrap();
		for native in [false, true] {
			let out = run(&root, &[], false, native, &["--debug-engine"], "guard");
			assert!(out.status.success(), "{out:?}");
			assert!(out.stderr.starts_with(b"rnx: engine=execution"), "{out:?}");
			assert_eq!(out.stdout, b"right\n");
		}
	}
	fs::remove_dir_all(root).unwrap();
}

#[test]
fn string_builder_aliases_copies_and_access_errors_in_files() {
	let root = scratch("string-builder");
	let shared = r#"
		fn append(s) { s.push_str("x"); }
		fn identity(s) { s }
		pub fn main(args) {
			let s = String::from("a"); let alias = s;
			alias.push_str("x"); let field = #{ v: s }; field.v.push_str("x");
			let v = [s]; v[0].push_str("x"); let f = || s.push_str("x"); f();
			append(s); identity(s).push_str("x"); println!("{}", s);
			let m = #{}; m[s] = 7;
			let a = s.clone(); let b = String::from(s); let c = s + "";
			let d = format!("{}",s); let e = `${s}`;
			for t in [a,b,c,d,e] { t.push_str("z"); println!("{}", t); }
			s.clear(); println!("{} {}", s.len(), m["axxxxxx"]);
		}"#;
	fs::write(root.join("main.rn"), shared).unwrap();
	verify_outcome(&root, &[], "rnx: engine=execution", true);
	for body in [
		r#"let s="ab";let it=s.chars();s.push_str(s);it.next()"#,
		r#"let s="ab";let it=s.chars();s.clear();it.next()"#,
		r#"let s="ab";let it=s.chars();s.reserve_exact(8);it.next()"#,
		r#""a".push("x")"#,
		r#""a".push_str('x')"#,
		r#""a".push_str(3)"#,
		r#"String::with_capacity(-1)"#,
		r#"String::with_capacity(1.5)"#,
		r#"fail(3)"#,
		r#"fail([])"#,
		r#"fail(())"#,
	] {
		let source = format!("fn fail(x){{x.push_str(\"z\")}} pub fn main(args){{{body}}}");
		fs::write(root.join("main.rn"), source).unwrap();
		verify_outcome(&root, &[], "rnx: engine=execution", false);
	}
	fs::remove_dir_all(root).unwrap();
}
