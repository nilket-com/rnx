//! Exercise the combined adapter executable across separate execution inputs.
use std::{
	io::Write,
	path::Path,
	process::{Command, Stdio},
};

#[test]
fn retained_frame_to_dense_to_tensor() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
	let mut child = Command::new(env!("CARGO_BIN_EXE_rnx-candle-demo"))
		.current_dir(root)
		.args(["--no-splash", "--color=never", "repl", "--engine=execution"])
		.env("RNX_CONFIG", "")
		.env("RNX_HISTORY", "/dev/null")
		.env_remove("RNX_MEMORY_CEILING")
		.env_remove("RNX_SESSION_STORE_BYTES")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.take().unwrap().write_all(br#"
let df=polars::read_csv("demos/data/features.csv",[("id","i64"),("a","f64"),("b","f64"),("c","f64")])?;
let d=df.to_dense(["a","c"],"f32")?;
let t=candle::Tensor::from_dense(d)?;
(t.rank(),t.elem_count(),df.width(),df.height()?)
t
let e=df.to_dense(["b"],"f64")?;
let u=candle::Tensor::from_dense(e)?;
(u.rank(),u.elem_count())
:reset
t.rank()
:quit
"#).unwrap();
	let out = child.wait_with_output().unwrap();
	let stdout = String::from_utf8(out.stdout).unwrap();
	let stderr = String::from_utf8(out.stderr).unwrap();
	assert!(out.status.success(), "{stderr}");
	for expected in [
		"(2, 128, 4, 64)",
		"Tensor[f32; 64x2]",
		"0.25 | 0.5",
		"(2, 64)",
	] {
		assert!(stdout.contains(expected), "{expected}: {stdout}\n{stderr}");
	}
	assert!(
		stderr.contains("t") && stderr.contains("t.rank()"),
		"{stderr}"
	);
	assert_eq!(stderr.matches("error").count(), 1, "{stderr}");
}
