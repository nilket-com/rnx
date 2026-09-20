//! Synthetic document tests pin persisted identity and refusal contracts.
//! Real filesystem/Cargo projections are exercised by the external gate fixture.
use super::*;
use crate::inventory::{Association, External};
pub(super) fn document() -> Document {
	let base = std::env::temp_dir().join("rnx-identity-shape");
	let native = base.join("native");
	Document {
		format: 2,
		generator: 2,
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
			build: None,
		},
		manifest: format!(
			"[dependencies.rnx]\npath={}\n",
			toml::Value::String(native.to_str().unwrap().into())
		),
		main: "fn main(){rnx::main_with(rnx::Extensions::none())}".into(),
		cargo_lock_blake3: "a".repeat(64),
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
				blake3: "b".repeat(64),
				files: vec![wire::File {
					path: "Cargo.toml".into(),
					bytes: 0,
					executable: false,
					blake3: "c".repeat(64),
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
	assert_eq!(good.key(), blake3::hash(good.bytes()).to_hex().as_str());
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
		"\"format\":2",
		"\"format\":2,\"format\":2",
		1,
	);
	assert!(Identity::decode(duplicate.as_bytes()).is_err());
	for mutate in [
		|d: &mut Document| d.format = 99,
		|d: &mut Document| d.generator = 99,
		|d: &mut Document| d.cargo_lock_blake3 = "Z".repeat(64),
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
		|d| d.cargo_lock_blake3 = "d".repeat(64),
		|d| d.main.push_str("\n// different generated registration\n"),
		|d| d.native.trees[0].blake3 = "e".repeat(64),
		|d| {
			d.native.external[0].file = Some(wire::File {
				path: "config.toml".into(),
				bytes: 3,
				executable: false,
				blake3: "f".repeat(64),
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

#[test]
fn build_key_changes_with_each_named_input_and_nothing_else() {
	let base = document().context;
	let key = |c: &Context, s: &[String]| build_key(c, s);
	let reference = key(&base, &[]);
	assert!(valid_digest(&reference));
	assert_eq!(reference, key(&base.clone(), &[]));
	let mut changed = Vec::new();
	for mutate in [
		|c: &mut Context| c.rustc.push('!'),
		|c: &mut Context| c.cargo.push('!'),
		|c: &mut Context| c.target.push('!'),
		|c: &mut Context| c.profile = "debug".into(),
		|c: &mut Context| c.features.push("server-runtime".into()),
		|c: &mut Context| c.cache_root.push("other"),
		|c: &mut Context| c.cargo_home.push("other"),
	] {
		let mut c = base.clone();
		mutate(&mut c);
		changed.push(key(&c, &[]));
	}
	changed.push(key(
		&base,
		&["target.x86_64-unknown-linux-gnu.linker=\"clang\"".into()],
	));
	changed.push(key(
		&base,
		&[
			"target.x86_64-unknown-linux-gnu.linker=\"clang\"".into(),
			"target.x86_64-unknown-linux-gnu.rustflags=[\"-C\",\"link-arg=-fuse-ld=mold\"]".into(),
		],
	));
	let mut unique: Vec<_> = changed.clone();
	unique.push(reference.clone());
	unique.sort();
	unique.dedup();
	assert_eq!(
		unique.len(),
		changed.len() + 1,
		"some input did not change the key"
	);
	// Not inputs: the rustup fields, and the build kind itself.
	let mut c = base.clone();
	c.rustup_home = Some(PathBuf::from("/x"));
	c.rustup_toolchain = Some("stable".into());
	c.build = Some(Build::Private);
	assert_eq!(key(&c, &[]), reference);
	// Framing: two settings cannot masquerade as one.
	assert_ne!(
		key(&base, &["a=1".into(), "b=2".into()]),
		key(&base, &["a=1b=2".into()])
	);
}
#[test]
fn a_path_identity_never_carries_a_build_kind() {
	let mut d = document();
	d.context.build = Some(Build::Private);
	assert!(Identity::from_document(d).is_err());
}

/// Two configuration files as Cargo would find them: one in Cargo home
/// (lowest precedence) and one deeper in the build directory's ancestry.
mod settings_tests_support {
	use super::*;
	pub(super) fn arrangement(tag: &str, home_text: &str, deep_text: &str) -> (Context, Inventory) {
		arrangement_with(tag, home_text, deep_text, None)
	}
	/// `deep_config` is a sibling `config` (no extension) beside the deeper
	/// `config.toml`, which Cargo then reads instead of it.
	pub(super) fn arrangement_with(
		tag: &str,
		home_text: &str,
		deep_text: &str,
		deep_config: Option<&str>,
	) -> (Context, Inventory) {
		let base = std::env::temp_dir().join(format!("rnx-settings-{}-{tag}", std::process::id()));
		let _ = std::fs::remove_dir_all(&base);
		let home = base.join("cargo");
		let deep = base.join("cache").join("build").join(".cargo");
		std::fs::create_dir_all(&home).unwrap();
		std::fs::create_dir_all(&deep).unwrap();
		let mut external = Vec::new();
		let mut files = vec![
			(home.join("config.toml"), home_text),
			(deep.join("config.toml"), deep_text),
		];
		if let Some(text) = deep_config {
			files.push((deep.join("config"), text));
		}
		for (path, text) in files {
			std::fs::write(&path, text).unwrap();
			external.push(External {
				path: path.clone(),
				file: Some(wire::File {
					path: path.file_name().unwrap().to_str().unwrap().into(),
					executable: false,
					bytes: text.len() as u64,
					blake3: blake3::hash(text.as_bytes()).to_hex().to_string(),
				}),
			});
		}
		let mut context = document().context;
		context.cargo_home = home;
		context.cache_root = base.join("cache");
		let mut inventory = document().native;
		inventory.external = external;
		(context, inventory)
	}
	const T: &str = "[target.x86_64-unknown-linux-gnu]\n";
	#[test]
	fn the_effective_linker_follows_precedence_and_swapping_files_changes_the_key() {
		let (c1, i1) = arrangement(
			"a",
			&format!("{T}linker = \"cc\"\n"),
			&format!("{T}linker = \"clang\"\n"),
		);
		let (c2, i2) = arrangement(
			"b",
			&format!("{T}linker = \"clang\"\n"),
			&format!("{T}linker = \"cc\"\n"),
		);
		let s1 = admitted_settings(&c1, &i1).unwrap();
		let s2 = admitted_settings(&c2, &i2).unwrap();
		assert_eq!(s1, vec!["target.x86_64-unknown-linux-gnu.linker=\"clang\""]);
		assert_eq!(s2, vec!["target.x86_64-unknown-linux-gnu.linker=\"cc\""]);
		assert_ne!(build_key(&c1, &s1), build_key(&c2, &s2));
		// The same effective linker from a different arrangement shares the key.
		let (c3, i3) = arrangement("c", "", &format!("{T}linker = \"clang\"\n"));
		let s3 = admitted_settings(&c3, &i3).unwrap();
		assert_eq!(s3, s1);
	}
	#[test]
	fn rustflags_are_joined_in_precedence_order_and_repeats_are_kept() {
		let once = format!("{T}rustflags = [\"-C\", \"link-arg=-fuse-ld=mold\"]\n");
		let (c1, i1) = arrangement("d", &once, "");
		let (c2, i2) = arrangement("e", &once, &once);
		let s1 = admitted_settings(&c1, &i1).unwrap();
		let s2 = admitted_settings(&c2, &i2).unwrap();
		assert_eq!(
			s1,
			vec!["target.x86_64-unknown-linux-gnu.rustflags=[\"-C\",\"link-arg=-fuse-ld=mold\"]"]
		);
		assert_eq!(
			s2,
			vec![
				"target.x86_64-unknown-linux-gnu.rustflags=[\"-C\",\"link-arg=-fuse-ld=mold\",\"-C\",\"link-arg=-fuse-ld=mold\"]"
			]
		);
		assert_ne!(build_key(&c1, &s1), build_key(&c2, &s2));
		// Order of joining: home first, deeper later.
		let (c3, i3) = arrangement(
			"f",
			&format!("{T}rustflags = [\"-C\", \"a\"]\n"),
			&format!("{T}rustflags = [\"-C\", \"b\"]\n"),
		);
		let (c4, i4) = arrangement(
			"g",
			&format!("{T}rustflags = [\"-C\", \"b\"]\n"),
			&format!("{T}rustflags = [\"-C\", \"a\"]\n"),
		);
		assert_eq!(
			admitted_settings(&c3, &i3).unwrap(),
			vec!["target.x86_64-unknown-linux-gnu.rustflags=[\"-C\",\"a\",\"-C\",\"b\"]"]
		);
		assert_ne!(
			admitted_settings(&c3, &i3).unwrap(),
			admitted_settings(&c4, &i4).unwrap()
		);
	}
}

mod masking_tests {
	use super::settings_tests_support::*;
	use super::*;
	const T: &str = "[target.x86_64-unknown-linux-gnu]\n";
	/// The review's counterexample: an empty `config` beside a `config.toml`
	/// naming clang. Cargo reads the empty file, so Cargo home's linker is
	/// the effective one, and changing it must change the key.
	#[test]
	fn a_sibling_config_masks_config_toml_for_the_linker() {
		let (c1, i1) = arrangement_with(
			"h",
			&format!("{T}linker = \"cc\"\n"),
			&format!("{T}linker = \"clang\"\n"),
			Some(""),
		);
		let (c2, i2) = arrangement_with(
			"i",
			&format!("{T}linker = \"clang\"\n"),
			&format!("{T}linker = \"clang\"\n"),
			Some(""),
		);
		let s1 = admitted_settings(&c1, &i1).unwrap();
		let s2 = admitted_settings(&c2, &i2).unwrap();
		assert_eq!(s1, vec!["target.x86_64-unknown-linux-gnu.linker=\"cc\""]);
		assert_eq!(s2, vec!["target.x86_64-unknown-linux-gnu.linker=\"clang\""]);
		assert_ne!(build_key(&c1, &s1), build_key(&c2, &s2));
		// The unmasked sibling still wins when it is the one Cargo reads.
		let (c3, i3) = arrangement_with(
			"j",
			&format!("{T}linker = \"cc\"\n"),
			"",
			Some(&format!("{T}linker = \"clang\"\n")),
		);
		assert_eq!(
			admitted_settings(&c3, &i3).unwrap(),
			vec!["target.x86_64-unknown-linux-gnu.linker=\"clang\""]
		);
	}
	#[test]
	fn a_sibling_config_masks_config_toml_for_rustflags() {
		let flags = format!("{T}rustflags = [\"-C\", \"link-arg=-fuse-ld=mold\"]\n");
		let (c1, i1) = arrangement_with("k", "", &flags, Some(""));
		assert_eq!(admitted_settings(&c1, &i1).unwrap(), Vec::<String>::new());
		let (c2, i2) = arrangement_with("l", &flags, &flags, Some(""));
		assert_eq!(
			admitted_settings(&c2, &i2).unwrap(),
			vec!["target.x86_64-unknown-linux-gnu.rustflags=[\"-C\",\"link-arg=-fuse-ld=mold\"]"]
		);
	}
}
