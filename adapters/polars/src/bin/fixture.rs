//! The generated wrapper's shape, for the adapter's own presentation tests:
//! one Polars extension with its presenter, nothing else. Built only with
//! `test-support`; no project uses it.
fn main() -> Result<(), Box<dyn std::error::Error>> {
	rnx::main_with(
		rnx::Extensions::none()
			.with("polars", rnx_polars::build)
			.present("polars", rnx_polars::present),
	)
}
