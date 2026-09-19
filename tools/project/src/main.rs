fn main() {
	if let Err(error) = rnx_project::dispatch(std::env::args_os().skip(1).collect()) {
		eprintln!("rnx-project: {error}");
		std::process::exit(rnx_project::failure_status());
	}
}
