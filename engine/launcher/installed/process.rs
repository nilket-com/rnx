// The installed helper hands process ownership to the engine image. Compile
// this as an ordinary Rust main so rustc supplies the matching start and exit
// sequence for the image's own copy of std.
unsafe extern "C" {
	fn rnx_installed_body() -> u8;
}

fn main() -> std::process::ExitCode {
	// SAFETY: the image links this function and sets its observer before entry.
	unsafe { rnx_installed_body() }.into()
}
