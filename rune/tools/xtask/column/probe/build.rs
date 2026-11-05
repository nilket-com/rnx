fn main() {
	let include = std::env::var("VALGRIND_INCLUDE")
		.unwrap_or_else(|_| "/home/me/opt/valgrind/include".into());
	cc::Build::new()
		.file("measure.c")
		.include(include)
		.compile("ck1_measure");
	println!("cargo:rerun-if-changed=measure.c");
}
