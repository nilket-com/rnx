#[path = "../tools/build_identity.rs"]
mod identity;
fn main() {
	let manifest = std::path::PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
	identity::emit(manifest.parent().unwrap());
}
