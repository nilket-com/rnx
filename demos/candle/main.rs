fn main() -> Result<(), Box<dyn std::error::Error>> {
	rnx::main_with(
		rnx::Extensions::none()
			.with("polars", rnx_polars::build)
			.present("polars", rnx_polars::present)
			.with("candle", rnx_candle::build)
			.present("candle", rnx_candle::present),
	)
}
