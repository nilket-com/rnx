mod artifact;
mod assembly;
mod cache_entry;
mod cache_identity;
mod cache_storage;
mod catalogue;
mod commands;
mod dep_wire;
mod fingerprint;
mod generate;
mod graph;
mod handshake;
mod input;
mod inventory;
mod manifest;
mod maps;
mod runtime_install;
mod wire;
mod workflow;
fn main() {
	if let Err(error) = workflow::cli(std::env::args_os().skip(1).collect()) {
		eprintln!("rnx-project: {error}");
		std::process::exit(if commands::interrupted() {
			commands::signal_status()
		} else {
			1
		});
	}
}
