//! Build-time refusal metadata from the same adapter installations that the
//! package selects. The independently built stock oracle compares the full
//! joined table (active and inactive) in the adapter integration controls.
use rnx::rune::{Context, Module, experimental_core::domain::file::Installer};
use std::io::{self, Write};

fn header() -> Result<(), Box<dyn std::error::Error>> {
	let mut context = Context::with_default_modules()?;
	let mut installer = Installer::new(&mut context);
	installer.install(rnx::interchange::module()?)?;
	let mut polars = Module::with_crate("polars")?;
	rnx_polars::build(&mut polars)?;
	installer.install(polars)?;
	let mut candle = Module::with_crate("candle")?;
	rnx_candle::build(&mut candle)?;
	installer.install(candle)?;
	let files = installer.finish().map_err(|e| format!("{e:?}"))?;
	let rows = files.inventory(&context).map_err(|e| format!("{e:?}"))?;
	write_header(
		rows.iter().map(String::as_str),
		io::BufWriter::new(io::stdout().lock()),
	)
}

fn write_header<'a>(
	rows: impl IntoIterator<Item = &'a str>,
	mut out: impl Write,
) -> Result<(), Box<dyn std::error::Error>> {
	for row in rows {
		let fields: Vec<_> = row.split('\t').collect();
		let [path, shape, receiver, argument, output, error] = fields[..] else {
			return Err("invalid installed file declaration".into());
		};
		let descriptor = match (shape, receiver, argument, output, error) {
			("resolution", "", "", "", "") => (16, 0, 0),
			("session-factory", "", "", "::polars::DataFrame", "") => (14, 0, 1),
			("session-result", "::polars::DataFrame", "", "", "::polars::Error") => (15, 1, 0),
			("session-error-text", "::polars::Error", "", "", "") => (17, 5, 0),
			(
				"csv" | "panic-csv" | "columns" | "convert" | "load" | "forward" | "unary"
				| "integer" | "pair" | "triple" | "text" | "names" | "display",
				..,
			) => continue,
			_ => return Err(format!("unsupported file declaration: {row}").into()),
		};
		let path = path.strip_prefix("::").ok_or("invalid adapter path")?;
		if !path.split("::").all(|part| {
			!part.is_empty() && part.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
		}) {
			return Err(format!("invalid adapter symbol {path:?}").into());
		}
		let (shape, receiver, output) = descriptor;
		writeln!(out, "REFUSAL(\"{path}\",{shape},{receiver},0,{output}),")?;
	}
	out.flush()?;
	Ok(())
}

/// Build helper linked to this exact archive; errors abort package assembly.
#[unsafe(no_mangle)]
pub extern "C" fn rnx_write_file_registration() -> i32 {
	match std::panic::catch_unwind(header) {
		Ok(Ok(())) => 0,
		Ok(Err(e)) => {
			eprintln!("adapter registration: {e}");
			1
		}
		Err(_) => 1,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn registration_emits_refusals_and_skips_enrolled_functions() {
		let mut bytes = Vec::new();
		write_header(
			[
				"::polars::example\tresolution\t\t\t\t",
				"::candle::Tensor::sqr\tunary\t::candle::Tensor\t\t::candle::Tensor\t",
			],
			&mut bytes,
		)
		.unwrap();
		assert_eq!(bytes, b"REFUSAL(\"polars::example\",16,0,0,0),\n");
	}

	#[test]
	fn unsupported_shapes_and_invalid_refusal_names_fail() {
		for row in [
			"::bad\tunknown\t\t\t\t",
			"::bad-name\tresolution\t\t\t\t",
			"missing columns",
		] {
			assert!(write_header([row], Vec::new()).is_err(), "{row}");
		}
	}
}
