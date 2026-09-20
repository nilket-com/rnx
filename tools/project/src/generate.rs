#![allow(dead_code)]
use crate::manifest::{Hook, Manifest};
use serde::Serialize;
use std::collections::BTreeMap;
use std::path::Path;
#[derive(Serialize)]
struct Cargo {
	package: Package,
	workspace: BTreeMap<String, String>,
	dependencies: BTreeMap<String, Dependency>,
}
#[derive(Serialize)]
struct Package {
	name: &'static str,
	version: &'static str,
	edition: &'static str,
	publish: bool,
}
#[derive(Serialize)]
struct Dependency {
	#[serde(skip_serializing_if = "Option::is_none")]
	package: Option<String>,
	path: String,
	#[serde(rename = "default-features")]
	#[serde(skip_serializing_if = "Option::is_none")]
	default_features: Option<bool>,
	#[serde(skip_serializing_if = "Vec::is_empty")]
	features: Vec<String>,
}
pub(crate) fn wrapper(manifest: &Manifest, base: &Path) -> Result<(String, String), String> {
	wrapper_style(manifest, base, false)
}
fn wrapper_style(
	manifest: &Manifest,
	base: &Path,
	legacy: bool,
) -> Result<(String, String), String> {
	manifest.validate()?;
	let runtime = manifest
		.runtime
		.as_ref()
		.ok_or("an executable override has no generated wrapper")?;
	let path = |p: &str| -> Result<String, String> {
		base.join(p)
			.to_str()
			.map(String::from)
			.ok_or("generated Cargo path is not Unicode".into())
	};
	let mut dependencies = BTreeMap::from([(
		"rnx".into(),
		Dependency {
			package: None,
			default_features: (!legacy).then_some(false),
			path: path(&runtime.path)?,
			features: if legacy {
				vec!["project-sources".into()]
			} else {
				vec!["count-allocations".into(), "project-sources".into()]
			},
		},
	)]);
	let mut main = "fn main() -> Result<(), Box<dyn std::error::Error>> {\n    rnx::main_with(rnx::Extensions::none()".to_owned();
	for (index, (name, native)) in manifest.native.iter().enumerate() {
		let alias = format!("native_{index}");
		dependencies.insert(
			alias.clone(),
			Dependency {
				package: Some(native.package.clone()),
				default_features: (!legacy).then_some(true),
				path: path(&native.path)?,
				features: vec![],
			},
		);
		let hook = match native.hook {
			Hook::Plain => "with",
			Hook::Lifecycle => "with_lifecycle",
		};
		main += &format!(".{hook}({name:?}, {alias}::{})", native.builder);
		if native.presentation {
			main += &format!(".present({name:?}, {alias}::present)");
		}
	}
	main += ")\n}\n";
	let cargo = Cargo {
		package: Package {
			name: PLACEHOLDER,
			version: "0.0.0",
			edition: "2024",
			publish: false,
		},
		workspace: BTreeMap::new(),
		dependencies,
	};
	Ok((toml::to_string(&cargo).map_err(|e| e.to_string())?, main))
}

/// Shared assembly generation resolves native locations without changing the
/// legacy per-project wrapper or normalizing paths inside native source bytes.
pub(crate) fn canonical_wrapper(
	manifest: &Manifest,
	base: &Path,
) -> Result<(String, String), String> {
	canonical_style(manifest, base, false)
}
fn canonical_style(
	manifest: &Manifest,
	base: &Path,
	legacy: bool,
) -> Result<(String, String), String> {
	let mut manifest = manifest.clone();
	let canonical = |path: &str| -> Result<String, String> {
		base.join(path)
			.canonicalize()
			.map_err(|e| format!("native path {path}: {e}"))?
			.into_os_string()
			.into_string()
			.map_err(|_| "native path is not Unicode".into())
	};
	if let Some(runtime) = &mut manifest.runtime {
		runtime.path = canonical(&runtime.path)?;
	}
	for native in manifest.native.values_mut() {
		native.path = canonical(&native.path)?;
	}
	wrapper_style(&manifest, base, legacy)
}

/// Version-three generator: Cargo owns Git acquisition; only explicit path
/// declarations are canonicalised on the host.
pub(crate) fn git_wrapper(
	declaration: &crate::schemas::Declaration,
	base: &Path,
) -> Result<(String, String), String> {
	let (manifest, main) = git_wrapper_retained(declaration, base)?;
	// Generator four: the package carries a digest of the wrapper computed
	// while it still bears the placeholder name, so two wrappers that differ
	// in any way are two Cargo units, whatever directory builds them.
	let name = wrapper_name(&manifest, &main);
	let named = manifest.replacen(
		&format!("name = {PLACEHOLDER:?}"),
		&format!("name = {name:?}"),
		1,
	);
	if !named.contains(&format!("name = {name:?}")) {
		return Err("generated wrapper has no package name to replace".into());
	}
	Ok((named, main))
}

/// Generator three's exact recipe: the placeholder-named wrapper. Retained
/// format-three identities are verified and rebuilt against this, never
/// against the digest-named form they predate.
pub(crate) fn git_wrapper_retained(
	declaration: &crate::schemas::Declaration,
	base: &Path,
) -> Result<(String, String), String> {
	declaration.validate()?;
	let runtime = declaration
		.runtime
		.as_ref()
		.ok_or("override has no wrapper")?;
	let mut projection = declaration.graph_manifest();
	projection.executable = None;
	projection.runtime = Some(crate::manifest::Location {
		path: runtime
			.path
			.clone()
			.unwrap_or_else(|| "/unused-git-coordinate".into()),
	});
	projection.native = declaration
		.native
		.iter()
		.map(|(name, n)| {
			(
				name.clone(),
				crate::manifest::Native {
					path: n
						.path
						.clone()
						.unwrap_or_else(|| "/unused-git-coordinate".into()),
					package: n.package.clone(),
					builder: n.builder.clone(),
					hook: n.hook,
					presentation: n.presentation,
					shared_build: n.shared_build,
				},
			)
		})
		.collect();
	if let Some(p) = &runtime.path {
		projection.runtime.as_mut().unwrap().path = base
			.join(p)
			.canonicalize()
			.map_err(|e| e.to_string())?
			.to_str()
			.ok_or("non-Unicode runtime")?
			.into();
	}
	for (name, n) in &declaration.native {
		if let Some(p) = &n.path {
			projection.native.get_mut(name).unwrap().path = base
				.join(p)
				.canonicalize()
				.map_err(|e| e.to_string())?
				.to_str()
				.ok_or("non-Unicode native")?
				.into();
		}
	}
	let (manifest, main) = wrapper(&projection, base)?;
	let mut doc: toml::Value = toml::from_str(&manifest).map_err(|e| e.to_string())?;
	let deps = doc
		.get_mut("dependencies")
		.and_then(toml::Value::as_table_mut)
		.ok_or("missing generated dependencies")?;
	let mut replace = |name: &str, url: &Option<String>, rev: &Option<String>| {
		if let Some(url) = url {
			let dep = deps.get_mut(name).unwrap().as_table_mut().unwrap();
			dep.remove("path");
			dep.insert("git".into(), url.clone().into());
			dep.insert("rev".into(), rev.clone().unwrap().into());
		}
	};
	replace("rnx", &runtime.git, &runtime.rev);
	for (i, n) in declaration.native.values().enumerate() {
		replace(&format!("native_{i}"), &n.git, &n.rev);
	}
	Ok((toml::to_string(&doc).map_err(|e| e.to_string())?, main))
}
/// The wrapper an identity of the given generator recorded, regenerated
/// from the declarations: three keeps the placeholder name, four the digest.
pub(crate) fn git_wrapper_for(
	generator: u32,
	declaration: &crate::schemas::Declaration,
	base: &Path,
) -> Result<(String, String), String> {
	match generator {
		3 => git_wrapper_retained(declaration, base),
		4 => git_wrapper(declaration, base),
		_ => Err("unsupported wrapper generator".into()),
	}
}

pub(crate) const PLACEHOLDER: &str = "rnx-project-app";
/// The wrapper's own name: BLAKE3 over a framed preimage of the generated
/// manifest (placeholder name) and `main`, with a domain tag, so neither the
/// name nor the assembly key that later hashes the named manifest is an
/// input. Equal wrapper content yields the same name, which is the same
/// unit; a one-byte difference yields a different one.
pub(crate) fn wrapper_name(manifest: &str, main: &str) -> String {
	let mut hasher = blake3::Hasher::new();
	hasher.update(b"rnx-wrapper-name-1\n");
	for part in [manifest, main] {
		hasher.update(&(part.len() as u64).to_le_bytes());
		hasher.update(part.as_bytes());
	}
	format!("rnx-app-{}", hasher.finalize().to_hex())
}
/// The executable a generated wrapper produces, from its manifest.
pub(crate) fn executable_name(manifest: &str) -> Result<String, String> {
	let doc: toml::Value = toml::from_str(manifest).map_err(|e| e.to_string())?;
	doc.get("package")
		.and_then(|p| p.get("name"))
		.and_then(|n| n.as_str())
		.map(str::to_owned)
		.ok_or_else(|| "generated wrapper has no package name".into())
}

/// Generator two predates the stock-management feature. Its exact historical
/// wrapper remains a valid recipe only for retained identity-two envelopes.
pub(crate) fn retained_wrapper(
	manifest: &Manifest,
	base: &Path,
	expected: (&str, &str),
) -> Result<bool, String> {
	for legacy in [false, true] {
		let (cargo, main) = canonical_style(manifest, base, legacy)?;
		if expected == (cargo.as_str(), main.as_str()) {
			return Ok(true);
		}
	}
	Ok(false)
}

#[cfg(test)]
mod name_tests {
	use super::*;
	#[test]
	fn wrapper_name_is_a_full_digest_over_framed_content() {
		let a = wrapper_name("[package]\nname = \"rnx-project-app\"\n", "fn main() {}\n");
		assert!(a.starts_with("rnx-app-") && a.len() == "rnx-app-".len() + 64);
		assert_eq!(
			a,
			wrapper_name("[package]\nname = \"rnx-project-app\"\n", "fn main() {}\n")
		);
		assert_ne!(
			a,
			wrapper_name("[package]\nname = \"rnx-project-app\"\n", "fn main() { }\n")
		);
		assert_ne!(
			a,
			wrapper_name(
				"[package]\nname = \"rnx-project-app\"\n\n",
				"fn main() {}\n"
			)
		);
		// Framing: moving a byte across the boundary changes the name.
		assert_ne!(wrapper_name("ab", "c"), wrapper_name("a", "bc"));
		assert_ne!(wrapper_name("", "abc"), wrapper_name("abc", ""));
	}
	#[test]
	fn each_generator_regenerates_its_own_recipe() {
		let d: crate::schemas::Declaration = serde_json::from_value(serde_json::json!({
			"format": 2, "application": {"entry": "main.rn"},
			"runtime": {"git": "https://example.invalid/rnx", "rev": "0123456789abcdef0123456789abcdef01234567"},
		}))
		.unwrap();
		let base = std::env::temp_dir();
		let (three, main3) = git_wrapper_for(3, &d, &base).unwrap();
		let (four, main4) = git_wrapper_for(4, &d, &base).unwrap();
		assert_eq!(main3, main4);
		assert_eq!(executable_name(&three).unwrap(), PLACEHOLDER);
		assert_eq!(
			executable_name(&four).unwrap(),
			wrapper_name(&three, &main3)
		);
		assert_eq!(
			three.replacen(PLACEHOLDER, &executable_name(&four).unwrap(), 1),
			four
		);
		assert!(git_wrapper_for(2, &d, &base).is_err());
	}
	#[test]
	fn executable_name_reads_the_package() {
		assert_eq!(
			executable_name("[package]\nname = \"rnx-app-00\"\n").unwrap(),
			"rnx-app-00"
		);
		assert!(executable_name("[package]\nversion = \"0.0.0\"\n").is_err());
	}
}
