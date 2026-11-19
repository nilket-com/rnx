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
		let dtype = schema_dtype(&kind)?;
		fields.push((p::PlSmallStr::from_str(&name), dtype));
	}
	Ok(Arc::new(p::Schema::from_iter(fields)))
}

fn schema_dtype(kind: &str) -> Result<p::DataType, String> {
	match kind {
		"string" => Ok(p::DataType::String),
		"i64" => Ok(p::DataType::Int64),
		"f64" => Ok(p::DataType::Float64),
		"bool" => Ok(p::DataType::Boolean),
		_ => Err("polars read_csv: dtype must be string, i64, f64 or bool".into()),
	}
}

#[cfg(feature = "generated")]
pub(crate) fn csv_schema_session(
	tree: &rune::host::OwnedTreeInput,
	path_bytes: usize,
) -> Result<Result<(Arc<p::Schema>, usize), String>, rune::host::OpaqueProducerLimit> {
	use rune::host::{OpaqueProducerLimit, TreeKind};
	let Some(root) = tree
		.node(0)
		.filter(|root| matches!(root.kind(), TreeKind::Vec))
	else {
		return Ok(Err("polars read_csv: schema must be a vector".into()));
	};
	if root.frontier() {
		return Err(OpaqueProducerLimit);
	}
	if root.is_empty() {
		return Ok(Err("polars read_csv: schema must not be empty".into()));
	}
	// The path bridge string and possible renderings stay live at this
	// boundary. The per-column charge covers the field vector, duplicate
	// set and both index and value storage of the final Polars Schema.
	let planning = path_bytes
		.checked_mul(16)
		.and_then(|path| path.checked_add(4096))
		.and_then(|path| path.checked_add(root.len().checked_mul(512)?))
		.ok_or(OpaqueProducerLimit)?;
	let mut remaining = tree
		.remaining_bytes()
		.and_then(|limit| limit.checked_sub(planning))
		.ok_or(OpaqueProducerLimit)?;
	let mut fields = Vec::new();
	fields
		.try_reserve_exact(root.len())
		.map_err(|_| OpaqueProducerLimit)?;
	let mut names = HashSet::new();
	names
		.try_reserve(root.len())
		.map_err(|_| OpaqueProducerLimit)?;
	for i in 0..root.len() {
		let field = (|| -> Result<_, String> {
			let tuple = root
				.child(i)
				.and_then(|(_, child)| tree.node(child))
				.filter(|node| matches!(node.kind(), TreeKind::Tuple))
				.ok_or_else(|| {
					format!(
						"polars read_csv: schema item {} must be a (name, dtype) tuple",
						i + 1
					)
				})?;
			if tuple.len() != 2 || tuple.frontier() {
				return Err(format!(
					"polars read_csv: schema item {} needs two fields",
					i + 1
				));
			}
			let field = |i| match tuple
				.child(i)
				.and_then(|(_, child)| tree.node(child))
				.map(|node| node.kind())
			{
				Some(TreeKind::String(text)) => Some(text.as_str()),
				_ => None,
			};
			let name = field(0).ok_or("polars read_csv: schema name must be a string")?;
			if name.is_empty() || !names.insert(name) {
				return Err("polars read_csv: schema names must be nonempty and unique".into());
			}
			let kind = field(1).ok_or("polars read_csv: dtype must be a string")?;
			Ok((name, schema_dtype(kind)?))
		})();
		let (name, dtype) = match field {
			Ok(field) => field,
			Err(error) => return Ok(Err(error)),
		};
		remaining = name
			.len()
			.checked_mul(4)
			.and_then(|bytes| remaining.checked_sub(bytes))
			.ok_or(OpaqueProducerLimit)?;
		fields.push((p::PlSmallStr::from_str(name), dtype));
	}
	drop(names);
	Ok(Ok((Arc::new(p::Schema::from_iter(fields)), remaining)))
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
