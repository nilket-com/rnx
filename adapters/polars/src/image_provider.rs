//! Borrowed schema view for the file provider. The parser and credit checks
//! are shared with the checked stock file path.
use super::*;
pub use crate::values::{SchemaKind, SchemaNode, SchemaTree};
use rnx::{
	interchange::Dense,
	rune::host::{ColumnSelection, OpaqueProducerLimit as Limit},
};

pub fn read_csv(path: &str, schema: &impl SchemaTree) -> Result<Result<DataFrame, String>, Limit> {
	let (parsed, _) = match values::csv_schema_session(schema, path.len())? {
		Ok(pair) => pair,
		Err(message) => return Ok(Err(message)),
	};
	load_file_schema(path, parsed)
}
pub fn retained(frame: &DataFrame) -> usize {
	dataframe_bytes(frame)
}
pub fn width(frame: &DataFrame) -> i64 {
	frame.0.width() as i64
}
pub fn display(frame: &DataFrame) -> String {
	preview::render(&frame.0)
		.unwrap_or_else(|e| format!("<polars::DataFrame: preview unavailable: {e}>"))
}
pub fn to_dense(
	frame: &DataFrame,
	columns: ColumnSelection,
	credit: usize,
) -> Result<Result<Dense, String>, Limit> {
	dense::checked::convert(frame, columns, credit)
}
