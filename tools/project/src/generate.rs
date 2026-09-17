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
	#[serde(skip_serializing_if = "Vec::is_empty")]
	features: Vec<String>,
}
pub(crate) fn wrapper(manifest: &Manifest, base: &Path) -> Result<(String, String), String> {
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
			path: path(&runtime.path)?,
			features: vec!["project-sources".into()],
		},
	)]);
	let mut main = "fn main() -> Result<(), Box<dyn std::error::Error>> {\n    rnx::main_with(rnx::Extensions::none()".to_owned();
	for (index, (name, native)) in manifest.native.iter().enumerate() {
		let alias = format!("native_{index}");
		dependencies.insert(
			alias.clone(),
			Dependency {
				package: Some(native.package.clone()),
				path: path(&native.path)?,
				features: vec![],
			},
		);
		let hook = match native.hook {
			Hook::Plain => "with",
			Hook::Lifecycle => "with_lifecycle",
		};
		main += &format!(".{hook}({name:?}, {alias}::{})", native.builder);
	}
	main += ")\n}\n";
	let cargo = Cargo {
		package: Package {
			name: "rnx-project-app",
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
	wrapper(&manifest, base)
}
