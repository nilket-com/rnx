#[path = "owned_scratch.rs"]
mod owned_scratch;
pub use owned_scratch::Scratch;

#[cfg(test)]
use std::io::ErrorKind;
#[test]
fn a_colliding_scratch_directory_is_never_touched() {
	// The control for ownership: a directory already at the scratch name,
	// planted with a sentinel, survives both the acquisition and the drop.
	// The plant lives inside an outer directory this test creates atomically
	// itself (not through the helper under test), so nothing outside it is
	// ever removed.
	let mut outer = None;
	for n in 0..1000u32 {
		let candidate =
			std::env::temp_dir().join(format!("rnx-scratch-collision-{}-{n}", std::process::id()));
		match std::fs::create_dir(&candidate) {
			Ok(()) => {
				outer = Some(candidate);
				break;
			}
			Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
			Err(e) => panic!("cannot create {}: {e}", candidate.display()),
		}
	}
	let outer = outer.expect("no free collision directory");
	let base = outer.join("scratch");
	std::fs::create_dir(&base).unwrap();
	std::fs::write(base.join("sentinel"), b"kept").unwrap();
	let owned = {
		let scratch = Scratch::dir(base.clone());
		assert_ne!(&*scratch, base.as_path());
		assert!(scratch.exists());
		scratch.to_path_buf()
	};
	assert!(!owned.exists(), "the guard's own directory was not removed");
	let mut unwound = None;
	assert!(
		std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
			let scratch = Scratch::dir(base.clone());
			std::fs::write(scratch.join("partial"), "test").unwrap();
			unwound = Some(scratch.to_path_buf());
			panic!("planted failure");
		}))
		.is_err()
	);
	assert!(!unwound.unwrap().exists());
	assert_eq!(std::fs::read(base.join("sentinel")).unwrap(), b"kept");
	std::fs::remove_dir_all(&outer).unwrap();
}
