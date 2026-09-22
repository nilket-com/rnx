//! Record 0073: support for the generated oracle tests. Hand-written,
//! only built with `test-support`. Values are compared structurally with
//! a per-case order policy, errors by kind, panics by message, and every
//! outcome the tests have not explicitly approved fails. Oracle
//! variability never excuses a binding failure: the binding side is
//! judged first, and known nondeterministic oracles are excluded per case
//! by the generator, not tolerated here.
use polars::prelude as p;
use rnx::rune;

/// A shown value: plain text, or a frame as its header plus one string
/// per row, so rows are never split out of a delimited text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Repr {
	Text(String),
	Frame { head: String, rows: Vec<String> },
	/// A materialized sequence (record 0077): one representation per
	/// element, in order, so equal-length sequences whose element texts
	/// would join alike stay apart; options and tuples nest as sequences
	/// tagged by their first element.
	Seq(Vec<Repr>),
}

impl Repr {
	/// Ordered flat text, for nesting inside options, vectors and tuples.
	pub fn to_text(&self) -> String {
		match self {
			Repr::Text(s) => s.clone(),
			Repr::Frame { head, rows } => format!("{head}; rows=[{}]", rows.join(" | ")),
			// length-framed elements keep the text injective
			Repr::Seq(items) => format!("[{}]", items.iter().map(|i| { let t = i.to_text(); format!("{}:{t}", t.len()) }).collect::<Vec<_>>().join(", ")),
		}
	}
	pub fn prefixed(self, prefix: &str) -> Repr {
		match self {
			Repr::Text(s) => Repr::Text(format!("{prefix}{s}")),
			Repr::Frame { head, rows } => Repr::Frame { head: format!("{prefix}{head}"), rows },
			Repr::Seq(items) => Repr::Seq(std::iter::once(Repr::Text(prefix.to_string())).chain(items).collect()),
		}
	}
}

/// One cell, structurally: a scalar as its `AnyValue` debug text (exact
/// for scalars: floats print at full precision); a list or array cell as
/// the full representation of its inner series, recursively; a struct
/// cell by its fields' names and values. Polars's display formatting,
/// which elides middle values, is never used for a nested value (record
/// 0076).
pub fn anyvalue_repr(v: &p::AnyValue) -> String {
	match v {
		p::AnyValue::List(inner) => format!("List({})", series_repr(inner).to_text()),
		#[allow(unreachable_patterns)]
		other => match other.dtype() {
			p::DataType::Struct(_) => {
				// materialise the one cell as a series and render it structurally
				let one = p::Series::from_any_values_and_dtype("cell".into(), std::slice::from_ref(other), &other.dtype(), true);
				match one {
					Ok(s) => format!("{:?}{}", other.dtype(), nested_series_body(&s)),
					Err(e) => format!("<cell error: {e}>"),
				}
			}
			_ => format!("{other:?}"),
		},
	}
}

/// The body of a nested series: for a struct, each field's name and full
/// representation; for anything else, its cells.
fn nested_series_body(s: &p::Series) -> String {
	match s.dtype() {
		p::DataType::Struct(_) => match s.struct_() {
			Ok(sc) => {
				let fields: Vec<String> = sc.fields_as_series().iter().map(|f| series_repr(f).to_text()).collect();
				format!("{{{}}}", fields.join("; "))
			}
			Err(e) => format!("<struct error: {e}>"),
		},
		_ => {
			let cells: Vec<String> = (0..s.len()).map(|i| match s.get(i) { Ok(v) => anyvalue_repr(&v), Err(e) => format!("<get error: {e}>") }).collect();
			format!("[{}]", cells.join(", "))
		}
	}
}

/// Every cell of a series, by index, structurally (see `anyvalue_repr`);
/// nulls included; a struct series by its fields. Independent of Polars's
/// `fmt` feature.
pub fn series_repr(s: &p::Series) -> Repr {
	let head = format!("{}:{:?}:", s.name(), s.dtype());
	if matches!(s.dtype(), p::DataType::Struct(_)) {
		// the struct's own validity per row, apart from its fields' nulls: a
		// null struct and a valid struct whose fields are null differ
		let nulls = s.is_null();
		let valid: Vec<String> = (0..nulls.len()).map(|i| match nulls.get(i) { Some(true) => "null".to_string(), Some(false) => "valid".to_string(), None => "?".to_string() }).collect();
		return Repr::Text(format!("{head}len={}:rows=[{}]:{}", s.len(), valid.join(", "), nested_series_body(s)));
	}
	let mut out = head;
	out.push('[');
	for i in 0..s.len() {
		if i > 0 {
			out.push_str(", ");
		}
		match s.get(i) {
			Ok(v) => out.push_str(&anyvalue_repr(&v)),
			Err(e) => out.push_str(&format!("<get error: {e}>")),
		}
	}
	out.push(']');
	Repr::Text(out)
}

pub fn column_repr(c: &p::Column) -> Repr {
	series_repr(c.as_materialized_series())
}

/// Dimensions and the columns with name and dtype in the header, then
/// one entry per row with all its cells.
pub fn frame_repr(df: &p::DataFrame) -> Repr {
	let mut head = format!("shape=({}, {}); columns=[", df.height(), df.width());
	let mut cols = Vec::new();
	for name in df.get_column_names() {
		match df.column(name) {
			Ok(c) => {
				head.push_str(&format!("{}:{:?},", c.name(), c.dtype()));
				cols.push(c.as_materialized_series().clone());
			}
			Err(e) => head.push_str(&format!("<column error: {e}>,")),
		}
	}
	head.push(']');
	let mut rows = Vec::with_capacity(df.height());
	for i in 0..df.height() {
		let mut row = String::from("(");
		for (j, c) in cols.iter().enumerate() {
			if j > 0 {
				row.push_str(", ");
			}
			match c.get(i) {
				Ok(v) => row.push_str(&anyvalue_repr(&v)),
				Err(e) => row.push_str(&format!("<get error: {e}>")),
			}
		}
		row.push(')');
		rows.push(row);
	}
	Repr::Frame { head, rows }
}

/// The kind name Rust matches on, the same derivation the generated
/// `polars::Error` uses.
pub fn error_kind(e: &p::PolarsError) -> String {
	let kind = format!("{e:?}");
	kind.split(['(', ' ', '{']).next().unwrap_or("Unknown").to_string()
}

/// The kind of a `polars::Error` held in a Rune value.
pub fn rune_error_kind(v: &rune::Value) -> Result<String, String> {
	v.borrow_ref::<crate::generated::support::Error>()
		.map(|e| e.0.clone())
		.map_err(|e| e.to_string())
}

/// What one side produced.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Side {
	Value(Repr),
	/// A Polars error of this kind.
	Error(String),
	/// A panic with this message.
	Panic(String),
	/// The script could not run: compile or VM error, or a conversion failure.
	Broken(String),
}

/// One Rust oracle run: its setup stage either failed before the measured
/// call, or ran the call and produced a `Side`. The generated oracle
/// function builds its fixtures under `catch_unwind` and passes them into
/// the call, so every run has its own staged setup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Staged {
	SetupFailed(String),
	Ran(Side),
}

/// How values are compared for one case. Exact ordered values by
/// default; unordered rows only where the generator identified an
/// operation whose Rust contract leaves row order unspecified.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
	pub unordered_rows: bool,
}

pub const ORDERED: Policy = Policy { unordered_rows: false };
pub const UNORDERED: Policy = Policy { unordered_rows: true };

/// What the setup stage of one side did. Setup is a separate execution
/// stage (the script's `setup` function; the case's Rust setup function)
/// that builds every fixture the measured call needs and nothing else; a
/// failure there is structural, whatever its message says.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Setup {
	/// The stage did not run because the script did not compile.
	NotRun,
	Ok,
	/// A panic, VM error or refused fixture, with its message.
	Failed(String),
}

/// The classification the test asserts on. Only `Match`, `RowOrderDiffers`
/// (under an unordered policy), `BothError` with equal kinds and
/// `BothPanic` with equal messages are approved; everything else fails.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
	/// Setup failed on both sides before the measured call, which was
	/// therefore never made. Counted apart; never verified; not a run
	/// failure.
	FixtureFailed,
	/// Setup failed on one side only: a constructor binding or a Rust
	/// fixture is broken. A failure.
	SetupMismatch,
	Match,
	RowOrderDiffers,
	BothError,
	BothPanic,
	ReuseFailed,
	Mismatch,
	ErrorKindMismatch,
	PanicMismatch,
	BindingPanicked,
	OraclePanicked,
	/// The oracle gave two different values: the case must be excluded
	/// explicitly by the generator, not tolerated.
	Nondeterministic,
	Broken,
}

impl Outcome {
	pub fn approved(&self) -> bool {
		matches!(self, Outcome::Match | Outcome::RowOrderDiffers | Outcome::BothError | Outcome::BothPanic)
	}
	/// Setup failure is neither approved nor a failure of the run.
	pub fn setup_failed(&self) -> bool {
		matches!(self, Outcome::FixtureFailed)
	}
	pub fn name(&self) -> &'static str {
		match self {
			Outcome::FixtureFailed => "fixture_failed",
			Outcome::SetupMismatch => "setup_mismatch",
			Outcome::Match => "match",
			Outcome::RowOrderDiffers => "row_order_differs",
			Outcome::BothError => "both_error",
			Outcome::BothPanic => "both_panic",
			Outcome::ReuseFailed => "reuse_failed",
			Outcome::Mismatch => "mismatch",
			Outcome::ErrorKindMismatch => "error_kind_mismatch",
			Outcome::PanicMismatch => "panic_mismatch",
			Outcome::BindingPanicked => "binding_panicked",
			Outcome::OraclePanicked => "oracle_panicked",
			Outcome::Nondeterministic => "oracle_nondeterministic",
			Outcome::Broken => "broken",
		}
	}
}

/// Value equality under a policy: `Some(true)` exact, `Some(false)` equal
/// as row sets (only under an unordered policy), `None` different.
fn same(policy: Policy, a: &Repr, b: &Repr) -> Option<bool> {
	if a == b {
		return Some(true);
	}
	match (a, b) {
		(Repr::Frame { head: ha, rows: ra }, Repr::Frame { head: hb, rows: rb }) if policy.unordered_rows && ha == hb => {
			let mut x = ra.clone();
			let mut y = rb.clone();
			x.sort();
			y.sort();
			if x == y { Some(false) } else { None }
		}
		_ => None,
	}
}

/// Classify one case. `rune_setup` and `rust_setup` are the two setup
/// stages; `first` is the binding's result, `second` the same call again
/// on the same receiver (instance methods only), `oracle` the Rust result
/// and `again` a second run of the oracle. A script that did not compile
/// is judged before any setup: nothing about a fixture excuses a binding
/// that does not build.
pub fn classify(policy: Policy, rune_setup: &Setup, rust_setup: &Setup, first: &Side, second: Option<&Side>, oracle: &Side, again: &Side) -> (Outcome, String) {
	if *rune_setup == Setup::NotRun {
		let why = match first { Side::Broken(e) => e.clone(), other => format!("setup did not run: {other:?}") };
		return (Outcome::Broken, why);
	}
	match (rune_setup, rust_setup) {
		(Setup::Failed(a), Setup::Failed(b)) => return (Outcome::FixtureFailed, format!("rune setup {a:?}; polars setup {b:?}")),
		(Setup::Failed(a), _) => return (Outcome::SetupMismatch, format!("rune setup failed {a:?}; polars setup {rust_setup:?}")),
		(_, Setup::Failed(b)) => return (Outcome::SetupMismatch, format!("polars setup failed {b:?}; rune setup {rune_setup:?}")),
		(Setup::Ok, Setup::Ok) => {}
		(_, Setup::NotRun) | (Setup::NotRun, _) => return (Outcome::Broken, "setup did not run".into()),
	}
	// The binding side is judged first: nothing about the oracle excuses
	// a script that did not run, or panicked where Rust did not.
	if let Side::Broken(e) = first {
		return (Outcome::Broken, e.clone());
	}
	// Then the oracle must agree with itself: no approved outcome rests
	// on a Rust result that a second run did not reproduce.
	let agrees = match (oracle, again) {
		(Side::Value(a), Side::Value(b)) => same(policy, a, b).is_some(),
		(a, b) => a == b,
	};
	if !agrees {
		return (Outcome::Nondeterministic, format!("polars gave {oracle:?} then {again:?}"));
	}
	let base = match (first, oracle) {
		(Side::Broken(e), _) => return (Outcome::Broken, e.clone()),
		(Side::Panic(a), Side::Panic(b)) => {
			if a == b { (Outcome::BothPanic, a.clone()) } else { return (Outcome::PanicMismatch, format!("rune {a:?} polars {b:?}")) }
		}
		(Side::Panic(m), _) => return (Outcome::BindingPanicked, m.clone()),
		(_, Side::Panic(m)) => return (Outcome::OraclePanicked, m.clone()),
		(_, Side::Broken(e)) => return (Outcome::Broken, format!("oracle: {e}")),
		(Side::Value(a), Side::Value(b)) => match same(policy, a, b) {
			Some(true) => (Outcome::Match, a.to_text()),
			Some(false) => (Outcome::RowOrderDiffers, a.to_text()),
			None => return (Outcome::Mismatch, format!("rune {:?} polars {:?}", a.to_text(), b.to_text())),
		},
		(Side::Error(a), Side::Error(b)) => {
			if a == b { (Outcome::BothError, a.clone()) } else { return (Outcome::ErrorKindMismatch, format!("rune {a} polars {b}")) }
		}
		(Side::Value(a), Side::Error(b)) => return (Outcome::Mismatch, format!("rune value {:?} polars error {b}", a.to_text())),
		(Side::Error(a), Side::Value(b)) => return (Outcome::Mismatch, format!("rune error {a} polars value {:?}", b.to_text())),
	};
	if let Some(s) = second {
		let reused = match (s, first) {
			(Side::Value(a), Side::Value(b)) => same(policy, a, b).is_some(),
			_ => s == first,
		};
		if !reused {
			return (Outcome::ReuseFailed, format!("second call on the same receiver gave {s:?}"));
		}
	}
	base
}

/// A formatted Rust result: a nested `<<ERR:kind>>` marker means the
/// wrapper would have failed with that kind.
pub fn collapse(s: String) -> Side {
	if let Some(i) = s.find("<<ERR:") {
		let kind = s[i + 6..].split(">>").next().unwrap_or("Unknown");
		return Side::Error(kind.to_string());
	}
	Side::Value(Repr::Text(s))
}

pub fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
	e.downcast_ref::<String>().cloned().or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_else(|| "<non-string panic>".to_string())
}

#[cfg(test)]
mod controls {
	//! Negative controls on the classifier: each injected wrong outcome
	//! must fail closed. The generated runner has its own integrated
	//! controls on top of these.
	use super::*;
	/// Classify with both setup stages succeeded.
	fn ok(policy: Policy, first: &Side, second: Option<&Side>, oracle: &Side, again: &Side) -> (Outcome, String) {
		classify(policy, &Setup::Ok, &Setup::Ok, first, second, oracle, again)
	}
	use polars::prelude::*;

	fn df() -> p::DataFrame {
		polars::df!("x" => [1i64, 2, 3], "y" => ["a", "b", "c"]).unwrap()
	}
	fn v(r: Repr) -> Side {
		Side::Value(r)
	}

	#[test]
	fn same_shape_wrong_frame_is_distinguished() {
		let a = frame_repr(&df());
		let b = frame_repr(&polars::df!("x" => [1i64, 2, 4], "y" => ["a", "b", "c"]).unwrap());
		assert_ne!(a, b);
		let (o, _) = ok(ORDERED, &v(a.clone()), None, &v(b.clone()), &v(b.clone()));
		assert_eq!(o, Outcome::Mismatch);
		let (o, _) = ok(UNORDERED, &v(a), None, &v(b.clone()), &v(b));
		assert_eq!(o, Outcome::Mismatch, "a changed cell is a mismatch even under an unordered policy");
	}

	#[test]
	fn a_permutation_is_a_mismatch_unless_the_case_is_unordered() {
		let a = frame_repr(&df());
		let b = frame_repr(&df().reverse());
		let (o, _) = ok(ORDERED, &v(a.clone()), None, &v(b.clone()), &v(b.clone()));
		assert_eq!(o, Outcome::Mismatch, "a reversed frame under the default policy must fail");
		let (o, _) = ok(UNORDERED, &v(a), None, &v(b.clone()), &v(b));
		assert_eq!(o, Outcome::RowOrderDiffers);
	}

	#[test]
	fn rows_with_the_delimiter_in_a_cell_stay_one_row() {
		let r = frame_repr(&polars::df!("s" => ["a | b"]).unwrap());
		match r {
			Repr::Frame { rows, .. } => assert_eq!(rows.len(), 1),
			_ => panic!(),
		}
	}

	#[test]
	fn nulls_and_dtypes_are_visible() {
		let s = p::Series::new("n".into(), [Some(1i64), None]);
		assert_eq!(series_repr(&s), Repr::Text("n:Int64:[Int64(1), Null]".into()));
	}

	#[test]
	fn changed_error_kind_fails() {
		let e = |k: &str| Side::Error(k.into());
		assert_eq!(ok(ORDERED, &e("ColumnNotFound"), None, &e("ComputeError"), &e("ComputeError")).0, Outcome::ErrorKindMismatch);
		assert_eq!(ok(ORDERED, &e("ColumnNotFound"), None, &e("ColumnNotFound"), &e("ColumnNotFound")).0, Outcome::BothError);
	}

	#[test]
	fn unrelated_panic_fails() {
		let pnc = |m: &str| Side::Panic(m.into());
		assert_eq!(ok(ORDERED, &pnc("injected"), None, &pnc("polars said no"), &pnc("polars said no")).0, Outcome::PanicMismatch);
		assert_eq!(ok(ORDERED, &pnc("same"), None, &pnc("same"), &pnc("same")).0, Outcome::BothPanic);
	}

	#[test]
	fn wrong_second_call_fails_under_both_policies() {
		let a = frame_repr(&df());
		let b = frame_repr(&polars::df!("x" => [9i64], "y" => ["z"]).unwrap());
		assert_eq!(ok(ORDERED, &v(a.clone()), Some(&v(b.clone())), &v(a.clone()), &v(a.clone())).0, Outcome::ReuseFailed);
		assert_eq!(ok(UNORDERED, &v(a.clone()), Some(&v(b)), &v(a.clone()), &v(a.clone())).0, Outcome::ReuseFailed);
		// a second call in another row order fails under the default policy
		let r = frame_repr(&df().reverse());
		assert_eq!(ok(ORDERED, &v(a.clone()), Some(&v(r.clone())), &v(a.clone()), &v(a.clone())).0, Outcome::ReuseFailed);
		assert_eq!(ok(UNORDERED, &v(a.clone()), Some(&v(r)), &v(a.clone()), &v(a)).0, Outcome::Match);
	}

	#[test]
	fn binding_success_against_panicking_oracle_fails() {
		let t = |s: &str| v(Repr::Text(s.into()));
		let (o, _) = ok(ORDERED, &t("1"), None, &Side::Panic("boom".into()), &Side::Panic("boom".into()));
		assert_eq!(o, Outcome::OraclePanicked);
		let (o, _) = ok(ORDERED, &t("1"), None, &Side::Panic("activate 'x' feature".into()), &Side::Panic("activate 'x' feature".into()));
		assert_eq!(o, Outcome::OraclePanicked, "a feature-gated Rust panic is not an excuse for a binding that returned a value");
	}

	#[test]
	fn nondeterministic_oracle_never_excuses_the_binding() {
		let t = |s: &str| v(Repr::Text(s.into()));
		let (run1, run2) = (t("run1"), t("run2"));
		// a broken binding is reported as such; every other binding result
		// against a disagreeing oracle is unexpected nondeterminism, which fails
		assert_eq!(ok(ORDERED, &Side::Broken("compile error".into()), None, &run1, &run2).0, Outcome::Broken);
		assert_eq!(ok(ORDERED, &Side::Panic("unrelated".into()), None, &run1, &run2).0, Outcome::Nondeterministic);
		assert_eq!(ok(ORDERED, &t("wrong"), None, &run1, &run2).0, Outcome::Nondeterministic);
		assert_eq!(ok(ORDERED, &t("run1"), None, &run1, &run2).0, Outcome::Nondeterministic);
		assert_eq!(ok(ORDERED, &t("run1"), Some(&t("other")), &run1, &run2).0, Outcome::Nondeterministic);
		assert!(!Outcome::Nondeterministic.approved(), "unexpected nondeterminism fails; known cases are excluded by the generator");
	}

	#[test]
	fn a_matching_first_run_never_hides_a_disagreeing_second_run() {
		let a = v(frame_repr(&df()));
		let changed = v(frame_repr(&df().reverse()));
		assert_eq!(ok(ORDERED, &a, None, &a, &changed).0, Outcome::Nondeterministic);
		assert_eq!(ok(ORDERED, &a, None, &a, &Side::Error("ComputeError".into())).0, Outcome::Nondeterministic);
		assert_eq!(ok(ORDERED, &a, None, &a, &Side::Panic("later".into())).0, Outcome::Nondeterministic);
		let e = Side::Error("ComputeError".into());
		assert_eq!(ok(ORDERED, &e, None, &e, &Side::Error("Other".into())).0, Outcome::Nondeterministic);
		let pnc = Side::Panic("same".into());
		assert_eq!(ok(ORDERED, &pnc, None, &pnc, &Side::Panic("other".into())).0, Outcome::Nondeterministic);
		// a broken binding still fails as broken, before agreement is asked
		assert_eq!(ok(ORDERED, &Side::Broken("compile".into()), None, &a, &changed).0, Outcome::Broken);
	}

	#[test]
	fn setup_failure_is_neither_verified_nor_a_match() {
		let failed = Setup::Failed("new failed: boom".into());
		let v = Side::Value(Repr::Text("1".into()));
		let unrun = Side::Broken("target not run".into());
		// both setups fail: counted apart, never verified, not a failure
		let (o, _) = classify(ORDERED, &failed, &failed, &unrun, None, &unrun, &unrun);
		assert_eq!(o, Outcome::FixtureFailed);
		assert!(!o.approved() && o.setup_failed());
		// one side failing setup is a failure of that side's fixture path
		assert_eq!(classify(ORDERED, &failed, &Setup::Ok, &unrun, None, &v, &v).0, Outcome::SetupMismatch);
		assert_eq!(classify(ORDERED, &Setup::Ok, &failed, &v, None, &unrun, &unrun).0, Outcome::SetupMismatch);
		assert!(!Outcome::SetupMismatch.approved() && !Outcome::SetupMismatch.setup_failed());
	}
	#[test]
	fn nested_values_are_compared_structurally() {
		// a list cell whose 21st inner value differs, beyond what Debug shows
		let inner_a: Vec<i64> = (0..40).collect();
		let mut inner_b = inner_a.clone();
		inner_b[20] = 999;
		let a = p::Series::new("x".into(), [p::Series::new("i".into(), inner_a)]);
		let b = p::Series::new("x".into(), [p::Series::new("i".into(), inner_b)]);
		assert_ne!(series_repr(&a), series_repr(&b), "an inner element hidden by Debug's ellipsis must be seen");
		assert_eq!(ok(ORDERED, &v(series_repr(&a)), None, &v(series_repr(&b)), &v(series_repr(&b))).0, Outcome::Mismatch);
		assert_eq!(ok(ORDERED, &v(series_repr(&a)), None, &v(series_repr(&a.clone())), &v(series_repr(&a))).0, Outcome::Match);
		// a null moved inside a list cell
		let c = p::Series::new("x".into(), [p::Series::new("i".into(), [Some(1i64), None, Some(3)])]);
		let d = p::Series::new("x".into(), [p::Series::new("i".into(), [None, Some(1i64), Some(3)])]);
		assert_ne!(series_repr(&c), series_repr(&d));
		// a struct field value, and a struct field name
		let e = df().into_struct("s".into()).into_series();
		let f = p::df!("x" => [1i64, 2, 4], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap().into_struct("s".into()).into_series();
		let g = p::df!("w" => [1i64, 2, 3], "y" => ["a", "b", "c"], "z" => [1.5f64, 2.5, 3.5]).unwrap().into_struct("s".into()).into_series();
		assert_ne!(series_repr(&e), series_repr(&f), "a changed struct field value must be seen");
		assert_ne!(series_repr(&e), series_repr(&g), "a renamed struct field must be seen");
		assert_eq!(series_repr(&e), series_repr(&df().into_struct("s".into()).into_series()));
	}
	#[test]
	fn struct_nullness_is_part_of_the_value() {
		use polars::prelude::IntoSeries;
		// a valid struct whose one field is null, against a null struct
		let valid = p::df!("x" => [None::<i64>], "y" => ["a"]).unwrap().into_struct("s".into()).into_series();
		let null = p::Series::full_null("s".into(), 1, valid.dtype());
		assert_eq!(valid.null_count(), 0);
		assert_eq!(null.null_count(), 1);
		assert_ne!(series_repr(&valid), series_repr(&null), "a null struct must not equal a valid struct with null fields");
		assert_eq!(ok(ORDERED, &v(series_repr(&valid)), None, &v(series_repr(&null)), &v(series_repr(&null))).0, Outcome::Mismatch);
		// an outer null moved to the other row
		let mut a = null.clone();
		a.append(&valid).unwrap();
		let mut b = valid.clone();
		b.append(&null).unwrap();
		assert_ne!(series_repr(&a), series_repr(&b), "a moved outer null must be seen");
		// the same distinctions nested in a list cell
		let la = p::Series::new("l".into(), [valid.clone()]);
		let lb = p::Series::new("l".into(), [null.clone()]);
		assert_ne!(series_repr(&la), series_repr(&lb), "a null struct inside a list cell must be seen");
		let lc = p::Series::new("l".into(), [a.clone()]);
		let ld = p::Series::new("l".into(), [b.clone()]);
		assert_ne!(series_repr(&lc), series_repr(&ld));
		assert_eq!(series_repr(&la), series_repr(&p::Series::new("l".into(), [valid.clone()])));
	}
	#[test]
	fn sequences_are_compared_structurally() {
		let t = |s: &str| Repr::Text(s.to_string());
		// equal-length string vectors whose joined text would collide
		let a = Repr::Seq(vec![t("a, b"), t("c")]);
		let b = Repr::Seq(vec![t("a"), t("b, c")]);
		assert_ne!(a, b);
		assert_ne!(a.to_text(), b.to_text(), "the framed text is injective too");
		assert_eq!(ok(ORDERED, &v(a.clone()), None, &v(b.clone()), &v(b.clone())).0, Outcome::Mismatch);
		assert_eq!(ok(ORDERED, &v(a.clone()), None, &v(a.clone()), &v(a.clone())).0, Outcome::Match);
		// a changed element, reordered elements, an option distinction
		assert_ne!(Repr::Seq(vec![t("1"), t("2")]), Repr::Seq(vec![t("1"), t("3")]));
		assert_ne!(Repr::Seq(vec![t("1"), t("2")]), Repr::Seq(vec![t("2"), t("1")]));
		assert_eq!(ok(UNORDERED, &v(Repr::Seq(vec![t("1"), t("2")])), None, &v(Repr::Seq(vec![t("2"), t("1")])), &v(Repr::Seq(vec![t("2"), t("1")]))).0, Outcome::Mismatch, "a sequence is ordered under every policy");
		assert_ne!(Repr::Seq(vec![Repr::Seq(vec![t("Some"), t("1")])]), Repr::Seq(vec![t("None")]));
		// a changed cell in a nested series element
		let s1 = series_repr(&p::Series::new("x".into(), [1i64, 2, 3]));
		let s2 = series_repr(&p::Series::new("x".into(), [1i64, 2, 4]));
		assert_ne!(Repr::Seq(vec![s1.clone()]), Repr::Seq(vec![s2]));
		assert_eq!(Repr::Seq(vec![s1.clone()]), Repr::Seq(vec![s1]));
		// a length difference
		assert_ne!(Repr::Seq(vec![t("1")]), Repr::Seq(vec![t("1"), t("1")]));
	}
	#[test]
	fn setup_is_a_stage_not_a_message() {
		// a target panic that happens to say "fixture:" is a target panic
		let f = Side::Panic("fixture: new failed: boom".into());
		assert_eq!(ok(ORDERED, &f, None, &f, &f).0, Outcome::BothPanic);
		let v = Side::Value(Repr::Text("1".into()));
		assert_eq!(ok(ORDERED, &f, None, &v, &v).0, Outcome::BindingPanicked);
		// a script that does not compile is broken whatever the Rust setup did
		let compile = Side::Broken("compile: bad".into());
		let failed = Setup::Failed("fixture: refused".into());
		assert_eq!(classify(ORDERED, &Setup::NotRun, &failed, &compile, None, &v, &v).0, Outcome::Broken);
		assert_eq!(classify(ORDERED, &Setup::NotRun, &Setup::Ok, &compile, None, &v, &v).0, Outcome::Broken);
		// a wrong value is a mismatch even when the second Rust run fails
		let wrong = Side::Value(Repr::Text("2".into()));
		assert_ne!(classify(ORDERED, &Setup::Ok, &Setup::Ok, &wrong, None, &v, &Side::Panic("fixture: second".into())).0, Outcome::FixtureFailed);
		assert!(!classify(ORDERED, &Setup::Ok, &Setup::Ok, &wrong, None, &v, &Side::Panic("fixture: second".into())).0.approved());
	}

	#[test]
	fn value_against_error_fails_both_ways() {
		let t = v(Repr::Text("1".into()));
		let e = Side::Error("ComputeError".into());
		assert_eq!(ok(ORDERED, &t, None, &e, &e).0, Outcome::Mismatch);
		assert_eq!(ok(ORDERED, &e, None, &t, &t).0, Outcome::Mismatch);
	}

	#[test]
	fn error_kind_derivation_matches_the_generated_error() {
		let e = p::PolarsError::ColumnNotFound("x".into());
		assert_eq!(error_kind(&e), "ColumnNotFound");
		assert_eq!(crate::generated::support::Error::from(e).0, "ColumnNotFound");
	}
}
