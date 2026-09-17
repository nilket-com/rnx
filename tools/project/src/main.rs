mod assembly;
mod commands;
mod fingerprint;
mod generate;
mod graph;
mod handshake;
mod input;
mod inventory;
mod manifest;
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
