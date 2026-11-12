//! Explicit final-owned scratch. Standalone tests retain their existing locations.
use std::path::PathBuf;

pub fn assigned(variable: &str, label: &str) -> Option<PathBuf> {
	std::env::var_os("RNX_FINAL")?;
	let parent = PathBuf::from(std::env::var_os(variable).expect("final output root not assigned"));
	assert!(parent.is_absolute(), "final output root must be absolute");
	assert_eq!(parent.canonicalize().unwrap(), parent, "final root symlink");
	assert!(parent.is_dir(), "final owner must create root");
	assert_eq!(std::path::Path::new(label).file_name().unwrap(), label);
	Some(parent.join(label))
}
