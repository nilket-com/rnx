//! The final's arguments, as `final.py` takes them.
use anyhow::{Context as _, Result, bail};
use std::ffi::OsString;
use std::path::PathBuf;

pub const USAGE: &str = "usage: rnx-final --out DIR [--required | --complete | --stages A,B] [--revalidate] [--docs-successor PRIOR] [--abandon-diagnostics] [--bootstrap RECORD]";

#[derive(Debug, Default, PartialEq)]
pub struct Options {
	pub out: PathBuf,
	/// required (the default), complete or explicit.
	pub scope: String,
	pub names: Option<Vec<String>>,
	pub revalidate: bool,
	pub successor: Option<PathBuf>,
	pub abandon: bool,
	pub bootstrap: Option<PathBuf>,
}

pub fn parse(arguments: &[OsString]) -> Result<Options> {
	let mut options = Options::default();
	let mut scope = None;
	let mut out = None;
	let mut iter = arguments.iter();
	while let Some(argument) = iter.next() {
		let mut value = || {
			iter.next()
				.map(PathBuf::from)
				.with_context(|| format!("{argument:?} needs a value"))
		};
		match argument.to_str().unwrap_or_default() {
			"--out" => out = Some(value()?),
			"--required" | "--complete" | "--stages" if scope.is_some() => {
				bail!("one of --required, --complete or --stages")
			}
			"--required" => scope = Some("required"),
			"--complete" => scope = Some("complete"),
			"--stages" => {
				scope = Some("explicit");
				options.names = Some(
					value()?
						.to_string_lossy()
						.split(',')
						.map(str::to_owned)
						.collect(),
				);
			}
			"--revalidate" => options.revalidate = true,
			"--docs-successor" => options.successor = Some(value()?),
			"--abandon-diagnostics" => options.abandon = true,
			"--bootstrap" => options.bootstrap = Some(value()?),
			_ => bail!("{USAGE}"),
		}
	}
	options.out = out.context(USAGE)?;
	options.scope = scope.unwrap_or("required").into();
	Ok(options)
}

/// The receipt a run's last line names.
pub fn receipt(options: &Options) -> &'static str {
	if options.successor.is_some()
		|| (options.revalidate && options.out.join("SUCCESSOR.json").exists())
	{
		"SUCCESSOR.json"
	} else {
		"RESULT.json"
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn args(list: &[&str]) -> Vec<OsString> {
		list.iter().map(OsString::from).collect()
	}

	/// Port of test_default_required_and_scoped_flag_is_explicit.
	#[test]
	fn the_default_is_required_and_a_stage_list_is_explicit() {
		for (flags, names, scope) in [
			(vec![], None, "required"),
			(vec!["--required"], None, "required"),
			(vec!["--complete"], None, "complete"),
			(
				vec!["--stages", "format,clippy"],
				Some(vec!["format".to_string(), "clippy".into()]),
				"explicit",
			),
		] {
			let mut list = vec!["--out", "ignored"];
			list.extend(flags);
			let options = parse(&args(&list)).unwrap();
			assert_eq!((options.names, options.scope.as_str()), (names, scope));
		}
		assert!(parse(&args(&["--out", "x", "--complete", "--required"])).is_err());
		assert!(parse(&args(&["--complete"])).is_err(), "--out is required");
	}

	/// Port of test_successor_prints_its_own_receipt.
	#[test]
	fn a_successor_names_its_own_receipt() {
		assert_eq!(
			receipt(&parse(&args(&["--out", "o", "--docs-successor", "prior"])).unwrap()),
			"SUCCESSOR.json"
		);
		assert_eq!(
			receipt(&parse(&args(&["--out", "o"])).unwrap()),
			"RESULT.json"
		);
	}
}
