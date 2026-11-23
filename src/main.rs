#[cfg(all(
	feature = "stock-management",
	target_arch = "x86_64",
	target_os = "linux",
	target_env = "gnu"
))]
mod stock_bundle {
	pub struct Stock;
	impl super::installed_file::EmbeddedFileBundle for Stock {
		const IMAGE: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/libengine_image.so"));
		const HELPER: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/stock-file"));
		const GENERATION: &str = concat!(
			include_str!(concat!(env!("OUT_DIR"), "/libengine_image.so.sha256")),
			"-",
			include_str!(concat!(env!("OUT_DIR"), "/stock-file.sha256"))
		);
	}
}
#[cfg(all(
	feature = "stock-management",
	feature = "project-file-bundle",
	target_arch = "x86_64",
	target_os = "linux",
	target_env = "gnu"
))]
use rnx::installed_file;
#[cfg(not(feature = "project-file-bundle"))]
#[cfg(all(
	feature = "stock-management",
	target_arch = "x86_64",
	target_os = "linux",
	target_env = "gnu"
))]
mod installed_file;
#[cfg(feature = "stock-management")]
mod coordinates {
	include!(concat!(env!("OUT_DIR"), "/coordinates.rs"));
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
	#[cfg(feature = "stock-management")]
	{
		let args = std::env::args_os().skip(1).collect::<Vec<_>>();
		let private = std::env::var_os("RNX_INTERNAL_DEP_FD").is_some();
		let group = args.first().and_then(|a| a.to_str());
		let selected = if private {
			Some(args.clone())
		} else {
			match group {
				Some("project") => Some(if args.len() == 1 {
					vec!["help".into()]
				} else {
					args[1..].to_vec()
				}),
				Some("runtime" | "cache" | "management-version") => Some(args.clone()),
				_ => None,
			}
		};
		if let Some(args) = selected {
			if let Err(e) = rnx_project::dispatch_stock(
				args,
				rnx_project::Coordinates {
					url: coordinates::URL,
					revision: coordinates::REV,
					state: coordinates::STATE,
					source_hint: coordinates::SOURCE,
				},
			) {
				eprintln!("rnx: {e}");
				std::process::exit(rnx_project::failure_status());
			}
			return Ok(());
		}
		rnx_project::mark_stock_runner();
	}
	#[cfg(all(
		feature = "stock-management",
		target_arch = "x86_64",
		target_os = "linux",
		target_env = "gnu"
	))]
	{
		rnx::main_with_installed_files(
			installed_file::launch_bundle::<stock_bundle::Stock, false, false>,
		)
	}
	#[cfg(not(all(
		feature = "stock-management",
		target_arch = "x86_64",
		target_os = "linux",
		target_env = "gnu"
	)))]
	{
		rnx::main_with(rnx::Extensions::none())
	}
}
