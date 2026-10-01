//! Record 0134: rnx with the working tree's Polars and Candle adapters, the
//! composition a project assembles, for checking the examples before push
//! (`:dep` fetches pushed commits; project builds refuse untracked files).
fn main() -> Result<(), Box<dyn std::error::Error>> {
	rnx::main_with(
		rnx::Extensions::none()
			.with("polars", rnx_polars::build)
			.present("polars", rnx_polars::present)
			.with("candle", rnx_candle::build)
			.present("candle", rnx_candle::present),
	)
}
