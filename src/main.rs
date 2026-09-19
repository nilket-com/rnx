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
	rnx::main_with(rnx::Extensions::none())
}
