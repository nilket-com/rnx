//! A source map is an explicit runner input, never an ambient context setting.
use super::Mounts;
use serde::Deserialize;
use std::io::Read;
use std::path::{Path, PathBuf};
const LIMIT: usize = 16 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
	format: u32,
	entry: String,
	#[serde(deserialize_with = "mount_list")]
	mounts: Vec<Mount>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Mount {
	#[serde(deserialize_with = "prefix_list")]
	prefix: Vec<String>,
	root: String,
}
fn bounded<'de, D, T, const N: usize>(de: D) -> Result<Vec<T>, D::Error>
where
	D: serde::Deserializer<'de>,
	T: Deserialize<'de>,
{
	struct List<T, const N: usize>(std::marker::PhantomData<T>);
	impl<'de, T: Deserialize<'de>, const N: usize> serde::de::Visitor<'de> for List<T, N> {
		type Value = Vec<T>;
		fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			write!(f, "at most {N} entries")
		}
		fn visit_seq<A: serde::de::SeqAccess<'de>>(
			self,
			mut seq: A,
		) -> Result<Self::Value, A::Error> {
			let mut values = Vec::new();
			while let Some(value) = seq.next_element()? {
				if values.len() == N {
					return Err(serde::de::Error::custom(format!(
						"array exceeds {N} entries"
					)));
				}
				values.push(value);
			}
			Ok(values)
		}
	}
	de.deserialize_seq(List::<T, N>(std::marker::PhantomData))
}
fn mount_list<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<Mount>, D::Error> {
	bounded::<D, Mount, 256>(de)
}
fn prefix_list<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<String>, D::Error> {
	bounded::<D, String, 16>(de)
}
fn absolute(path: &str) -> Result<PathBuf, String> {
	if path.contains('\0') || !Path::new(path).is_absolute() {
		return Err("source map paths must be absolute and contain no NUL".into());
	}
	Ok(path.into())
}
fn decode(bytes: &[u8], entry: &Path) -> Result<Mounts, String> {
	let document: Document = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
	if document.format != 1 {
		return Err("unsupported source map format (expected 1)".into());
	}
	let expected = absolute(&document.entry)?;
	if !entry.is_absolute() || entry != expected {
		return Err("source map entry does not match the absolute run entry".into());
	}
	if document.mounts.len() > 256 {
		return Err("source map exceeds 256 mounts".into());
	}
	let mounts = document
		.mounts
		.into_iter()
		.map(|m| Ok((m.prefix, absolute(&m.root)?)))
		.collect::<Result<Vec<_>, String>>()?;
	Mounts::new(mounts)
}
pub(super) fn read(path: &Path, entry: &Path) -> Result<Mounts, String> {
	let action = || {
		let mut bytes = Vec::new();
		crate::fs::regular(path)
			.map_err(|e| e.to_string())?
			.take(LIMIT as u64 + 1)
			.read_to_end(&mut bytes)
			.map_err(|e| e.to_string())?;
		if bytes.len() > LIMIT {
			return Err(format!("source map exceeds {LIMIT} bytes"));
		}
		decode(&bytes, entry)
	};
	action().map_err(|e| format!("cannot read source map {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
	use super::*;
	fn doc() -> serde_json::Value {
		let base = std::env::temp_dir();
		serde_json::json!({"format":1,"entry":base.join("main.rn"),"mounts":[{"prefix":["pkg"],"root":base.join("pkg")}]})
	}
	#[test]
	fn strict_shape_entry_and_version() {
		let d = doc();
		let entry = PathBuf::from(d["entry"].as_str().unwrap());
		assert!(decode(&serde_json::to_vec(&d).unwrap(), &entry).is_ok());
		for patch in [
			serde_json::json!({"format":2}),
			serde_json::json!({"entry":"relative"}),
			serde_json::json!({"unknown":true}),
			serde_json::json!({"mounts":null}),
		] {
			let mut bad = d.clone();
			for (k, v) in patch.as_object().unwrap() {
				bad[k] = v.clone();
			}
			assert!(decode(&serde_json::to_vec(&bad).unwrap(), &entry).is_err());
		}
		let bytes = serde_json::to_string(&d)
			.unwrap()
			.replacen("{", "{\"format\":1,", 1);
		assert!(decode(bytes.as_bytes(), &entry).is_err());
		assert!(
			decode(
				&serde_json::to_vec(&d).unwrap(),
				&entry.with_file_name("other.rn")
			)
			.is_err()
		);
	}
	#[test]
	fn byte_bound_is_on_read_including_detection_byte() {
		let path =
			std::env::temp_dir().join(format!("rnx-source-map-bound-{}", std::process::id()));
		let d = doc();
		let entry = PathBuf::from(d["entry"].as_str().unwrap());
		let mut bytes = serde_json::to_vec(&d).unwrap();
		bytes.resize(LIMIT, b' ');
		std::fs::write(&path, &bytes).unwrap();
		assert!(read(&path, &entry).is_ok());
		bytes.push(b' ');
		std::fs::write(&path, &bytes).unwrap();
		assert!(
			read(&path, &entry)
				.err()
				.unwrap()
				.contains("exceeds 16777216 bytes")
		);
		std::fs::remove_file(path).unwrap();
	}
	#[test]
	fn bounded_arrays_duplicates_and_fields() {
		let d = doc();
		let entry = PathBuf::from(d["entry"].as_str().unwrap());
		for n in [256, 257] {
			let mut d = d.clone();
			d["mounts"] = serde_json::Value::Array(
				(0..n)
					.map(
						|i| serde_json::json!({"prefix":[format!("p{i}")],"root":std::env::temp_dir()}),
					)
					.collect(),
			);
			assert_eq!(
				decode(&serde_json::to_vec(&d).unwrap(), &entry).is_ok(),
				n == 256
			);
		}
		for n in [16, 17] {
			let mut d = d.clone();
			d["mounts"][0]["prefix"] = serde_json::json!(vec!["a"; n]);
			assert_eq!(
				decode(&serde_json::to_vec(&d).unwrap(), &entry).is_ok(),
				n == 16
			);
		}
		let mut bad = d.clone();
		bad["mounts"] = serde_json::json!([d["mounts"][0], d["mounts"][0]]);
		assert!(decode(&serde_json::to_vec(&bad).unwrap(), &entry).is_err());
		let mut bad = d;
		bad["mounts"][0]["extra"] = true.into();
		assert!(decode(&serde_json::to_vec(&bad).unwrap(), &entry).is_err());
	}
}
