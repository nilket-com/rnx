//! Pin storage only: classification always uses expanded observations.
use serde_json::Value;

fn equal_class(value: &Value) -> bool {
	value["class"].as_str() == Some("Equal")
}

fn pairs(value: &Value, writing: bool) -> Result<Value, &'static str> {
	match value {
		Value::Array(items) => items.iter().map(|v| pairs(v, writing)).collect(),
		Value::Object(fields) => {
			let mut fields = fields
				.iter()
				// Observations are opaque payloads, including objects whose own keys
				// happen to be named class/both/legacy/domain.
				.map(|(k, v)| {
					let v = if matches!(k.as_str(), "both" | "legacy" | "domain") {
						v.clone()
					} else {
						pairs(v, writing)?
					};
					Ok((k.clone(), v))
				})
				.collect::<Result<serde_json::Map<_, _>, &'static str>>()?;
			if value.get("class").is_some() {
				let both = fields.remove("both");
				let legacy = fields.remove("legacy");
				let domain = fields.remove("domain");
				match (both, legacy, domain) {
					(Some(_), _, _) if writing => return Err("expanded row contains both"),
					(Some(v), None, None) => {
						fields.insert("legacy".into(), v.clone());
						fields.insert("domain".into(), v);
					}
					(None, Some(a), Some(b)) => {
						if equal_class(value) && (!writing || a != b) {
							return Err("equality row requires one identical observation");
						}
						if writing && a == b {
							fields.insert("both".into(), a);
						} else {
							if !writing && a == b {
								return Err("identical observations require both");
							}
							fields.insert("legacy".into(), a);
							fields.insert("domain".into(), b);
						}
					}
					(None, None, None) if !equal_class(value) => {}
					_ => return Err("mixed or incomplete observation pair"),
				}
			}
			Ok(Value::Object(fields))
		}
		_ => Ok(value.clone()),
	}
}

pub(super) fn encode(value: &Value) -> Result<Value, &'static str> {
	if value.get("schema").is_some() {
		return Err("expanded table already has a storage schema");
	}
	let mut result = pairs(value, true)?;
	result
		.as_object_mut()
		.ok_or("table must be an object")?
		.insert("schema".into(), 1.into());
	Ok(result)
}

pub(super) fn expand(value: &Value) -> Result<Value, &'static str> {
	if value.get("schema") != Some(&Value::from(1)) {
		return Err("unknown parity storage schema");
	}
	let mut result = pairs(value, false)?;
	result
		.as_object_mut()
		.ok_or("table must be an object")?
		.remove("schema");
	Ok(result)
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[test]
	fn current_table_round_trip_is_exact_and_canonical() {
		let stored: Value =
			serde_json::from_str(include_str!("../../../tests/fixtures/session_parity.json"))
				.unwrap();
		let expanded = expand(&stored).unwrap();
		assert_eq!(encode(&expanded).unwrap(), stored);
		assert_eq!(expand(&encode(&expanded).unwrap()).unwrap(), expanded);
	}

	#[test]
	fn equality_cannot_store_differing_values_or_two_copies() {
		let class = "Equal";
		let mut row = json!({"class":class,"legacy":{"value":7},"domain":{"value":7}});
		let stored = encode(&json!({"rows":[row.clone()]})).unwrap();
		assert_eq!(stored["rows"][0]["both"], row["legacy"]);
		assert!(stored["rows"][0].get("legacy").is_none());
		row["domain"]["value"] = 8.into();
		assert!(encode(&json!({"rows":[row.clone()]})).is_err());
		assert!(expand(&json!({"schema":1,"rows":[row.clone()]})).is_err());
		row["domain"]["value"] = 7.into();
		assert!(expand(&json!({"schema":1,"rows":[row]})).is_err());
	}

	#[test]
	fn mixed_partial_unknown_and_double_encoded_forms_are_refused() {
		for row in [
			json!({"class":"Equal","both":7,"legacy":7}),
			json!({"class":"Equal","both":7,"domain":7}),
			json!({"class":"Equal","both":7,"legacy":7,"domain":7}),
			json!({"class":"Wording","legacy":7}),
			json!({"class":"Equal"}),
		] {
			assert!(expand(&json!({"schema":1,"rows":[row.clone()]})).is_err());
			assert!(encode(&json!({"rows":[row]})).is_err());
		}
		assert!(expand(&json!({"schema":2,"rows":[]})).is_err());
		assert!(expand(&json!({"rows":[]})).is_err());
		assert!(encode(&json!({"schema":1,"rows":[]})).is_err());
	}

	#[test]
	fn property_equality_can_preserve_different_observations() {
		let raw = json!({"rows":[{"class":"Equal by property","legacy":{"clock":1},"domain":{"clock":2}}]});
		let stored = encode(&raw).unwrap();
		assert!(stored["rows"][0].get("both").is_none());
		assert_eq!(expand(&stored).unwrap(), raw);
	}

	#[test]
	fn observation_keys_are_opaque_data() {
		let observation = json!({"class":"Equal","legacy":1,"both":{"class":"Equal"}});
		let raw = json!({"rows":[{"class":"Equal","legacy":observation,"domain":observation}]});
		let stored = encode(&raw).unwrap();
		assert_eq!(stored["rows"][0]["both"], observation);
		assert_eq!(expand(&stored).unwrap(), raw);
	}

	#[test]
	fn different_pairs_and_unpaired_rows_preserve_every_field() {
		let raw = json!({"rows":[
			{"class":"Wording","legacy":{"message":"old"},"domain":{"message":"new"},"rule":"reviewed"},
			{"class":"Not classified","blocked_by":"row 3"}
		]});
		assert_eq!(expand(&encode(&raw).unwrap()).unwrap(), raw);
	}
}
