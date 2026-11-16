//! Durable receipts in the bytes Python's tools write and read.
use crate::pyjson;
use anyhow::{Context as _, Result};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest as _, Sha256};
use std::io::{Read as _, Write as _};
use std::path::Path;

/// A field Python either omits, writes as null, or writes with a value.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Field<T> {
	#[default]
	Absent,
	Null,
	Value(T),
}

impl<T> Field<T> {
	pub fn is_absent(&self) -> bool {
		matches!(self, Field::Absent)
	}

	pub fn value(&self) -> Option<&T> {
		match self {
			Field::Value(v) => Some(v),
			_ => None,
		}
	}
}

impl<T> From<Option<T>> for Field<T> {
	/// Python's `dict.get(...)` written back: None becomes null.
	fn from(value: Option<T>) -> Self {
		value.map_or(Field::Null, Field::Value)
	}
}

impl<T: Serialize> Serialize for Field<T> {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		match self {
			// Struct fields use skip_serializing_if = "Field::is_absent".
			Field::Absent | Field::Null => serializer.serialize_none(),
			Field::Value(v) => v.serialize(serializer),
		}
	}
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Field<T> {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		// A missing field never reaches here: serde(default) makes it Absent.
		Ok(Option::<T>::deserialize(deserializer)?.into())
	}
}

/// SHA-256 of a file's stored bytes.
pub fn digest(path: &Path) -> Result<String> {
	let mut file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
	let mut hasher = Sha256::new();
	let mut buffer = vec![0; 1024 * 1024];
	loop {
		let n = file.read(&mut buffer)?;
		if n == 0 {
			break;
		}
		hasher.update(&buffer[..n]);
	}
	Ok(pyjson::hex(&hasher.finalize()))
}

pub fn sync_directory(path: &Path) -> Result<()> {
	std::fs::File::open(path)
		.and_then(|d| d.sync_all())
		.with_context(|| format!("sync {}", path.display()))
}

/// Write `value` as Python's atomic_json does: a synced temporary file in
/// the same directory, renamed over the target, then the directory synced.
pub fn atomic_json<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<()> {
	let text = pyjson::pretty(&serde_json::to_value(value)?)?;
	let parent = path.parent().context("receipt path has no directory")?;
	let name = path
		.file_name()
		.context("receipt path has no name")?
		.to_string_lossy();
	let nonce: u64 = rand_nonce();
	let temporary = parent.join(format!(".{name}.{nonce:016x}.pending"));
	let result = (|| -> Result<()> {
		let mut file = std::fs::OpenOptions::new()
			.write(true)
			.create_new(true)
			.open(&temporary)?;
		file.write_all(text.as_bytes())?;
		file.sync_all()?;
		std::fs::rename(&temporary, path)?;
		sync_directory(parent)
	})();
	let _ = std::fs::remove_file(&temporary);
	result.with_context(|| format!("write {}", path.display()))
}

fn rand_nonce() -> u64 {
	let mut bytes = [0u8; 8];
	if std::fs::File::open("/dev/urandom")
		.and_then(|mut f| f.read_exact(&mut bytes))
		.is_err()
	{
		let nanos = std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.map(|d| d.as_nanos())
			.unwrap_or(0);
		return nanos as u64 ^ std::process::id() as u64;
	}
	u64::from_le_bytes(bytes)
}

/// A fresh 32-hex-digit identifier, as Python's uuid4().hex.
pub fn invocation_id() -> String {
	format!("{:016x}{:016x}", rand_nonce(), rand_nonce())
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	#[derive(Serialize, Deserialize, PartialEq, Debug)]
	struct Row {
		#[serde(default, skip_serializing_if = "Field::is_absent")]
		error: Field<String>,
	}

	#[test]
	fn absence_null_and_value_round_trip_distinctly() {
		for (text, field) in [
			("{}", Field::Absent),
			(r#"{"error":null}"#, Field::Null),
			(r#"{"error":"x"}"#, Field::Value("x".to_string())),
		] {
			let row: Row = serde_json::from_str(text).unwrap();
			assert_eq!(row.error, field);
			assert_eq!(
				pyjson::compact(&serde_json::to_value(&row).unwrap()).unwrap(),
				text
			);
		}
	}

	#[test]
	fn atomic_json_writes_python_bytes_and_leaves_no_temporary() {
		let dir = std::env::temp_dir().join(format!("rnx-final-receipt-{}", invocation_id()));
		std::fs::create_dir(&dir).unwrap();
		let path = dir.join("RESULT.json");
		atomic_json(&path, &json!({"b": 1.0, "a": "é"})).unwrap();
		assert_eq!(
			std::fs::read_to_string(&path).unwrap(),
			"{\n  \"a\": \"\\u00e9\",\n  \"b\": 1.0\n}\n"
		);
		assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
		std::fs::remove_dir_all(&dir).unwrap();
	}
}
