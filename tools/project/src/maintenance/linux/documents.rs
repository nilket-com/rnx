//! Bounded JSON inspection, including duplicate-key refusal at every level.
use super::{Budget, Result};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde_json::{Map, Value};
struct Seed<'a>(&'a mut Budget);
impl<'de> DeserializeSeed<'de> for Seed<'_> {
	type Value = Value;
	fn deserialize<D: de::Deserializer<'de>>(self, d: D) -> std::result::Result<Value, D::Error> {
		self.0.reserve(128).map_err(de::Error::custom)?;
		d.deserialize_any(self)
	}
}
impl<'de> Visitor<'de> for Seed<'_> {
	type Value = Value;
	fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
		f.write_str("bounded JSON without duplicate keys")
	}
	fn visit_unit<E: de::Error>(self) -> std::result::Result<Value, E> {
		Ok(Value::Null)
	}
	fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Value, E> {
		Ok(v.into())
	}
	fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Value, E> {
		Ok(v.into())
	}
	fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Value, E> {
		Ok(v.into())
	}
	fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Value, E> {
		serde_json::Number::from_f64(v)
			.map(Value::Number)
			.ok_or_else(|| E::custom("nonfinite number"))
	}
	fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Value, E> {
		self.0.reserve(v.len() as u64).map_err(E::custom)?;
		Ok(v.into())
	}
	fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
		let mut v = vec![];
		while let Some(n) = a.next_element_seed(Seed(&mut *self.0))? {
			v.push(n)
		}
		Ok(v.into())
	}
	fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> std::result::Result<Value, A::Error> {
		let mut v = Map::new();
		while let Some(k) = a.next_key::<String>()? {
			self.0
				.reserve(k.len() as u64 + 128)
				.map_err(de::Error::custom)?;
			if v.contains_key(&k) {
				return Err(de::Error::custom("duplicate JSON key"));
			}
			v.insert(k, a.next_value_seed(Seed(&mut *self.0))?);
		}
		Ok(v.into())
	}
}
pub(super) fn decode(bytes: &[u8], budget: &mut Budget) -> Result<Value> {
	let mut d = serde_json::Deserializer::from_slice(bytes);
	let v = Seed(budget)
		.deserialize(&mut d)
		.map_err(|e| e.to_string())?;
	d.end().map_err(|e| e.to_string())?;
	Ok(v)
}
