//! Synthetic document tests pin persisted identity and refusal contracts.
//! Real filesystem/Cargo projections are exercised by the external gate fixture.
use super::*;
use crate::inventory::{Association, External};
fn document() -> Document {
	let base = std::env::temp_dir().join("rnx-identity-shape");
	let native = base.join("native");
	Document {
		format: 1,
		generator: 1,
		context: Context {
			cache_root: base.join("cache"),
			cargo_home: base.join("cargo"),
			rustup_home: None,
			rustup_toolchain: None,
			rustc: "rustc fixture".into(),
			cargo: "cargo fixture".into(),
			target: "fixture-target".into(),
			profile: "release".into(),
			features: vec!["project-sources".into()],
		},
		manifest: format!(
			"[dependencies.rnx]\npath={}\n",
			toml::Value::String(native.to_str().unwrap().into())
		),
		main: "fn main(){rnx::main_with(rnx::Extensions::none())}".into(),
		cargo_lock_sha256: "a".repeat(64),
		native: Inventory {
			platform: "fixture-platform".into(),
			packages: vec![Association {
				id: "fixture-package-id".into(),
				name: "rnx".into(),
				manifest: native.join("Cargo.toml"),
				root: native.clone(),
			}],
			trees: vec![fingerprint::Tree {
				root: native.clone(),
				sha256: "b".repeat(64),
				files: vec![wire::File {
					path: "Cargo.toml".into(),
					bytes: 0,
					executable: false,
					sha256: "c".repeat(64),
				}],
			}],
			external: vec![External {
				path: base.join("config.toml"),
				file: None,
			}],
		},
	}
}
#[test]
fn persisted_identity_is_strict_and_canonical() {
	let good = Identity::from_document(document()).unwrap();
	let again = Identity::decode(good.bytes()).unwrap();
	assert_eq!(good.key(), again.key());
	assert_eq!(good.bytes(), again.bytes());
	let mut value: serde_json::Value = serde_json::from_slice(good.bytes()).unwrap();
	value["unknown"] = true.into();
	assert!(Identity::decode(&serde_json::to_vec(&value).unwrap()).is_err());
	value.as_object_mut().unwrap().remove("unknown");
	value["context"]["unknown"] = true.into();
	assert!(Identity::decode(&serde_json::to_vec(&value).unwrap()).is_err());
	let mut spaced = good.bytes().to_vec();
	spaced.push(b'\n');
	assert!(Identity::decode(&spaced).is_err());
	let duplicate = String::from_utf8(good.bytes().to_vec()).unwrap().replacen(
		"\"format\":1",
		"\"format\":1,\"format\":1",
		1,
	);
	assert!(Identity::decode(duplicate.as_bytes()).is_err());
	for mutate in [
		|d: &mut Document| d.format = 2,
		|d: &mut Document| d.generator = 2,
		|d: &mut Document| d.cargo_lock_sha256 = "Z".repeat(64),
		|d: &mut Document| d.context.features.push("project-sources".into()),
	] {
		let mut d = document();
		mutate(&mut d);
		assert!(Identity::from_document(d).is_err());
	}
}
#[test]
fn build_context_changes_cannot_reuse_a_key() {
	let baseline = Identity::from_document(document()).unwrap();
	let changes: Vec<fn(&mut Document)> = vec![
		|d| d.context.cache_root.push("other"),
		|d| d.context.cargo_home.push("other"),
		|d| d.context.rustup_home = Some(d.context.cargo_home.join("rustup")),
		|d| d.context.rustup_toolchain = Some("fixture-toolchain".into()),
		|d| d.context.rustc.push_str(" changed"),
		|d| d.context.cargo.push_str(" changed"),
		|d| d.context.target = "another-target".into(),
		|d| d.context.profile = "debug".into(),
		|d| d.context.features.push("second-feature".into()),
		|d| d.cargo_lock_sha256 = "d".repeat(64),
		|d| d.main.push_str("\n// different generated registration\n"),
		|d| d.native.trees[0].sha256 = "e".repeat(64),
		|d| {
			d.native.external[0].file = Some(wire::File {
				path: "config.toml".into(),
				bytes: 3,
				executable: false,
				sha256: "f".repeat(64),
			})
		},
	];
	for mutate in changes {
		let mut doc = document();
		mutate(&mut doc);
		assert_ne!(baseline.key(), Identity::from_document(doc).unwrap().key());
	}
}
#[test]
fn inventory_associations_and_bounds_are_not_optional() {
	let mut doc = document();
	doc.native.packages[0].root.push("missing");
	assert!(Identity::from_document(doc).is_err());
	let mut doc = document();
	doc.native.packages[0].name = "not-rnx".into();
	assert!(Identity::from_document(doc).is_err());
	let mut doc = document();
	doc.native.trees.push(doc.native.trees[0].clone());
	assert!(Identity::from_document(doc).is_err());
	let mut doc = document();
	doc.native.trees[0].files[0].path = "../escape".into();
	assert!(Identity::from_document(doc).is_err());
	let mut doc = document();
	doc.native.trees[0].files[0].bytes = fingerprint::BYTES;
	assert!(Identity::from_document(doc.clone()).is_ok());
	doc.native.trees[0].files[0].bytes += 1;
	assert!(Identity::from_document(doc).is_err());
	let mut doc = document();
	doc.main = "x".repeat(input::DOCUMENT_LIMIT);
	assert!(Identity::from_document(doc).is_err());
	assert!(Identity::decode(&vec![b' '; input::DOCUMENT_LIMIT + 1]).is_err());
	let mut doc = document();
	doc.context.cache_root = doc.native.trees[0].root.join("cache");
	assert!(Identity::from_document(doc).is_err());
}
