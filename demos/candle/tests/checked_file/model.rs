use super::*;
use std::process::Stdio;

const EXEC: &str = "rnx: engine=execution ";
const LOAD: &str = r#"pub fn main(args){
	println!("MODEL-START");
	let m=candle::Mlp::load(args[0])?;
	let (i,h,o)=m.dims();
	println!("dims={},{},{}",i,h,o);
	Ok(())
}"#;

fn data_dir() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR")).join("../data")
}

fn tensors(i: usize, h: usize, o: usize) -> Vec<(&'static str, &'static str, Vec<usize>)> {
	vec![
		("fc1.weight", "F32", vec![h, i]),
		("fc1.bias", "F32", vec![h]),
		("fc2.weight", "F32", vec![o, h]),
		("fc2.bias", "F32", vec![o]),
	]
}

// A sparse zero payload is real F32 tensor data. Safetensors permits trailing
// spaces in its JSON header, so the boundary fixture has an exact byte count.
fn weights(path: &Path, entries: &[(&str, &str, Vec<usize>)], total: Option<usize>) {
	let mut offset = 0;
	let mut fields = Vec::new();
	for (name, dtype, shape) in entries {
		let start = offset;
		let width = if *dtype == "F32" { 4 } else { 1 };
		offset += shape.iter().product::<usize>() * width;
		fields.push(format!(
			r#""{name}":{{"dtype":"{dtype}","shape":{shape:?},"data_offsets":[{start},{offset}]}}"#
		));
	}
	let header = format!("{{{}}}", fields.join(","));
	let len = total.map_or(header.len().next_multiple_of(8), |size| size - offset - 8);
	assert!(len >= header.len());
	let mut file = File::create_new(path).unwrap();
	file.write_all(&(len as u64).to_le_bytes()).unwrap();
	write!(file, "{header:len$}").unwrap();
	file.set_len((8 + len + offset) as u64).unwrap();
}

fn loaded(script: &Path, path: &Path, dims: &str) {
	let (out, _) = check(cli(script, path).output().unwrap(), EXEC, 0);
	assert_eq!(out.lines().collect::<Vec<_>>(), ["MODEL-START", dims]);
}

fn rejected(script: &Path, path: &Path, reason: &str) {
	let (out, err) = check(cli(script, path).output().unwrap(), EXEC, 1);
	assert_eq!(out.lines().collect::<Vec<_>>(), ["MODEL-START"]);
	let message = format!("error: Mlp::load {:?}: {reason}", path.to_str().unwrap());
	assert!(err.lines().any(|s| s == message), "{err}");
}

#[test]
fn bundled_and_exact_limit_load_then_one_more_byte_fails() {
	let dir = Scratch::new();
	let script = dir.script(LOAD);
	loaded(&script, &data_dir().join("mlp.safetensors"), "dims=3,8,1");
	let path = dir.0.join("weights");
	let cap = 64 << 20;
	weights(&path, &tensors(4094, 4095, 1), Some(cap));
	loaded(&script, &path, "dims=4094,4095,1");
	fs::OpenOptions::new()
		.append(true)
		.open(&path)
		.unwrap()
		.write_all(b"x")
		.unwrap();
	rejected(&script, &path, "67108865 bytes, at most 67108864");
}

#[test]
fn file_failures_preserve_the_loader_diagnostics() {
	let dir = Scratch::new();
	let script = dir.script(LOAD);
	rejected(
		&script,
		&dir.0.join("missing"),
		"No such file or directory (os error 2)",
	);
	rejected(&script, &dir.0, "it is a directory");
	let path = dir.0.join("short");
	fs::write(&path, b"xyz").unwrap();
	rejected(&script, &path, "header too small");
	#[cfg(unix)]
	{
		let pipe = dir.0.join("fifo");
		assert!(
			Command::new("mkfifo")
				.arg(&pipe)
				.status()
				.unwrap()
				.success()
		);
		rejected(&script, &pipe, "it is not a regular file");
	}
	for (case, reason) in [
		(
			"keys",
			r#"tensors ["fc1.bias", "fc1.weight", "fc2.weight"], want exactly ["fc1.weight", "fc1.bias", "fc2.weight", "fc2.bias"]"#,
		),
		("dtype", "fc1.weight is U8, want F32"),
		("rank", "fc1.weight has rank 1, want 2"),
		("dimension", "in = 4097, want 1 to 4096"),
		(
			"mismatch",
			"mismatched dimensions: fc1.weight [2, 3], fc1.bias [3], fc2.weight [1, 2], fc2.bias [1]",
		),
	] {
		let mut entries = tensors(3, 2, 1);
		match case {
			"keys" => {
				entries.pop();
			}
			"dtype" => entries[0].1 = "U8",
			"rank" => entries[0].2 = vec![6],
			"dimension" => entries[0].2 = vec![2, 4097],
			_ => entries[1].2 = vec![3],
		}
		let path = dir.0.join(case);
		weights(&path, &entries, None);
		rejected(&script, &path, reason);
	}
}

#[test]
fn forward_runs_in_a_checked_file_and_tensor_export_falls_back() {
	let dir = Scratch::new();
	let demo = include_str!("../../model.rn").replacen(
		"let dir =",
		r#"println!("MODEL-START"); let dir ="#,
		1,
	);
	let (prefix, _) = demo.split_once("let y = model.forward(x)?;").unwrap();
	let checked = format!(
		r#"{prefix}
		let y=model.forward(x)?;
		println!("rank={{}} count={{}}",y.rank(),y.elem_count()); Ok(()) }}"#
	);
	let (out, _) = check(
		cli(&dir.script(&checked), &data_dir()).output().unwrap(),
		EXEC,
		0,
	);
	assert_eq!(
		out.lines().collect::<Vec<_>>(),
		["MODEL-START", "rank=2 count=64"]
	);
	for (body, route) in [
		(format!("{prefix} model.dims() }}"), EXEC),
		(
			format!("{prefix} let y=model.forward(x)?; let b=y.to_dense([\"score\"])?; Ok(()) }}"),
			"rnx: engine=legacy refusal=OpaqueReceiver",
		),
		(demo, "rnx: engine=legacy "),
	] {
		let (out, _) = check(
			cli(&dir.script(&body), &data_dir()).output().unwrap(),
			route,
			0,
		);
		assert_eq!(
			out.lines().filter(|line| *line == "MODEL-START").count(),
			1,
			"{out}"
		);
	}
}

#[test]
fn moved_model_and_explicit_format() {
	let dir = Scratch::new();
	let input = data_dir().join("mlp.safetensors");
	for (stmt, route) in [
		("drop(m);m.dims()", EXEC),
		(
			r#"println!("{}",m)"#,
			"rnx: engine=legacy refusal=OpaqueOperation",
		),
	] {
		let script = dir.script(&format!(
			r#"pub fn main(args){{println!("MODEL-START");let m=candle::Mlp::load(args[0])?;{stmt}}}"#
		));
		let (out, err) = check(cli(&script, &input).output().unwrap(), route, 1);
		assert_eq!(out.trim(), "MODEL-START");
		if route == EXEC {
			assert!(err.contains("it has been moved"), "{err}");
		}
	}
}

#[test]
fn pinned_session_refuses_before_effects_and_continues() {
	for allowance in [None, Some("8589934592")] {
		let mut cmd = Command::new(env!("CARGO_BIN_EXE_rnx-candle-demo"));
		cmd.args(["--no-splash", "--color=never", "repl", "--engine=execution"])
			.env("RNX_CONFIG", "")
			.env("RNX_HISTORY", "/dev/null")
			.env_remove("RNX_SESSION_STORE_BYTES");
		if let Some(n) = allowance {
			cmd.env("RNX_SESSION_STORE_BYTES", n);
		}
		let mut proc = cmd
			.stdin(Stdio::piped())
			.stdout(Stdio::piped())
			.stderr(Stdio::piped())
			.spawn()
			.unwrap();
		proc.stdin.take().unwrap().write_all(b"println!(\"MODEL-START\"); let m=candle::Mlp::load(\"unused\")?; m.dims()\nprintln!(\"next={}\",40+2)\n:quit\n").unwrap();
		let result = proc.wait_with_output().unwrap();
		let out = String::from_utf8(result.stdout).unwrap();
		let err = String::from_utf8(result.stderr).unwrap();
		assert!(
			result.status.success() && err.contains("candle::Mlp::load needs host enrollment"),
			"{out}\n{err}"
		);
		assert!(
			!out.contains("MODEL-START") && out.contains("next=42"),
			"{out}"
		);
	}
}

#[test]
fn wrong_and_moved_paths_fail_at_the_call() {
	let dir = Scratch::new();
	let path = data_dir().join("mlp.safetensors");
	for (expr, reason) in [
		(
			"let m=candle::Mlp::load(42)?; m.dims()",
			"Expected type `std::string::String` but found `std::i64`",
		),
		(
			"let p=args[0]; drop(p); let m=candle::Mlp::load(p)?; m.dims()",
			"it has been moved",
		),
	] {
		let script = dir.script(&format!(
			r#"pub fn main(args){{println!("MODEL-START");{expr}}}"#
		));
		let (out, err) = check(cli(&script, &path).output().unwrap(), EXEC, 1);
		assert_eq!(out.trim(), "MODEL-START");
		assert!(err.contains(reason), "{err}");
	}
}

#[test]
fn forward_reports_moved_inputs_in_order() {
	let dir = Scratch::new();
	let demo = include_str!("../../model.rn");
	let (prefix, _) = demo.split_once("let y = model.forward(x)?;").unwrap();
	for drop_inputs in ["drop(model);", "drop(x);", "drop(model);drop(x);"] {
		let src = format!("{prefix}{drop_inputs} let y=model.forward(x)?;y.rank()}}");
		let (_, err) = check(
			cli(&dir.script(&src), &data_dir()).output().unwrap(),
			EXEC,
			1,
		);
		assert!(err.contains("it has been moved"), "{err}");
	}
}

#[test]
fn unary_tensor_methods_each_select_the_checked_file() {
	let dir = Scratch::new();
	let prefix = include_str!("../../model.rn")
		.split_once("let model =")
		.unwrap()
		.0;
	for method in [
		"exp",
		"log",
		"sqrt",
		"sqr",
		"abs",
		"neg",
		"recip",
		"tanh",
		"relu",
		"gelu",
		"gelu_erf",
		"silu",
		"sin",
		"cos",
		"floor",
		"ceil",
		"round",
		"sign",
		"erf",
		"abs()?.sqrt()?.tanh",
	] {
		let src = format!(
			"{prefix}let y=x.{method}()?; println!(\"{{}} {{}}\", y.rank(), y.elem_count()); Ok(()) }}"
		);
		let (out, _) = check(
			cli(&dir.script(&src), &data_dir()).output().unwrap(),
			EXEC,
			0,
		);
		assert_eq!(out.trim(), "2 192", "{method}");
	}
}
