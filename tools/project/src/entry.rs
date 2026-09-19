//! Executable-specific routing, separate from the runner and persisted identity.
use std::{cell::Cell, ffi::OsString, path::Path};
#[derive(Clone, Copy)]
pub struct Coordinates {
	pub url: &'static str,
	pub revision: &'static str,
	pub state: &'static str,
	pub source_hint: &'static str,
}
thread_local! {static STOCK:Cell<Option<Coordinates>>=const{Cell::new(None)};}
thread_local! {static STOCK_RUNNER: Cell<bool> = const { Cell::new(false) };}
/// Called only by the stock binary before entering the runner; no tool startup.
pub fn mark_stock_runner() {
	STOCK_RUNNER.set(true);
}
pub fn is_stock_runner() -> bool {
	STOCK_RUNNER.get()
}

pub(crate) fn coordinates() -> Option<Coordinates> {
	STOCK.get()
}
pub(crate) fn stock() -> bool {
	coordinates().is_some()
}
pub fn dispatch(args: Vec<OsString>) -> Result<(), String> {
	crate::workflow::cli(args)
}
pub fn dispatch_stock(args: Vec<OsString>, coordinates: Coordinates) -> Result<(), String> {
	struct Restore(Option<Coordinates>);
	impl Drop for Restore {
		fn drop(&mut self) {
			STOCK.set(self.0);
		}
	}
	let _restore = Restore(STOCK.replace(Some(coordinates)));
	dispatch(args)
}
pub(crate) fn quote(path: &Path) -> Result<String, String> {
	let s = path.to_str().ok_or("command path is not Unicode")?;
	if s.contains('\0') {
		return Err("command path contains NUL".into());
	}
	Ok(format!("'{}'", s.replace('\'', "'\\''")))
}
pub(crate) fn project_prefix() -> Result<String, String> {
	let exe = std::env::current_exe().map_err(|e| e.to_string())?;
	Ok(format!(
		"{}{}",
		quote(&exe)?,
		if stock() { " project" } else { "" }
	))
}
pub(crate) fn recovery(manifest: &Path) -> Result<String, String> {
	let prefix = project_prefix()?;
	let manifest = quote(manifest)?;
	Ok(format!(
		"{prefix} lock --manifest {manifest}\n{prefix} build --manifest {manifest}\n{prefix} session --manifest {manifest}"
	))
}
impl Coordinates {
	pub(crate) fn override_help(self) -> String {
		let path = Path::new(self.source_hint);
		let usable = path.is_absolute() && crate::catalogue::supported_runtime(path);
		let (p, label) = if usable {
			(
				path,
				"Build-time checkout suggestion (not selected automatically):",
			)
		} else {
			(
				Path::new("/absolute/path/to/rnx"),
				"Replace this placeholder with a supported rnx checkout:",
			)
		};
		format!(
			"{label}\nexport RNX_DEP_RUNTIME={}",
			quote(p).unwrap_or_default()
		)
	}
	pub(crate) fn notice(self) -> Result<String, String> {
		if !matches!(self.state, "acquired" | "unverified")
			|| self.revision.len() != 40
			|| !self.revision.bytes().all(|b| b.is_ascii_hexdigit())
			|| self.url.is_empty()
		{
			return Err(format!(
				"default runtime coordinates refused: {} {} ({}).\n{}",
				self.url,
				self.revision,
				self.state,
				self.override_help()
			));
		}
		Ok(format!(
			"Runtime: Git {} at {} ({})\nCargo/Rust, Git and native build tools are required.",
			self.url,
			self.revision,
			if self.state == "acquired" {
				"acquired; remote availability may change"
			} else {
				"not yet confirmed reachable"
			}
		))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	#[test]
	fn coordinates_gate_dirty_not_unverified_and_restore_frontend() {
		let clean = Coordinates {
			url: "https://github.com/nilket-com/rnx",
			revision: "1111111111111111111111111111111111111111",
			state: "unverified",
			source_hint: "/does/not/exist",
		};
		assert!(
			clean
				.notice()
				.unwrap()
				.contains("not yet confirmed reachable")
		);
		for state in ["dirty", "unknown"] {
			let c = Coordinates { state, ..clean };
			let e = c.notice().unwrap_err();
			assert!(e.contains("placeholder"));
			assert!(e.contains("export RNX_DEP_RUNTIME='/absolute/path/to/rnx'"));
		}
		assert!(!stock());
		assert!(dispatch_stock(vec!["not-a-command".into()], clean).is_err());
		assert!(!stock());
	}
	#[test]
	fn command_prefix_quotes_the_executable_and_selects_only_the_project_group() {
		assert_eq!(quote(Path::new("/a b/o'ne")).unwrap(), "'/a b/o'\\''ne'");
		assert!(!project_prefix().unwrap().ends_with(" project"));
	}
}
