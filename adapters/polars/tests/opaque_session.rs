//! Live adapter routes for the enrolled DataFrame bindings.
#![cfg(all(unix, feature = "generated"))]

#[path = "../../../tests/support/final_output.rs"]
mod final_output;
use std::{
	fs,
	io::Write,
	path::Path,
	process::{Command, Output, Stdio},
	time::{SystemTime, UNIX_EPOCH},
};

fn capture(dir: &Path, label: &str, args: &[&str], input: &str) -> Output {
	capture_with_allowance(dir, label, args, input, None)
}

fn capture_with_allowance(
	dir: &Path,
	label: &str,
	args: &[&str],
	input: &str,
	allowance: Option<&str>,
) -> Output {
	let mut command = Command::new("/usr/bin/timeout");
	command
		.args([
			"--kill-after=2s",
			"60s",
			env!("CARGO_BIN_EXE_rnx-polars"),
			"--no-splash",
			"--color=never",
		])
		.args(args)
		.env("RNX_CONFIG", "")
		.env("RNX_HISTORY", "/dev/null")
		.env_remove("RNX_MEMORY_CEILING")
		.env_remove("RNX_SESSION_STORE_BYTES");
	if let Some(allowance) = allowance {
		command
			.env("RNX_SESSION_STORE_BYTES", allowance)
			.env("POLARS_MAX_THREADS", "1");
	}
	let mut child = command
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child
		.stdin
		.take()
		.unwrap()
		.write_all(input.as_bytes())
		.unwrap();
	let output = child.wait_with_output().unwrap();
	fs::write(dir.join(format!("{label}.stdout")), &output.stdout).unwrap();
	fs::write(dir.join(format!("{label}.stderr")), &output.stderr).unwrap();
	fs::write(
		dir.join(format!("{label}.status")),
		format!("{:?}\n", output.status.code()),
	)
	.unwrap();
	assert!(output.status.success(), "{output:?}");
	output
}

fn transcript(output: &[u8]) -> String {
	let mut text = String::from_utf8(output.to_vec()).unwrap();
	for number in 1..=10 {
		text = text
			.replace(&format!("[{number}] > "), "")
			.replace(&format!("[{number}] "), "");
	}
	text
}

#[test]
fn dataframe_crosses_checked_inputs_and_file_route_refuses_before_effects() {
	let nonce = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let root = final_output::assigned("RNX_FINAL_ROUTING_ROOT", "adapter-polars-opaque")
		.unwrap_or_else(|| {
			Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
				"target/routing-cli/opaque-{}-{nonce}",
				std::process::id()
			))
		});
	fs::create_dir_all(&root).unwrap();
	let receipt = root.join("RESULT.json");
	fs::write(&receipt, b"{\"status\":\"FAIL\"}\n").unwrap();
	let input = concat!(
		"polars::DataFrame::empty().width()\n",
		"let frame=polars::DataFrame::empty();\n",
		"frame.width()\n",
		"frame\n",
		"format!(\"{frame}\")\n",
		":reset\n",
		"frame.width()\n",
		"let replacement=polars::DataFrame::empty();replacement.width()\n",
		":quit\n",
	);
	fs::write(root.join("session.rn"), input).unwrap();
	let legacy = capture(&root, "legacy", &["repl", "--engine=legacy"], input);
	let execution = capture(&root, "execution", &["repl", "--engine=execution"], input);
	let legacy_text = transcript(&legacy.stdout);
	let execution_text = transcript(&execution.stdout);
	let preview = "shape: (0, 0)\n┌┐\n╞╡\n└┘\n";
	assert_eq!(
		legacy_text,
		format!("0\n0\n<::polars::DataFrame>\n{preview:?}\nsession reset\n0\n")
	);
	assert_eq!(
		execution_text,
		legacy_text.replace("<::polars::DataFrame>\n", &format!("{preview}\n"))
	);
	assert_eq!(legacy.stderr, execution.stderr);
	assert!(String::from_utf8_lossy(&execution.stderr).contains("No local variable `frame`"));

	let rejected = capture(
		&root,
		"refused",
		&["repl", "--engine=execution"],
		concat!(
			"let frame=polars::DataFrame::empty();\n",
			"println!(\"prefix-effect\");frame.shape()\n",
			"frame.width()\n",
			":quit\n",
		),
	);
	assert_eq!(transcript(&rejected.stdout), "0\n");
	let stderr = String::from_utf8(rejected.stderr).unwrap();
	assert!(
		stderr.contains("refused: an unproven or unsupported host-value receiver")
			&& stderr.contains("(input 2, line 1, column 27)"),
		"{stderr}"
	);

	let file = root.join("file.rn");
	fs::write(
		&file,
		"pub fn main(args){println!(\"file-effect\");let frame=polars::DataFrame::empty();42}",
	)
	.unwrap();
	let file = file.to_str().unwrap();
	let output = capture(&root, "file", &["run", "--debug-engine", file], "");
	assert_eq!(output.stdout, b"file-effect\n42\n");
	let stderr = String::from_utf8(output.stderr).unwrap();
	let routes: Vec<_> = stderr
		.lines()
		.filter(|line| line.starts_with("rnx: engine="))
		.collect();
	assert_eq!(
		routes,
		["rnx: engine=legacy refusal=OpaqueEntry"],
		"{stderr}"
	);
	fs::write(receipt, b"{\"status\":\"PASS\"}\n").unwrap();
}

#[test]
fn csv_on_the_checked_session_retains_the_frame_and_preserves_string_errors() {
	let nonce = SystemTime::now()
		.duration_since(UNIX_EPOCH)
		.unwrap()
		.as_nanos();
	let root = final_output::assigned("RNX_FINAL_ROUTING_ROOT", "adapter-polars-csv")
		.unwrap_or_else(|| {
			Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
				"target/routing-cli/csv-{}-{nonce}",
				std::process::id()
			))
		});
	fs::create_dir_all(&root).unwrap();
	let receipt = root.join("RESULT.json");
	fs::write(&receipt, b"{\"status\":\"FAIL\"}\n").unwrap();
	let write = |name, contents: &[u8]| {
		let path = root.join(name);
		fs::write(&path, contents).unwrap();
		serde_json::to_string(path.to_str().unwrap()).unwrap()
	};
	let good = write("sales.csv", b"name,amount\nalpha,2\nbeta,3\n");
	let bad = write("bad.csv", b"name,amount\nalpha,nope\n");
	let missing = serde_json::to_string(root.join("missing.csv").to_str().unwrap()).unwrap();
	let schema = r#"[("name", "string"), ("amount", "i64")]"#;
	let input = format!(
		concat!(
			"let frame = polars::read_csv({good}, {schema})?;\n",
			"frame.width()\n",
			"fn probe(path, schema) {{ let _ = polars::read_csv(path, schema)?; Ok(()) }}\n",
			"probe({missing}, {schema})\n",
			"probe({good}, [(\"name\", \"invalid\")])\n",
			"probe({bad}, {schema})\n",
			":reset\nframe.width()\n:quit\n",
		),
		good = good,
		schema = schema,
		missing = missing,
		bad = bad
	);
	fs::write(root.join("session.rn"), &input).unwrap();
	let old = capture(&root, "legacy", &["repl", "--engine=legacy"], &input);
	let new = capture(&root, "execution", &["repl", "--engine=execution"], &input);
	assert_eq!(
		old.stderr,
		new.stderr,
		"{}",
		String::from_utf8_lossy(&new.stderr)
	);
	let output = transcript(&new.stdout);
	assert_eq!(transcript(&old.stdout), output);
	assert!(output.starts_with("2\nErr("), "{output}");
	assert!(output.contains("No such file or directory"), "{output}");
	assert!(
		output.contains("dtype must be string, i64, f64 or bool"),
		"{output}"
	);
	assert!(output.contains("could not parse `nope`"), "{output}");
	assert!(String::from_utf8_lossy(&new.stderr).contains("No local variable `frame`"));

	let bytes = write("bytes.csv", &vec![b'x'; 1024 * 1024 + 1]);
	let rows = write(
		"rows.csv",
		format!("name,amount\n{}", "a,1\n".repeat(100_000)).as_bytes(),
	);
	let compressed = write("compressed.csv", &[0x1f, 0x8b, 0, 0]);
	let input = format!(
		concat!(
			"let bytes = polars::read_csv({bytes}, {schema})?;\n",
			"let rows = polars::read_csv({rows}, {schema})?;\n",
			"let compressed = polars::read_csv({compressed}, {schema})?;\n",
			":vars\n",
			"println!(\"prefix-effect\"); polars::read_csv({good})?.width()\n",
			":quit\n",
		),
		bytes = bytes,
		rows = rows,
		compressed = compressed,
		schema = schema,
		good = good
	);
	let execution = ["repl", "--engine=execution"];
	let bounded = capture_with_allowance(&root, "bounds", &execution, &input, Some("1048576"));
	let stderr = String::from_utf8(bounded.stderr).unwrap();
	assert_eq!(stderr.matches("StoreCapacity").count(), 3, "{stderr}");
	assert!(stderr.contains("arity"), "{stderr}");
	let stdout = transcript(&bounded.stdout);
	for name in ["bytes", "rows", "compressed", "prefix-effect"] {
		assert!(!stdout.contains(name), "{stdout}");
	}

	// This file crosses both old limits. Each frame fits on its own. Retaining
	// the first takes enough Store credit away to refuse the second parse.
	let scaled = write(
		"scaled.csv",
		format!("name,amount\n{}", "abcdefghijkl,1\n".repeat(20_000)).as_bytes(),
	);
	let load = format!("let first = polars::read_csv({scaled}, {schema})?; first.width()\n");
	let old = capture(
		&root,
		"scaled-legacy",
		&["repl", "--engine=legacy"],
		&format!("{load}:quit\n"),
	);
	assert_eq!(transcript(&old.stdout), "2\n");
	let input = format!(
		"{load}let second = polars::read_csv({scaled}, {schema})?; second.width()\n:reset\n{load}:quit\n"
	);
	let scaled = capture_with_allowance(
		&root,
		"scaled-execution",
		&execution,
		&input,
		Some("2200000"),
	);
	assert_eq!(transcript(&scaled.stdout), "2\nsession reset\n2\n");
	let stderr = String::from_utf8(scaled.stderr).unwrap();
	assert_eq!(stderr.matches("StoreCapacity").count(), 1, "{stderr}");

	let file = root.join("file.rn");
	fs::write(
		&file,
		format!(
			"pub fn main(args) {{ println!(\"file-effect\"); polars::read_csv({good}, {schema})?.width() }}"
		),
	)
	.unwrap();
	let file = capture(
		&root,
		"csv-file",
		&["run", "--debug-engine", file.to_str().unwrap()],
		"",
	);
	assert_eq!(file.stdout, b"file-effect\n2\n");
	assert!(
		String::from_utf8_lossy(&file.stderr).contains("rnx: engine=legacy refusal=OpaqueEntry")
	);
	fs::write(receipt, b"{\"status\":\"PASS\"}\n").unwrap();
}

#[test]
fn retained_csv_reports_height_after_load_and_reset() {
	let root = tempfile_for_height("retained");
	let csv = root.join("rows.csv");
	fs::write(&csv, "n\n10\n20\n30\n").unwrap();
	let input = format!(
		"let frame = polars::read_csv({}, [(\"n\",\"i64\")])?;\nframe.height()?\n:reset\npolars::DataFrame::empty().height()?\n:quit\n",
		serde_json::to_string(csv.to_str().unwrap()).unwrap()
	);
	let result = capture(&root, "retained", &["repl", "--engine=execution"], &input);
	assert!(
		result.stderr.is_empty(),
		"{}",
		String::from_utf8_lossy(&result.stderr)
	);
	assert_eq!(transcript(&result.stdout), "3\nsession reset\n0\n");
	let path = root.join("file.rn");
	fs::write(
		&path,
		"pub fn main(args) { println!(\"file-effect\"); polars::DataFrame::empty().height()? }",
	)
	.unwrap();
	let file = capture(
		&root,
		"file",
		&["run", "--debug-engine", path.to_str().unwrap()],
		"",
	);
	assert_eq!(file.stdout, b"file-effect\n0\n");
	assert!(
		String::from_utf8_lossy(&file.stderr).contains("rnx: engine=legacy refusal=OpaqueEntry")
	);
}

fn tempfile_for_height(label: &str) -> std::path::PathBuf {
	let root = final_output::assigned(
		"RNX_FINAL_ROUTING_ROOT",
		&format!("adapter-polars-height-{label}"),
	)
	.unwrap_or_else(|| {
		Path::new(env!("CARGO_MANIFEST_DIR")).join(format!(
			"target/routing-cli/height-{label}-{}-{}",
			std::process::id(),
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		))
	});
	fs::create_dir_all(&root).unwrap();
	root
}

#[cfg(all(feature = "test-support", target_pointer_width = "64"))]
#[test]
fn height_exposes_a_typed_error_and_keeps_the_frame_after_propagation() {
	let root = tempfile_for_height("conversion");
	let execution = ["repl", "--engine=execution"];
	let input = concat!(
		"let f=polars::height_overflow_fixture();\n",
		"match f.height() { Ok(x)=>x,Err(e)=>e }\n",
		"match f.height() { Err(e)=>e.kind(),Ok(x)=>format!(\"{x}\") }\n",
		"match f.height() { Ok(x)=>format!(\"{x}\"),Err(e)=>format!(\"{e}\") }\n",
		"f.height()?\nf.width()\n:quit\n"
	);
	let result = capture(&root, "conversion", &execution, input);
	let message = "height: 9223372036854775808 does not fit a script integer";
	assert_eq!(
		transcript(&result.stdout),
		format!("{message}\n\"ConversionError\"\n\"{message}\"\n0\n")
	);
	assert_eq!(
		String::from_utf8(result.stderr).unwrap(),
		concat!(
			"runtime error (position not in your input; :debug shows the generated source): ",
			"Error { kind: Type, location: None, operation: \"store\", refusal: None, detail: None }\n"
		)
	);
	let helper = capture(
		&root,
		"helper",
		&execution,
		concat!(
			"fn caught() { let f=polars::height_overflow_fixture(); match f.height() { Ok(x)=>format!(\"{x}\"),Err(e)=>e.kind() } }\n",
			"caught()\n",
			"fn denied() { println!(\"effect\"); let f=polars::height_overflow_fixture(); let x=f.height()?; Ok(x) }\n",
			"denied()\n:quit\n"
		),
	);
	assert_eq!(transcript(&helper.stdout), "\"ConversionError\"\n");
	let stderr = String::from_utf8(helper.stderr).unwrap();
	assert!(
		stderr.contains("refused: this operation on a host value") && stderr.contains("(input 3,"),
		"{stderr}"
	);
	assert!(stderr.contains("Missing item denied"), "{stderr}");
	let escape = capture(
		&root,
		"escapes",
		&execution,
		concat!(
			"let f=polars::height_overflow_fixture();\n",
			"println!(\"effect\"); [f.height()]\n",
			"println!(\"effect\"); { let wrapped=f.height(); || wrapped }\n",
			"println!(\"effect\"); std::mem::drop(f.height())\n",
			"println!(\"effect\"); match f.height() { Ok(x)=>x,_=>0 }\n",
			":quit\n"
		),
	);
	assert_eq!(transcript(&escape.stdout), "");
	let stderr = String::from_utf8(escape.stderr).unwrap();
	assert_eq!(stderr.matches("refused:").count(), 4, "{stderr}");
}
