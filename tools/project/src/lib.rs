//! Internal management implementation; the runner is not a dependency.
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
mod git_inventory;
mod graph;
mod handshake;
mod input;
mod inventory;
mod maintenance;
mod manifest;
mod maps;
mod new_identity;
mod runtime_install;
mod schemas;
mod wire;
mod workflow;

mod entry;
pub use entry::{Coordinates, dispatch, dispatch_stock, is_stock_runner, mark_stock_runner};
#[cfg(test)]
mod tests;
pub fn failure_status() -> i32 {
	if commands::interrupted() {
		commands::signal_status()
	} else {
		1
	}
}
