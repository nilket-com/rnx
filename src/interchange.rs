//! Record 0129: a neutral dense numeric block, the one value native adapters
//! exchange numbers through. rnx owns it because it is the only crate every
//! adapter depends on; an adapter's own types are private to it. A block is
//! a row-major matrix of `f32` or `f64`, at least 1 × 1, with one distinct
//! name per column, and every value finite. It is immutable: its fields are
//! private, [`Dense::new`] is the only constructor and checks every
//! invariant, and cloning shares the buffer.
use rune::{Any, Context, Module};
use std::sync::Arc;

/// At most 2²⁴ values: 64 MiB of `f32` or 128 MiB of `f64`.
pub const MAX_VALUES: usize = 1 << 24;
/// At most 4,096 columns.
pub const MAX_COLUMNS: usize = 4096;
/// A column name is 1 to 256 bytes.
pub const MAX_NAME: usize = 256;

/// The values, row-major. The dtype is the variant. `Arc<Vec<T>>` (not
/// `Arc<[T]>`): `Arc::new(vec)` moves a `Vec` in without copying its
/// elements.
#[derive(Clone, Debug)]
pub enum Data {
	F32(Arc<Vec<f32>>),
	F64(Arc<Vec<f64>>),
}

impl Data {
	pub fn len(&self) -> usize {
		match self {
			Data::F32(v) => v.len(),
			Data::F64(v) => v.len(),
		}
	}
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}
	pub fn dtype(&self) -> &'static str {
		match self {
			Data::F32(_) => "f32",
			Data::F64(_) => "f64",
		}
	}
}

#[derive(Any, Clone, Debug)]
#[rune(item = ::interchange)]
pub struct Dense {
	data: Data,
	rows: usize,
	columns: usize,
	names: Arc<Vec<String>>,
}

/// The shape's own limits, checked before anything proportional to it is
/// allocated: both axes at least 1, at most [`MAX_COLUMNS`] columns, and
/// `rows × columns` (checked) at most [`MAX_VALUES`].
pub fn check_shape(rows: usize, columns: usize) -> Result<usize, String> {
	if rows == 0 || columns == 0 {
		return Err(format!(
			"interchange: a block needs at least one row and one column, found {rows} x {columns}"
		));
	}
	if columns > MAX_COLUMNS {
		return Err(format!(
			"interchange: {columns} columns, at most {MAX_COLUMNS}"
		));
	}
	match rows.checked_mul(columns) {
		Some(n) if n <= MAX_VALUES => Ok(n),
		_ => Err(format!(
			"interchange: {rows} x {columns} values, at most {MAX_VALUES}"
		)),
	}
}

/// The names' own limits: exactly `columns` of them, each 1 to [`MAX_NAME`]
/// bytes, all distinct.
pub fn check_names(names: &[String], columns: usize) -> Result<(), String> {
	if names.len() != columns {
		return Err(format!(
			"interchange: {} names for {columns} columns",
			names.len()
		));
	}
	let mut seen = std::collections::HashSet::new();
	for n in names {
		if n.is_empty() || n.len() > MAX_NAME {
			return Err(format!(
				"interchange: a column name is 1 to {MAX_NAME} bytes, found {}",
				n.len()
			));
		}
		if !seen.insert(n.as_str()) {
			return Err(format!("interchange: duplicate column name {n:?}"));
		}
	}
	Ok(())
}

/// A list of names borrowed from a script, checked before anything is
/// copied: the vector's length first (1 to [`MAX_COLUMNS`]), then each
/// borrowed string's byte length (1 to [`MAX_NAME`]), then distinctness.
/// Only a list that passes is cloned, so a refused list costs at most the
/// borrow guards of 4,096 entries. The script's vector stays usable.
pub fn names_from(value: &rune::Value, op: &str, what: &str) -> Result<Vec<String>, String> {
	let not_names = || format!("{op}: {what} must be a vector of names");
	let values = value
		.borrow_ref::<rune::runtime::Vec>()
		.map_err(|_| not_names())?;
	if values.is_empty() {
		return Err(format!("{op}: a block needs at least one column"));
	}
	if values.len() > MAX_COLUMNS {
		return Err(format!(
			"{op}: {} {what}, at most {MAX_COLUMNS}",
			values.len()
		));
	}
	let mut borrowed = Vec::with_capacity(values.len());
	for v in values.iter() {
		let name = v.borrow_string_ref().map_err(|_| not_names())?;
		if name.is_empty() || name.len() > MAX_NAME {
			return Err(format!(
				"{op}: a column name is 1 to {MAX_NAME} bytes, found {}",
				name.len()
			));
		}
		borrowed.push(name);
	}
	let mut seen = std::collections::HashSet::with_capacity(borrowed.len());
	for name in &borrowed {
		if !seen.insert(&**name) {
			return Err(format!("{op}: duplicate column name {:?}", &**name));
		}
	}
	Ok(borrowed.iter().map(|name| name.to_string()).collect())
}

impl Dense {
	/// The only constructor: the shape and names within their limits,
	/// `rows × columns` equal to the buffer's length, and every value finite.
	pub fn new(
		data: Data,
		rows: usize,
		columns: usize,
		names: Vec<String>,
	) -> Result<Dense, String> {
		let n = check_shape(rows, columns)?;
		check_names(&names, columns)?;
		if data.len() != n {
			return Err(format!(
				"interchange: {rows} x {columns} = {n} values, the buffer holds {}",
				data.len()
			));
		}
		let bad = match &data {
			Data::F32(v) => v.iter().position(|x| !x.is_finite()),
			Data::F64(v) => v.iter().position(|x| !x.is_finite()),
		};
		if let Some(i) = bad {
			return Err(format!(
				"interchange: a non-finite value at row {}, column {:?}",
				i / columns,
				names[i % columns]
			));
		}
		Ok(Dense {
			data,
			rows,
			columns,
			names: Arc::new(names),
		})
	}
	pub fn data(&self) -> &Data {
		&self.data
	}
	pub fn rows(&self) -> usize {
		self.rows
	}
	pub fn columns(&self) -> usize {
		self.columns
	}
	pub fn names(&self) -> &[String] {
		&self.names
	}
	pub fn dtype(&self) -> &'static str {
		self.data.dtype()
	}
}

// the script-visible surface: a block is read, never built, by scripts
fn shape(this: &Dense) -> (i64, i64) {
	(this.rows as i64, this.columns as i64)
}
fn names(this: &Dense) -> Vec<String> {
	this.names.to_vec()
}
fn dtype(this: &Dense) -> String {
	this.dtype().into()
}
/// A bounded summary: the dtype, the shape and at most 8 names, each cut at
/// 32 characters and escaped. Never the values.
pub fn summary(this: &Dense) -> String {
	use std::fmt::Write;
	let mut text = format!("Dense[{}; {} x {}](", this.dtype(), this.rows, this.columns);
	for (i, n) in this.names.iter().take(8).enumerate() {
		if i > 0 {
			text.push_str(", ");
		}
		let cut: String = n.chars().take(32).collect();
		let _ = write!(text, "{}", cut.escape_debug());
		if cut.len() < n.len() {
			text.push('…');
		}
	}
	if this.columns > 8 {
		text.push_str(", …");
	}
	text.push(')');
	text
}
fn display(this: &Dense, f: &mut rune::runtime::Formatter) -> rune::runtime::VmResult<()> {
	use rune::alloc::fmt::TryWrite;
	rune::vm_try!(f.try_write_str(&summary(this)));
	rune::runtime::VmResult::Ok(())
}

/// The prompt's presenter: the same bounded summary as `DISPLAY_FMT`.
pub(crate) fn present(presenters: &mut crate::present::Presenters) -> Result<(), String> {
	presenters.set_owner("interchange");
	presenters.register::<Dense>(|block, out| {
		out.push(&summary(block));
		Ok(())
	})
}

/// The `interchange` module: the `Dense` type and its read-only surface.
/// rnx installs it in every context; an adapter's own tests (or an
/// embedder building a context by hand) install it with this.
pub fn module() -> Result<Module, rune::ContextError> {
	let mut module = Module::with_crate("interchange")?;
	module.ty::<Dense>()?;
	module.associated_function("shape", shape)?;
	module.associated_function("names", names)?;
	module.associated_function("dtype", dtype)?;
	module.associated_function(&rune::runtime::Protocol::DISPLAY_FMT, display)?;
	Ok(module)
}

pub(crate) fn install(context: &mut Context) -> crate::Result<Vec<crate::host::HostFunction>> {
	context.install(module()?)?;
	Ok(vec![
		crate::host::HostFunction {
			path: "interchange::Dense".into(),
			doc: "Dense: a finite f32/f64 row-major matrix with named columns, at most 2^24 values; adapters exchange numbers through it",
		},
		crate::host::HostFunction {
			path: "interchange::Dense::shape".into(),
			doc: "shape() -> (rows, columns)",
		},
		crate::host::HostFunction {
			path: "interchange::Dense::names".into(),
			doc: "names() -> Vec<String>: the column names",
		},
		crate::host::HostFunction {
			path: "interchange::Dense::dtype".into(),
			doc: "dtype() -> String: \"f32\" or \"f64\"",
		},
	])
}

#[cfg(test)]
mod tests {
	use super::*;

	fn names(n: usize) -> Vec<String> {
		(0..n).map(|i| format!("c{i}")).collect()
	}

	#[test]
	fn the_constructor_enforces_every_invariant() {
		let ok = Dense::new(
			Data::F32(Arc::new(vec![1.0, 2.0, 3.0, 4.0])),
			2,
			2,
			names(2),
		)
		.unwrap();
		assert_eq!((ok.rows(), ok.columns(), ok.dtype()), (2, 2, "f32"));
		let refused = |d: Result<Dense, String>, want: &str| {
			let e = d.unwrap_err();
			assert!(e.contains(want), "{e} (wanted {want})");
		};
		refused(
			Dense::new(Data::F64(Arc::new(vec![])), 0, 1, names(1)),
			"at least one row",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![])), 1, 0, vec![]),
			"at least one row and one column",
		);
		refused(
			Dense::new(
				Data::F64(Arc::new(vec![1.0])),
				1,
				MAX_COLUMNS + 1,
				names(MAX_COLUMNS + 1),
			),
			"columns, at most",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![1.0])), usize::MAX, 2, names(2)),
			"values, at most",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![1.0; 3])), 2, 2, names(2)),
			"the buffer holds 3",
		);
		refused(
			Dense::new(
				Data::F64(Arc::new(vec![1.0; 2])),
				1,
				2,
				vec!["a".into(), "a".into()],
			),
			"duplicate column name",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![1.0; 2])), 1, 2, vec!["a".into()]),
			"1 names for 2 columns",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![1.0])), 1, 1, vec![String::new()]),
			"1 to 256 bytes",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![1.0])), 1, 1, vec!["x".repeat(257)]),
			"1 to 256 bytes",
		);
		refused(
			Dense::new(Data::F64(Arc::new(vec![1.0, f64::NAN])), 1, 2, names(2)),
			"non-finite value at row 0, column \"c1\"",
		);
		refused(
			Dense::new(Data::F32(Arc::new(vec![f32::INFINITY])), 1, 1, names(1)),
			"non-finite",
		);
		// the limits, at their boundary
		assert!(check_shape(MAX_VALUES / MAX_COLUMNS, MAX_COLUMNS).is_ok());
		assert!(check_shape(MAX_VALUES / MAX_COLUMNS + 1, MAX_COLUMNS).is_err());
		assert!(check_shape(MAX_VALUES, 1).is_ok());
		assert!(check_shape(MAX_VALUES + 1, 1).is_err());
		// a script's name list: length first, then each borrowed name, then
		// distinctness; the list stays usable
		let list = |v: Vec<String>| rune::to_value(v).unwrap();
		let from = |v: &rune::Value| names_from(v, "op", "names");
		assert_eq!(from(&list(names(MAX_COLUMNS))).unwrap().len(), MAX_COLUMNS);
		refused(
			from(&list(names(MAX_COLUMNS + 1))).map(|_| ok.clone()),
			"4097 names, at most 4096",
		);
		refused(
			from(&list(vec![])).map(|_| ok.clone()),
			"at least one column",
		);
		assert!(from(&list(vec!["x".repeat(MAX_NAME)])).is_ok());
		refused(
			from(&list(vec!["x".repeat(MAX_NAME + 1)])).map(|_| ok.clone()),
			"1 to 256 bytes, found 257",
		);
		refused(
			from(&list(vec![String::new()])).map(|_| ok.clone()),
			"1 to 256 bytes, found 0",
		);
		refused(
			from(&list(vec!["a".into(), "a".into()])).map(|_| ok.clone()),
			"op: duplicate column name \"a\"",
		);
		refused(
			from(&rune::to_value(1i64).unwrap()).map(|_| ok.clone()),
			"names must be a vector of names",
		);
		let mixed = rune::to_value(vec![
			rune::to_value("a").unwrap(),
			rune::to_value(1i64).unwrap(),
		])
		.unwrap();
		refused(
			from(&mixed).map(|_| ok.clone()),
			"names must be a vector of names",
		);
		let kept = list(vec!["a".into(), "b".into()]);
		from(&kept).unwrap();
		assert_eq!(rune::from_value::<Vec<String>>(kept).unwrap(), ["a", "b"]);
		// the prompt presents the same bounded summary as `println!`
		let mut presenters = crate::present::Presenters::default();
		present(&mut presenters).unwrap();
		let wide = Dense::new(
			Data::F64(Arc::new(vec![0.0; 12])),
			1,
			12,
			(0..12)
				.map(|i| format!("{}\u{1b}", "n".repeat(40 + i)))
				.collect(),
		)
		.unwrap();
		let shown = presenters
			.present(&rune::to_value(wide.clone()).unwrap(), 4096)
			.unwrap()
			.unwrap();
		assert_eq!(shown, summary(&wide));
		assert!(shown.starts_with("Dense[f64; 1 x 12](") && shown.ends_with(", …)"));
		assert!(!shown.contains('\u{1b}') && shown.len() < 512, "{shown}");
		// a clone shares the buffer
		let c = ok.clone();
		match (ok.data(), c.data()) {
			(Data::F32(a), Data::F32(b)) => assert!(Arc::ptr_eq(a, b)),
			_ => unreachable!(),
		}
	}
}
