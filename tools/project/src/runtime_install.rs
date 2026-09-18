//! Retained source installation; tool-owned session discovery.
#![allow(dead_code)] // Also compiled in the private library for checks.
#[cfg(unix)]
mod git;
#[cfg(unix)]
mod hooks;
#[cfg(unix)]
mod layout;
#[cfg(unix)]
mod storage;
#[cfg(unix)]
mod unix;
#[cfg(unix)]
#[allow(unused_imports)]
pub(crate) use unix::{Selection, cli};
#[cfg(not(unix))]
pub(crate) fn cli(_: &[std::ffi::OsString]) -> Result<(), String> {
	Err("runtime installation commands require Unix in this release".into())
}

#[cfg(not(unix))]
pub(crate) struct Selection {
	pub source: std::path::PathBuf,
	pub notice: String,
}
#[cfg(not(unix))]
impl Selection {
	pub(crate) fn discover() -> Result<Self, String> {
		Err("dependency transitions require Unix supervision".into())
	}
	pub(crate) fn validate(&self) -> Result<(), String> {
		Err("dependency transitions require Unix supervision".into())
	}
}
