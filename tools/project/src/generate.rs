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
