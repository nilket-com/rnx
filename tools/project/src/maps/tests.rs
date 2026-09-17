use super::*;
use std::{
	cell::Cell,
	path::PathBuf,
	sync::atomic::{AtomicUsize, Ordering},
};
struct Temp(PathBuf);
impl Temp {
	fn new() -> Self {
		static N: AtomicUsize = AtomicUsize::new(0);
		let p = std::env::temp_dir().join(format!(
			"rnx-map-{}-{}",
			std::process::id(),
			N.fetch_add(1, Ordering::Relaxed)
		));
		fs::create_dir(&p).unwrap();
		Self(p)
	}
}
impl Drop for Temp {
	fn drop(&mut self) {
		fs::remove_dir_all(&self.0).unwrap();
	}
}
#[test]
fn identical_reuses_missing_and_different_publish_and_failure_survives() {
	let t = Temp::new();
	let path = t.0.join("map.json");
	let count = Cell::new(0);
	let publish = || {
		count.set(count.get() + 1);
		fs::write(&path, b"canonical").map_err(|e| e.to_string())
	};
	ensure(&path, b"canonical", publish).unwrap();
	assert_eq!(count.get(), 1);
	let modified = fs::metadata(&path).unwrap().modified().unwrap();
	ensure(&path, b"canonical", publish).unwrap();
	assert_eq!(count.get(), 1);
	assert_eq!(modified, fs::metadata(&path).unwrap().modified().unwrap());
	// The address is not trusted: changed bytes under the same name are repaired.
	fs::write(&path, b"corruption").unwrap();
	assert_eq!(
		ensure(&path, b"canonical", || Err("publish failed".into())).unwrap_err(),
		"publish failed"
	);
	assert_eq!(fs::read(&path).unwrap(), b"corruption");
	ensure(&path, b"canonical", publish).unwrap();
	assert_eq!(count.get(), 2);
	assert_eq!(fs::read(&path).unwrap(), b"canonical");
}
#[test]
fn oversized_and_non_regular_maps_never_publish() {
	let t = Temp::new();
	let path = t.0.join("map.json");
	fs::File::create(&path)
		.unwrap()
		.set_len(input::DOCUMENT_LIMIT as u64 + 1)
		.unwrap();
	assert!(
		ensure(&path, b"canonical", || panic!("must not publish"))
			.unwrap_err()
			.contains("exceeds")
	);
	fs::remove_file(&path).unwrap();
	fs::create_dir(&path).unwrap();
	assert!(
		ensure(&path, b"canonical", || panic!("must not publish"))
			.unwrap_err()
			.contains("regular")
	);
}
#[cfg(unix)]
#[test]
fn symlink_and_fifo_never_publish_or_block() {
	use std::os::unix::fs::symlink;
	let t = Temp::new();
	let path = t.0.join("map.json");
	let target = t.0.join("target");
	fs::write(&target, b"canonical").unwrap();
	symlink(&target, &path).unwrap();
	assert!(ensure(&path, b"canonical", || panic!("must not publish")).is_err());
	assert_eq!(fs::read(&target).unwrap(), b"canonical");
	fs::remove_file(&path).unwrap();
	assert!(
		std::process::Command::new("mkfifo")
			.arg(&path)
			.status()
			.unwrap()
			.success()
	);
	assert!(ensure(&path, b"canonical", || panic!("must not publish")).is_err());
}

#[cfg(unix)]
#[test]
fn unreadable_map_refuses_without_publication() {
	use std::os::unix::fs::PermissionsExt;
	// Root bypasses mode permission checks; not evidence of readable user files.
	if unsafe { libc::geteuid() } == 0 {
		return;
	}
	let t = Temp::new();
	let path = t.0.join("map.json");
	fs::write(&path, b"canonical").unwrap();
	fs::set_permissions(&path, fs::Permissions::from_mode(0o0)).unwrap();
	assert!(ensure(&path, b"canonical", || panic!("must not publish")).is_err());
}
