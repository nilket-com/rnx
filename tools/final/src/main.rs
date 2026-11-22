//! rnx's final controller: `cargo final`-style entry via tools/final/final.sh.
use anyhow::{Result, bail};
use rnx_final::controller::{self, Settings};
use std::ffi::OsString;
use std::path::{Path, PathBuf};

fn root() -> PathBuf {
	Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.expect("the controller's checkout")
}

fn final_main(arguments: &[OsString]) -> Result<i32> {
	let options = rnx_final::cli::parse(arguments)?;
	let (out, scope, names, revalidate, successor, abandon, bootstrap) = (
		options.out.clone(),
		options.scope.as_str(),
		options.names.clone(),
		options.revalidate,
		options.successor.clone(),
		options.abandon,
		options.bootstrap.clone(),
	);
	let root = root();
	let out = if out.is_absolute() {
		out
	} else {
		std::env::current_dir()?.join(out)
	};
	let result = if abandon {
		rnx_final::evidence::abandon_diagnostics(&out)?;
		serde_json::json!({"status": "PASS"})
	} else if revalidate {
		controller::revalidate_any(&out)?
	} else {
		let (record, started) = match &bootstrap {
			Some(path) => {
				let (record, started) = rnx_final::bootstrap::accept(path, &root)?;
				(Some(record), started)
			}
			None => (None, std::time::Instant::now()),
		};
		let complete = scope == "complete" && successor.is_none();
		if complete && record.is_none() {
			bail!(
				"a complete final starts from tools/final/final.sh, which builds and hands over the controller"
			);
		}
		let unit = if complete {
			Some(rnx_final::guardian::own_unit()?)
		} else {
			None
		};
		let settings = Settings {
			root: root.clone(),
			out: out.clone(),
			names,
			scope: scope.into(),
			fresh: false,
			environment: std::env::vars().collect(),
			guardian: PathBuf::from("/proc/self/exe"),
			started,
			bootstrap: record,
			unit,
			hooks: std::sync::Arc::new(controller::Live),
			tokens: None,
		};
		match &successor {
			Some(prior) => controller::successor(&settings, prior)?,
			None => controller::run(&settings)?,
		}
	};
	let receipt = rnx_final::cli::receipt(&options);
	println!(
		"{} {} {}",
		result["status"].as_str().unwrap_or("FAIL"),
		result["acceptance_scope"].as_str().unwrap_or(""),
		out.join(receipt).display()
	);
	Ok(if result["status"] == "PASS" { 0 } else { 1 })
}

fn main() {
	let arguments: Vec<OsString> = std::env::args_os().skip(1).collect();
	let result = match arguments.first().and_then(|a| a.to_str()) {
		Some("cached-cargo") => rnx_final::cache::cargo_main(&arguments[1..]),
		Some("guardian") => rnx_final::guardian::main(&arguments[1..]),
		Some("stage") => rnx_final::stage::main(&arguments[1..], &root()),
		// Never resolved: a copied controller's build checkout may be gone.
		Some("jobs") => rnx_final::jobs::main(
			&arguments[1..],
			&Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
		),
		Some("python-list") => {
			rnx_final::pylist::check(&root(), rnx_final::pylist::BASELINE).map(|r| {
				println!("{r}");
				0
			})
		}
		Some("compare")
			if arguments.len() == 3 || (arguments.len() == 5 && arguments[3] == "--witness") =>
		{
			rnx_final::compare::compare(
				Path::new(&arguments[1]),
				Path::new(&arguments[2]),
				arguments.get(4).map(Path::new),
			)
			.map(|r| {
				println!("{}", serde_json::to_string_pretty(&r).unwrap_or_default());
				if r["status"] == "PASS" { 0 } else { 1 }
			})
		}
		_ => final_main(&arguments),
	};
	match result {
		Ok(status) => std::process::exit(status),
		Err(error) => {
			eprintln!("rnx-final: {error:#}");
			std::process::exit(2);
		}
	}
}
