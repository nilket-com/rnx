//! Record 0130: the test materialize and JSON limits are process-wide, and
//! 0080's bridge reads them on another thread, so they can't be per thread.
//! A test binary that lowers one therefore holds its own lock in every
//! `#[test]`; this audit keeps that true as tests are added.
#[test]
fn every_test_in_a_limit_lowering_binary_holds_its_lock() {
	let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
	let mut audited = 0;
	let mut entries: Vec<_> = std::fs::read_dir(&dir)
		.unwrap()
		.map(|e| e.unwrap().path())
		.collect();
	entries.sort();
	for path in entries {
		if path.extension().is_none_or(|e| e != "rs") || path.ends_with("limit_locks.rs") {
			continue;
		}
		let text = std::fs::read_to_string(&path).unwrap();
		if !(text.contains("set_materialize_limit(") || text.contains("set_json_limit(")) {
			continue;
		}
		audited += 1;
		// each test's first statement is the lock (a `use` or a comment runs nothing)
		let mut rest = text.as_str();
		while let Some(at) = rest.find("#[test]") {
			rest = &rest[at + "#[test]".len()..];
			let body = &rest[rest.find('{').unwrap() + 1..];
			let first = body
				.lines()
				.map(str::trim)
				.find(|l| !l.is_empty() && !l.starts_with("use ") && !l.starts_with("//"))
				.unwrap_or("");
			assert!(
				first.starts_with("let _") && first.contains(".lock()"),
				"{}: a test does not take the lock first: {first:?}",
				path.display()
			);
		}
	}
	assert_eq!(audited, 19, "the limit-lowering test binaries");
}
