use bytes::BytesMut;
use rnx::rune;
use rune::runtime::{Bytes, Object, Value, Vec as RuneVec};
use std::error::Error;
use tokio_postgres::types::{FromSql, IsNull, ToSql, Type, to_sql_checked};

pub(crate) const LIMIT: usize = 8 * 1024 * 1024;
const MIN_MS: i128 = -377705023201000;
const MAX_MS: i128 = 253402207200999;
const PG_EPOCH_US: i128 = 946684800000000;

pub(crate) fn error(e: impl std::fmt::Display) -> String {
	e.to_string()
}

pub(crate) fn string(s: &str) -> Result<Value, String> {
	Value::try_from(rune::alloc::String::try_from(s).map_err(error)?).map_err(error)
}

pub(crate) fn insert(o: &mut Object, key: &str, value: Value) -> Result<(), String> {
	o.insert(rune::alloc::String::try_from(key).map_err(error)?, value)
		.map_err(error)?;
	Ok(())
}

pub(crate) struct Account(usize);
impl Account {
	pub(crate) fn new() -> Self {
		Self(0)
	}
	pub(crate) fn charge(&mut self, bytes: usize, row: usize) -> Result<(), String> {
		self.0 = self.0.checked_add(bytes).ok_or_else(|| self.refusal(row))?;
		if self.0 > LIMIT {
			return Err(self.refusal(row));
		}
		Ok(())
	}
	fn refusal(&self, row: usize) -> String {
		format!("logical payload exceeds {LIMIT} bytes at row {row} (row 0 is SQL and parameters)")
	}
}

#[derive(Debug)]
pub(crate) enum Param {
	Null,
	Bool(bool),
	Int(i64),
	Float(f64),
	Text(String),
	Bytes(Vec<u8>),
}
impl Param {
	pub(crate) fn ty(&self) -> Type {
		match self {
			Self::Null => Type::UNKNOWN,
			Self::Bool(_) => Type::BOOL,
			Self::Int(_) => Type::INT8,
			Self::Float(_) => Type::FLOAT8,
			Self::Text(_) => Type::TEXT,
			Self::Bytes(_) => Type::BYTEA,
		}
	}
}
impl ToSql for Param {
	fn to_sql(
		&self,
		ty: &Type,
		out: &mut BytesMut,
	) -> Result<IsNull, Box<dyn Error + Sync + Send>> {
		match self {
			Self::Null => Ok(IsNull::Yes),
			Self::Bool(v) => v.to_sql(ty, out),
			Self::Int(v) => v.to_sql(ty, out),
			Self::Float(v) => v.to_sql(ty, out),
			Self::Text(v) => v.to_sql(ty, out),
			Self::Bytes(v) => v.to_sql(ty, out),
		}
	}
	fn accepts(_: &Type) -> bool {
		// prepare_typed fixes every non-NULL parameter to its wire type.
		true
	}
	to_sql_checked!();
}

pub(crate) fn params(value: Value, account: &mut Account) -> Result<Vec<Param>, String> {
	let values = value
		.borrow_ref::<RuneVec>()
		.map_err(|_| "params must be a vector")?;
	if values.len() > 1000 {
		return Err("params exceeds 1000 values".into());
	}
	let mut result = Vec::with_capacity(values.len());
	for (index, value) in values.iter().enumerate() {
		account.charge(16, 0)?;
		let param = if value.into_unit().is_ok() {
			Param::Null
		} else if let Ok(v) = value.as_bool() {
			Param::Bool(v)
		} else if let Ok(v) = value.as_signed() {
			Param::Int(v)
		} else if let Ok(v) = value.as_float() {
			Param::Float(v)
		} else if let Ok(v) = value.borrow_string_ref() {
			if v.contains('\0') {
				return Err(format!("parameter {} contains NUL", index + 1));
			}
			account.charge(v.len(), 0)?;
			Param::Text(v.to_string())
		} else if let Ok(v) = value.borrow_ref::<Bytes>() {
			account.charge(v.len(), 0)?;
			Param::Bytes(v.to_vec())
		} else {
			return Err(format!(
				"parameter {} has unsupported Rune type {}",
				index + 1,
				value.type_info()
			));
		};
		result.push(param);
	}
	Ok(result)
}

pub(crate) fn supported(ty: &Type) -> bool {
	matches!(
		*ty,
		Type::BOOL
			| Type::INT2
			| Type::INT4
			| Type::INT8
			| Type::FLOAT4
			| Type::FLOAT8
			| Type::TEXT
			| Type::VARCHAR
			| Type::BPCHAR
			| Type::NAME
			| Type::BYTEA
			| Type::JSON
			| Type::JSONB
			| Type::TIMESTAMPTZ
	)
}

struct Raw<'a>(&'a [u8]);
impl<'a> FromSql<'a> for Raw<'a> {
	fn from_sql(_: &Type, raw: &'a [u8]) -> Result<Self, Box<dyn Error + Sync + Send>> {
		Ok(Self(raw))
	}
	fn accepts(_: &Type) -> bool {
		true
	}
}

fn moment(pg_microseconds: i64) -> Result<i64, String> {
	if matches!(pg_microseconds, i64::MIN | i64::MAX) {
		return Err("infinite timestamptz is not a time moment".into());
	}
	let us = i128::from(pg_microseconds) + PG_EPOCH_US;
	if !(MIN_MS * 1000..=MAX_MS * 1000).contains(&us) {
		return Err(format!(
			"timestamptz is outside the time range {MIN_MS} through {MAX_MS} milliseconds"
		));
	}
	Ok(us.div_euclid(1000) as i64)
}

fn scalar<T: for<'a> FromSql<'a>>(ty: &Type, raw: &[u8]) -> Result<T, String> {
	T::from_sql(ty, raw).map_err(error)
}

fn decode(
	ty: &Type,
	raw: Option<Raw<'_>>,
	account: &mut Account,
	row: usize,
) -> Result<Value, String> {
	account.charge(16, row)?;
	let Some(Raw(raw)) = raw else {
		return Ok(Value::from(()));
	};
	match *ty {
		Type::BOOL => Ok(Value::from(scalar::<bool>(ty, raw)?)),
		Type::INT2 => Ok(Value::from(i64::from(scalar::<i16>(ty, raw)?))),
		Type::INT4 => Ok(Value::from(i64::from(scalar::<i32>(ty, raw)?))),
		Type::INT8 => Ok(Value::from(scalar::<i64>(ty, raw)?)),
		Type::FLOAT4 => Ok(Value::from(f64::from(scalar::<f32>(ty, raw)?))),
		Type::FLOAT8 => Ok(Value::from(scalar::<f64>(ty, raw)?)),
		Type::TIMESTAMPTZ => Ok(Value::from(moment(scalar::<i64>(&Type::INT8, raw)?)?)),
		Type::BYTEA => {
			account.charge(raw.len(), row)?;
			Value::try_from(Bytes::from_vec(
				rune::alloc::Vec::try_from(raw).map_err(error)?,
			))
			.map_err(error)
		}
		Type::TEXT | Type::VARCHAR | Type::BPCHAR | Type::NAME | Type::JSON | Type::JSONB => {
			let raw = if *ty == Type::JSONB {
				match raw.split_first() {
					Some((1, body)) => body,
					_ => return Err("unsupported jsonb wire version".into()),
				}
			} else {
				raw
			};
			account.charge(raw.len(), row)?;
			string(std::str::from_utf8(raw).map_err(error)?)
		}
		_ => Err(format!("unsupported PostgreSQL type {}", ty.name())),
	}
}

pub(crate) fn row(
	value: tokio_postgres::Row,
	account: &mut Account,
	number: usize,
) -> Result<Value, String> {
	if number > 10000 {
		return Err(format!("result exceeds 10000 rows at row {number}"));
	}
	account.charge(16, number)?;
	let mut object = Object::with_capacity(value.len()).map_err(error)?;
	for (index, column) in value.columns().iter().enumerate() {
		account.charge(column.name().len(), number)?;
		let raw = value.try_get::<_, Option<Raw<'_>>>(index).map_err(error)?;
		let item = decode(column.type_(), raw, account, number).map_err(|e| {
			format!(
				"column {:?} ({}): {e}",
				column.name(),
				column.type_().name()
			)
		})?;
		insert(&mut object, column.name(), item)?;
	}
	Value::try_from(object).map_err(error)
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn moments_check_the_original_instant_before_flooring() {
		for ms in [MIN_MS, MAX_MS] {
			assert_eq!(moment((ms * 1000 - PG_EPOCH_US) as i64).unwrap(), ms as i64);
		}
		assert!(moment((MIN_MS * 1000 - PG_EPOCH_US - 1) as i64).is_err());
		assert!(moment((MAX_MS * 1000 - PG_EPOCH_US + 1) as i64).is_err());
		assert_eq!(moment((-PG_EPOCH_US - 1) as i64).unwrap(), -1);
		assert!(moment(i64::MAX).is_err());
		assert!(moment(i64::MIN).is_err());
	}
	#[test]
	fn account_accepts_exactly_the_limit_and_refuses_overflow() {
		let mut account = Account::new();
		account.charge(LIMIT, 1).unwrap();
		assert!(account.charge(1, 2).unwrap_err().contains("row 2"));
		assert!(account.charge(usize::MAX, 3).is_err());
	}
}
