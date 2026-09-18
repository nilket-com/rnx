//! Retained source installation; session discovery is integrated in gate three.
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
pub(crate) use unix::cli;
#[cfg(not(unix))]
pub(crate) fn cli(_: &[std::ffi::OsString]) -> Result<(), String> {
	Err("runtime installation commands require Unix in this release".into())
}
