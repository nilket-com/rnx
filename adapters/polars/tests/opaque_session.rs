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
	let mut child = Command::new("/usr/bin/timeout")
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
			"println!(\"prefix-effect\");frame.height()\n",
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

	let bytes = write("bytes.csv", &vec![b'x'; 64 * 1024 + 1]);
	let rows = write(
		"rows.csv",
		format!("name,amount\n{}", "a,1\n".repeat(2_048)).as_bytes(),
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
	let bounded = capture(&root, "bounds", &["repl", "--engine=execution"], &input);
	let stderr = String::from_utf8(bounded.stderr).unwrap();
	assert_eq!(stderr.matches("StoreCapacity").count(), 3, "{stderr}");
	assert!(stderr.contains("arity"), "{stderr}");
	let stdout = transcript(&bounded.stdout);
	for name in ["bytes", "rows", "compressed", "prefix-effect"] {
		assert!(!stdout.contains(name), "{stdout}");
	}

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
