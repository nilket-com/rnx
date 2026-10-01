//! Record 0129: a frame's numeric columns to and from rnx's neutral block.
//! Each conversion policy is tested at its boundary, as a policy; every
//! refusal names the column and row; the frame, the block and the column
//! list stay usable after every call.
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut context = Context::with_default_modules().unwrap();
	context
		.install(rnx::interchange::module().unwrap())
		.unwrap();
	context.install(polars).unwrap();
	let runtime = Arc::new(context.runtime().unwrap());
	let mut sources = Sources::new();
	sources.insert(Source::memory(script).unwrap()).unwrap();
	let mut diagnostics = rune::Diagnostics::new();
	let built = rune::prepare(&mut sources)
		.with_context(&context)
		.with_diagnostics(&mut diagnostics)
		.build();
	if built.is_err() {
		panic!("diagnostics: {:?}", diagnostics.diagnostics());
	}
	let mut vm = Vm::new(runtime, Arc::new(built.unwrap()));
	vm.call(["main"], ()).map_err(|e| e.to_string())
}

/// The script's `Ok(String)`, or its `Err` text.
fn outcome(script: &str) -> Result<String, String> {
	let v = run(script)?;
	match rune::from_value::<Result<String, rune::Value>>(v).map_err(|e| e.to_string())? {
		Ok(s) => Ok(s),
		Err(e) => Err(rune::from_value::<String>(e).unwrap_or_else(|_| "non-string error".into())),
	}
}

/// A unique CSV per call, so parallel tests never share a file.
fn csv(body: &str) -> String {
	static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
	let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
	let d = std::env::temp_dir().join(format!("rnx-0129-dense-{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	let p = d.join(format!("f-{n}.csv"));
	std::fs::write(&p, body).unwrap();
	p.to_str().unwrap().to_owned()
}

/// `to_dense` of one column read with `schema`, as `dtype`.
fn one(value: &str, schema: &str, dtype: &str) -> Result<String, String> {
	let path = csv(&format!("a\n{value}\n"));
	outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"a\", {schema:?})])?; \
		   let b = df.to_dense([\"a\"], {dtype:?})?; \
		   let back = polars::DataFrame::from_dense(b)?; Ok(`${{back}}`) }}"
	))
}

#[test]
fn integers_are_admitted_in_the_contiguous_exact_range_only() {
	// f64: |v| <= 2^53, inspected as an integer before any cast
	assert!(
		one("9007199254740992", "i64", "f64")
			.unwrap()
			.contains("9007199254740992.0")
	);
	let e = one("9007199254740993", "i64", "f64").unwrap_err();
	assert!(
		e.contains("outside the exact integer range") && e.contains("row 0") && e.contains("\"a\""),
		"{e}"
	);
	assert!(one("-9007199254740992", "i64", "f64").is_ok());
	assert!(one("-9007199254740993", "i64", "f64").is_err());
	// f32: |v| <= 2^24; 2^24 + 2 is representable and still refused (policy)
	assert!(
		one("16777216", "i64", "f32")
			.unwrap()
			.contains("16777216.0")
	);
	assert!(one("16777217", "i64", "f32").is_err());
	assert!(
		one("16777218", "i64", "f32")
			.unwrap_err()
			.contains("outside the exact integer range")
	);
}

#[test]
fn floats_follow_the_finite_range_policy() {
	// f32::MAX admitted; the next f64 above it refused, though it would round to f32::MAX
	assert!(one("3.4028234663852886e38", "f64", "f32").is_ok());
	let e = one("3.402823466385289e38", "f64", "f32").unwrap_err();
	assert!(e.contains("outside the f32 range"), "{e}");
	assert!(one("0.1", "f64", "f32").unwrap().contains("0.1"));
	for bad in ["NaN", "inf", "-inf"] {
		let e = one(bad, "f64", "f64").unwrap_err();
		assert!(
			e.contains("non-finite value in column \"a\" at row 0"),
			"{bad}: {e}"
		);
	}
}

#[test]
fn nulls_and_non_numeric_columns_are_refused_by_name() {
	let path = csv("a,s,b\n1,x,true\n,y,false\n");
	let e = outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"a\", \"i64\"), (\"s\", \"string\"), (\"b\", \"bool\")])?; \
		   let r = df.to_dense([\"a\"], \"f64\"); Ok(`${{r.is_err()}}`) }}"
	));
	assert_eq!(e.unwrap(), "true");
	let e = outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"a\", \"i64\"), (\"s\", \"string\"), (\"b\", \"bool\")])?; df.to_dense([\"a\"], \"f64\")?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(e.contains("a null in column \"a\" at row 1"), "{e}");
	for (col, want) in [("s", "is str, not numeric"), ("b", "is bool, not numeric")] {
		let e = outcome(&format!(
			"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"a\", \"i64\"), (\"s\", \"string\"), (\"b\", \"bool\")])?; df.to_dense([{col:?}], \"f64\")?; Ok(``) }}"
		))
		.unwrap_err();
		assert!(e.contains(want), "{col}: {e}");
	}
}

#[test]
fn the_column_list_and_dtype_are_checked_first() {
	let path = csv("a,b\n1,2\n");
	let try_ = |cols: &str, dtype: &str| {
		outcome(&format!(
			"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"a\", \"i64\"), (\"b\", \"i64\")])?; df.to_dense({cols}, {dtype:?})?; Ok(``) }}"
		))
		.unwrap_err()
	};
	assert!(try_("[]", "f64").contains("at least one column"));
	assert!(try_("[\"a\", \"a\"]", "f64").contains("duplicate column name"));
	assert!(try_("[\"nope\"]", "f64").contains("nope"));
	assert!(try_("[\"a\"]", "f16").contains("dtype must be"));
	assert!(try_("\"a\"", "f64").contains("columns must be a vector"));
	// the list's limits, at and past the boundary, before any column lookup
	let over = format!("[{}]", vec!["\"a\""; 4097].join(", "));
	assert!(try_(&over, "f64").contains("4097 columns, at most 4096"));
	let at = format!("[{}]", vec!["\"a\""; 4096].join(", "));
	assert!(try_(&at, "f64").contains("duplicate column name"));
	let long = format!("[\"{}\"]", "n".repeat(257));
	assert!(try_(&long, "f64").contains("1 to 256 bytes, found 257"));
	let fits = format!("[\"{}\"]", "n".repeat(256));
	let e = try_(&fits, "f64");
	assert!(!e.contains("bytes") && e.contains("not found"), "{e}");
}

#[test]
fn round_trips_and_every_binding_stays_usable() {
	let path = csv("id,x,y\n1,1.5,10\n2,-2.25,20\n3,0.0,30\n");
	let got = outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"id\", \"i64\"), (\"x\", \"f64\"), (\"y\", \"i64\")])?; \
		   let cols = [\"x\", \"y\"]; \
		   let b = df.to_dense(cols, \"f64\")?; \
		   let back = polars::DataFrame::from_dense(b)?; \
		   let wide = df.select_([\"id\"])?.with_dense(b)?; \
		   Ok(`${{b.shape().0}}x${{b.shape().1}} ${{b.dtype()}} ${{cols.len()}} ${{df.height()?}} ${{back.width()}} ${{wide.width()}}`) }}"
	));
	assert_eq!(got.unwrap(), "3x2 f64 2 3 2 3");
	// with_dense: a name already in the frame is Polars' error; heights must match
	let e = outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"id\", \"i64\"), (\"x\", \"f64\"), (\"y\", \"i64\")])?; \
		   df.with_dense(df.to_dense([\"x\"], \"f64\")?)?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(e.contains("with_dense") && e.contains("x"), "{e}");
}

#[test]
fn strings_are_checked_before_copying_and_the_frame_stays_usable() {
	let path = csv("id,text\n1,hello\n2,\"a, b\"\n3,\n");
	let read = format!("polars::read_csv({path:?}, [(\"id\", \"i64\"), (\"text\", \"string\")])?");
	let got = outcome(&format!(
		"pub fn main() {{ let df = {read}; let c = \"text\"; let e = df.strings(c); let n = df.strings(\"id\"); \
		   let ok = df.select_([\"id\"])?.height()?; Ok(`${{e.is_err()}} ${{n.is_err()}} ${{c}} ${{ok}}`) }}"
	));
	assert_eq!(got.unwrap(), "true true text 3");
	let e = outcome(&format!(
		"pub fn main() {{ let df = {read}; df.strings(\"text\")?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(e.contains("a null in column \"text\" at row 2"), "{e}");
	let e = outcome(&format!(
		"pub fn main() {{ let df = {read}; df.strings(\"id\")?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(e.contains("column \"id\" is i64, not str"), "{e}");
	let e = outcome(&format!(
		"pub fn main() {{ let df = {read}; df.strings(\"nope\")?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(e.contains("nope"), "{e}");
	// the values, in order, commas and all
	let path = csv("text\nhello\n\"a, b\"\nzz\n");
	let got = outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"text\", \"string\")])?; let s = df.strings(\"text\")?; Ok(`${{s.len()}}|${{s[0]}}|${{s[1]}}|${{s[2]}}`) }}"
	));
	assert_eq!(got.unwrap(), "3|hello|a, b|zz");
	// the row limit, at and past the boundary
	let rows = |n: usize| {
		let mut b = String::from("text\n");
		for _ in 0..n {
			b.push_str("x\n");
		}
		csv(&b)
	};
	for (n, ok) in [(65_536, true), (65_537, false)] {
		let path = rows(n);
		let r = outcome(&format!(
			"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"text\", \"string\")])?; Ok(`${{df.strings(\"text\")?.len()}}`) }}"
		));
		match ok {
			true => assert_eq!(r.unwrap(), "65536"),
			false => assert!(r.unwrap_err().contains("65537 rows, at most 65536")),
		}
	}
	// the byte limit: 1,025 texts of 64 KiB is just over 64 MiB
	let mut b = String::from("text\n");
	let long = "y".repeat(64 << 10);
	for _ in 0..1025 {
		b.push_str(&long);
		b.push('\n');
	}
	let path = csv(&b);
	let e = outcome(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, [(\"text\", \"string\")])?; df.strings(\"text\")?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(e.contains("more than 67108864 bytes of text"), "{e}");
}
