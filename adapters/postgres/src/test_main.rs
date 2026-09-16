fn main() -> Result<(), Box<dyn std::error::Error>> {
	rnx_postgres::testing::enable_pause();
	rnx::main_with(rnx::Extensions::none().with_lifecycle("postgres", rnx_postgres::build))
}
