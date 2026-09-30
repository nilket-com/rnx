//! Record 0125: loading. `read_csv(path)` infers the schema with Polars' own
//! inference (date parsing off); `read_csv(path, schema)` is unchanged;
//! `read_json(path)` reads a local JSON array of objects; and a listed Polars
//! path parameter accepts a local path string, refusing anything Polars would
//! read with a URI scheme, before any Polars call.
#![cfg(feature = "generated")]
use rnx::rune::{self, Context, Module, Source, Sources, Vm};
use std::sync::Arc;

fn run(script: &str) -> Result<rune::Value, String> {
	let mut polars = Module::with_crate("polars").unwrap();
	rnx_polars::build(&mut polars).unwrap();
	let mut context = Context::with_default_modules().unwrap();
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

/// A script's `Ok(String)`, or its `Err` text, or the VM error.
fn text(script: &str) -> Result<String, String> {
	let v = run(script)?;
	match rune::from_value::<Result<String, rune::Value>>(v).map_err(|e| e.to_string())? {
		Ok(s) => Ok(s),
		Err(e) => Err(format!(
			"script error: {}",
			rune::from_value::<String>(e.clone()).unwrap_or_else(|_| format!("{e:?}"))
		)),
	}
}

fn dir(name: &str) -> std::path::PathBuf {
	let d = std::env::temp_dir().join(format!("rnx-0125-{name}-{}", std::process::id()));
	std::fs::create_dir_all(&d).unwrap();
	d
}

fn sales(d: &std::path::Path) -> String {
	let p = d.join("sales.csv");
	std::fs::write(
		&p,
		"id,date,region,qty,price,ok\n1,2026-01-03,north,3,1.25,true\n2,2026-01-04,south,,2.5,false\n",
	)
	.unwrap();
	p.to_str().unwrap().to_owned()
}

#[test]
fn read_csv_with_one_argument_infers_and_keeps_dates_as_strings() {
	let d = dir("infer");
	let path = sales(&d);
	let got = text(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?})?; let d = ``; for x in df.dtypes() {{ d += `${{x}},`; }} Ok(`${{d}}|${{df.height()?}}`) }}"
	))
	.unwrap();
	// Polars' own inference: the date stays a string (date parsing is off)
	assert_eq!(got, "i64,str,str,i64,f64,bool,|2", "{got}");
	// the same frame as Rust's inference is checked by the workflow probe's
	// twin (`probes/0125`, `w1.csv_file`)
}

#[test]
fn read_csv_with_two_arguments_is_the_strict_loader_unchanged() {
	let d = dir("strict");
	let path = sales(&d);
	let schema = r#"[("id", "i64"), ("date", "string"), ("region", "string"), ("qty", "i64"), ("price", "f64"), ("ok", "bool")]"#;
	let got = text(&format!(
		"pub fn main() {{ let df = polars::read_csv({path:?}, {schema})?; Ok(`${{df.height()?}}`) }}"
	))
	.unwrap();
	assert_eq!(got, "2");
	// every strict refusal is as before
	for (schema, want) in [
		(r#"[("id", "i64")]"#, "header arity does not match schema"),
		(
			r#"[("date", "string"), ("id", "i64"), ("region", "string"), ("qty", "i64"), ("price", "f64"), ("ok", "bool")]"#,
			"header names or order do not match schema",
		),
	] {
		let err = text(&format!(
			"pub fn main() {{ polars::read_csv({path:?}, {schema})?; Ok(``) }}"
		))
		.unwrap_err();
		assert!(err.contains(want), "{err}");
	}
	let dir_path = d.to_str().unwrap();
	let err = text(&format!(
		"pub fn main() {{ polars::read_csv({dir_path:?}, {schema})?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(err.contains("not a regular file"), "{err}");
	let err = text(&format!(
		"pub fn main() {{ polars::read_csv({dir_path:?})?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(err.contains("not a regular file"), "{err}");
}

#[test]
fn read_csv_refuses_other_arities_naming_both_forms() {
	for args in ["", r#""a", "b", "c""#] {
		let err = run(&format!("pub fn main() {{ polars::read_csv({args}) }}")).unwrap_err();
		assert!(
			err.contains("read_csv takes (path) or (path, schema)"),
			"{err}"
		);
	}
}

#[test]
fn read_json_reads_a_local_array_of_objects() {
	let d = dir("json");
	let p = d.join("regions.json");
	std::fs::write(&p, r#"[{"region":"north","manager":"Ada","target":10},{"region":"south","manager":"Grace","target":12}]"#).unwrap();
	let path = p.to_str().unwrap();
	let got = text(&format!(
		"pub fn main() {{ let df = polars::read_json({path:?})?; let d = ``; for x in df.dtypes() {{ d += `${{x}},`; }} Ok(`${{d}}|${{df.height()?}}`) }}"
	))
	.unwrap();
	assert_eq!(got, "str,str,i64,|2", "{got}");
	let dir_path = d.to_str().unwrap();
	let err = text(&format!(
		"pub fn main() {{ polars::read_json({dir_path:?})?; Ok(``) }}"
	))
	.unwrap_err();
	assert!(err.contains("not a regular file"), "{err}");
}

/// The refusal a path gets, or `None` if it reaches Polars.
fn path_refusal(path: &str) -> Option<String> {
	let script = format!(
		"pub fn main() {{ match polars::LazyFrame::scan_parquet({path:?}, polars::ScanArgsParquet::default_()) {{ Ok(_) => Ok(`reached`), Err(e) => Ok(`polars`) }} }}"
	);
	match run(&script) {
		Ok(_) => None,
		Err(e) => Some(e),
	}
}

#[test]
fn a_path_argument_is_local_only_checked_before_and_after_conversion() {
	// direct URIs and every `file:` spelling: refused by the raw check
	for uri in [
		"s3://b/x.parquet",
		"gs://b/x",
		"az://b/x",
		"http://h/x",
		"https://h/x",
		"file:///x",
		"file:/x",
		"file:x",
	] {
		let e = path_refusal(uri).unwrap_or_else(|| panic!("{uri} reached Polars"));
		assert!(
			e.contains("LazyFrame::scan_parquet: `path`")
				&& e.contains("is not a local path (a URI"),
			"{uri}: {e}"
		);
	}
	// normalisation to cloud: the raw string has no scheme, but Polars'
	// From<&str> strips the extended-path prefix and turns the backslashes
	// into `s3://`; the converted check refuses it
	let sneaky = r"\\?\s3:\\bucket\x.parquet";
	assert_eq!(
		polars_utils::pl_path::PlRefPath::from(sneaky).as_str(),
		"s3://bucket/x.parquet",
		"the control must normalise to a cloud URI at this pin"
	);
	let e = path_refusal(sneaky).expect("the normalised cloud path reached Polars");
	assert!(e.contains("Polars reads it with a URI scheme"), "{e}");
	let e = path_refusal(r"\\?\file:\x.parquet").expect("the normalised file: path reached Polars");
	assert!(e.contains("Polars reads it with a URI scheme"), "{e}");
	// ordinary local paths reach Polars: absolute, relative, a colon in a
	// later segment, a glob, and Windows drive spellings
	for local in [
		"/no/such/x.parquet",
		"no/such/x.parquet",
		"dir/a:b.parquet",
		"dir/*.parquet",
		"C:/data/x.parquet",
		r"C:\data\x.parquet",
		r"\\?\C:\data\x.parquet",
	] {
		assert_eq!(path_refusal(local), None, "{local} was refused");
	}
}

#[test]
fn a_path_argument_reads_a_local_file_and_refuses_other_types() {
	let d = dir("scan");
	let csv = sales(&d);
	let parquet = d.join("s.parquet");
	let parquet = parquet.to_str().unwrap();
	let got = text(&format!(
		"pub fn main() {{ polars::read_csv({csv:?})?.write_parquet_new({parquet:?})?; \
		 let lf = polars::LazyFrame::scan_parquet({parquet:?}, polars::ScanArgsParquet::default_())?; \
		 Ok(`${{lf.collect()?.height()?}}`) }}"
	))
	.unwrap();
	assert_eq!(got, "2");
	let e = run(
		"pub fn main() { polars::LazyFrame::scan_parquet(5, polars::ScanArgsParquet::default_()) }",
	)
	.unwrap_err();
	assert!(
		e.contains("LazyFrame::scan_parquet: `path` must be String or PlRefPath, found ::std::i64"),
		"{e}"
	);
	// the other listed callables take a string too
	let got = text(&format!(
		"pub fn main() {{ let r = polars::LazyCsvReader::new({csv:?}); Ok(`ok`) }}"
	))
	.unwrap();
	assert_eq!(got, "ok");
	let e = run(r#"pub fn main() { polars::LazyJsonLineReader::new("s3://b/x") }"#).unwrap_err();
	assert!(
		e.contains("LazyJsonLineReader::new: `path`") && e.contains("is not a local path"),
		"{e}"
	);
}

/// Review of 0125: a path is borrowed, never taken from the script's
/// binding. After every call, on success and on a Polars or file error, the
/// script still reads its path. (A URI refusal is a VM error that ends the
/// script, so no later use exists; its dispatch borrows the same way, which
/// the generator's self-test pins.)
#[test]
fn a_path_binding_survives_every_call() {
	let d = dir("reuse");
	let csv = sales(&d);
	let parquet = d.join("r.parquet");
	let parquet = parquet.to_str().unwrap();
	let missing = d.join("missing.parquet");
	let missing = missing.to_str().unwrap();
	let schema = r#"[("id", "i64"), ("date", "string"), ("region", "string"), ("qty", "i64"), ("price", "f64"), ("ok", "bool")]"#;
	let got = text(&format!(
		"pub fn main() {{ \
		   let p = {csv:?}; let q = {parquet:?}; let m = {missing:?}; let n = 0; \
		   let a = polars::read_csv(p)?; n += p.len(); \
		   let b = polars::read_csv(p, {schema})?; n += p.len(); \
		   a.write_parquet_new(q)?; \
		   let lf = polars::LazyFrame::scan_parquet(q, polars::ScanArgsParquet::default_())?; n += q.len(); \
		   let r = polars::LazyCsvReader::new(p); n += p.len(); \
		   let e1 = polars::read_csv(m); n += m.len(); \
		   let e2 = polars::read_csv(m, {schema}); n += m.len(); \
		   let e3 = polars::LazyFrame::scan_parquet(m, polars::ScanArgsParquet::default_())?.collect(); n += m.len(); \
		   Ok(`${{n}} ${{e1.is_err()}} ${{e2.is_err()}} ${{e3.is_err()}}`) }}"
	))
	.unwrap();
	let (p, q, m) = (csv.len(), parquet.len(), missing.len());
	assert_eq!(got, format!("{} true true true", 3 * p + q + 3 * m));
}
