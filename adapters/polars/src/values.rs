use crate::{Expr, p, rune};
use rune::runtime::Vec as RuneVec;
use std::{collections::HashSet, sync::Arc};

pub(crate) fn schema(value: rune::Value) -> Result<Arc<p::Schema>, String> {
	let values = value
		.borrow_ref::<RuneVec>()
		.map_err(|_| "polars read_csv: schema must be a vector")?;
	if values.is_empty() {
		return Err("polars read_csv: schema must not be empty".into());
	}
	let mut fields = Vec::new();
	let mut names = HashSet::new();
	for (i, value) in values.iter().enumerate() {
		let tuple = value.borrow_tuple_ref().map_err(|_| {
			format!(
				"polars read_csv: schema item {} must be a (name, dtype) tuple",
				i + 1
			)
		})?;
		if tuple.len() != 2 {
			return Err(format!(
				"polars read_csv: schema item {} needs two fields",
				i + 1
			));
		}
		let name = tuple[0]
			.borrow_string_ref()
			.map_err(|_| "polars read_csv: schema name must be a string")?;
		if name.is_empty() || !names.insert(name.to_string()) {
			return Err("polars read_csv: schema names must be nonempty and unique".into());
		}
		let kind = tuple[1]
			.borrow_string_ref()
			.map_err(|_| "polars read_csv: dtype must be a string")?;
		let dtype = match &*kind {
			"string" => p::DataType::String,
			"i64" => p::DataType::Int64,
			"f64" => p::DataType::Float64,
			"bool" => p::DataType::Boolean,
			_ => return Err("polars read_csv: dtype must be string, i64, f64 or bool".into()),
		};
		fields.push((p::PlSmallStr::from_str(&name), dtype));
	}
	Ok(Arc::new(p::Schema::from_iter(fields)))
}
pub(crate) fn literal(value: rune::Value) -> Result<Expr, String> {
	let expr = if let Ok(v) = value.as_bool() {
		p::lit(v)
	} else if let Ok(v) = value.as_signed() {
		p::lit(v)
	} else if let Ok(v) = value.as_float() {
		if !v.is_finite() {
			return Err("polars lit: float must be finite".into());
		}
		p::lit(v)
	} else if let Ok(v) = value.borrow_string_ref() {
		p::lit(v.to_string())
	} else {
		return Err(format!(
			"polars lit: unsupported type {}",
			value.type_info()
		));
	};
	Ok(Expr(expr))
}
