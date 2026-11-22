//! Stock-side oracle for the checked file installer. Compilation is enough to
//! observe the original refusal, before the CLI selects a fallback.
mod inventory;
use rnx::rune::{
	Context, Diagnostics, Module, Source, Sources, compile::NoopSourceLoader,
	experimental_core::domain::file::Installer,
};
use std::error::Error;
fn main() -> Result<(), Box<dyn Error>> {
	let path = std::env::args_os().nth(1).ok_or("expected a Rune file")?;
	let mut context = Context::with_default_modules()?;
	let mut installer = Installer::new(&mut context);
	installer.install(rnx::interchange::module()?)?;
	let mut polars = Module::with_crate("polars")?;
	rnx_polars::build(&mut polars)?;
	if path == "--inventory-extra-stock" {
		polars.function("planted_inventory_extra", || ()).build()?;
	}
	installer.install(polars)?;
	let mut candle = Module::with_crate("candle")?;
	rnx_candle::build(&mut candle)?;
	installer.install(candle)?;
	let files = installer.finish().map_err(|e| format!("{e:?}"))?;
	if path == "--inventory" || path == "--refusal-header" || path == "--inventory-extra-stock" {
		return inventory::write(
			files.inventory(&context).map_err(|e| format!("{e:?}"))?,
			path == "--refusal-header",
		);
	}
	let mut sources = Sources::new();
	sources.insert(Source::from_path(path)?)?;
	let result = files.compile(
		&mut sources,
		&context,
		&mut Diagnostics::new(),
		1,
		&mut NoopSourceLoader::default(),
	);
	let e = result.err().ok_or("stock admitted this input")?;
	let why = e.refusal.ok_or_else(|| format!("stock failed: {e:?}"))?;
	match e.location {
		Some(loc) => println!(
			"refused: {} source={} revision={} bytes={}..{}",
			why.name(),
			loc.source,
			loc.revision,
			loc.start,
			loc.end
		),
		None => println!("refused: {} unlocated", why.name()),
	}
	Ok(())
}
