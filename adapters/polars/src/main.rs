fn main() -> Result<(), Box<dyn std::error::Error>> {
	rnx::main_with(rnx::Extensions::none().with("polars", rnx_polars::build))
}
