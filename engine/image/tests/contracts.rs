#![cfg(all(target_arch = "x86_64", target_os = "linux", target_env = "gnu"))]

#[test]
fn file_abi_and_bound_launchers() {
	let exe = std::env::current_exe().unwrap();
	// Cargo test builds the feature-selected example cdylib in this profile.
	let image = exe
		.ancestors()
		.nth(2)
		.unwrap()
		.join("examples/libengine_image.so");
	let status = std::process::Command::new("sh")
		.arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/controls.sh"))
		.arg(image)
		.arg(if cfg!(feature = "image-controls") {
			"observed"
		} else {
			"ordinary"
		})
		// Cargo injects its dependency directory; the controls independently
		// exercise both the normal startup search and a competing library.
		.env_remove("LD_LIBRARY_PATH")
		.status()
		.unwrap();
	assert!(status.success(), "{status}");
}
