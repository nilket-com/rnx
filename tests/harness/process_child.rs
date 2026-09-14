use std::io::{Read, Write};
fn main() {
	let args: Vec<_> = std::env::args().collect();
	match args.get(1).map(String::as_str).unwrap_or("cwd") {
		"echo" => {
			let mut b = Vec::new();
			std::io::stdin().read_to_end(&mut b).unwrap();
			std::io::stdout().write_all(&b).unwrap();
		}
		"fail" => {
			eprint!("complaint");
			std::process::exit(7);
		}
		"bad" => {
			std::io::stdout().write_all(b"x\xff").unwrap();
		}
		"big" => {
			std::io::stdout().write_all(&vec![b'x'; 3_000_000]).unwrap();
		}
		"sleep" => {
			std::fs::write("started", b"yes").unwrap();
			std::thread::sleep(std::time::Duration::from_secs(20));
		}
		"env" => {
			for key in &args[2..] {
				println!("{key}={:?}", std::env::var_os(key));
			}
		}
		"build" => print!(
			"{}\n{}",
			std::env::current_dir().unwrap().display(),
			std::env::var("RNX_BUILD_MODE").unwrap()
		),
		"cwd" => print!("{}", std::env::current_dir().unwrap().display()),
		"marker" => std::fs::write("launched", b"yes").unwrap(),
		"identity" => print!("{}", std::env::current_exe().unwrap().display()),
		_ => panic!("unknown fixture"),
	}
}
