//! Pinned, closed module trees keep file admission separate from the single-source census.
use super::{CensusResult, hash_bytes, write_new};
use rune::ast::Spanned;
use rune::compile::{self, SourceLoader};
use rune::experimental_core::{error::Refusal, frontend};
use rune::{Item, Source, SourceId, Sources};
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const ENTRY: &str = include_str!("../../../tools/verify/fixtures/census/module-tree/main.rn");
const MODULE: &str = include_str!("../../../tools/verify/fixtures/census/module-tree/pkg/mod.rn");
const LEAF: &str = include_str!("../../../tools/verify/fixtures/census/module-tree/pkg/leaf.rn");
const REFUSED: &str =
	include_str!("../../../tools/verify/fixtures/census/module-tree/refused-leaf.rn");
const CASES: [&str; 2] = ["nested", "refused-leaf"];

struct Tree(BTreeMap<PathBuf, &'static str>);

impl Tree {
	fn source(&self, path: &Path) -> CensusResult<Source> {
		let text = self
			.0
			.get(path)
			.ok_or("source is outside the pinned module tree")?;
		Ok(Source::with_path(path.to_string_lossy(), text, path)?)
	}
}

impl SourceLoader for Tree {
	fn load(
		&mut self,
		sources: &Sources,
		id: SourceId,
		item: &Item,
		span: &dyn Spanned,
	) -> compile::Result<Source> {
		let Some(mut base) = sources
			.get(id)
			.and_then(Source::path)
			.and_then(Path::parent)
			.map(Path::to_owned)
		else {
			return Err(compile::Error::msg(
				span,
				"module-tree source has no parent",
			));
		};
		for component in item {
			let rune::item::ComponentRef::Str(name) = component else {
				return Err(compile::Error::msg(
					span,
					"module-tree module is not a file path",
				));
			};
			base.push(name);
		}
		for path in [base.join("mod.rn"), base.with_extension("rn")] {
			if self.0.contains_key(&path) {
				return self
					.source(&path)
					.map_err(|e| compile::Error::msg(span, e.to_string()));
			}
		}
		Err(compile::Error::msg(
			span,
			"module load is outside the pinned tree",
		))
	}
}

pub(in crate::verification) fn child(case: &str) -> CensusResult<Value> {
	let (leaf, leaf_pin, refusal) = match case {
		"nested" => (
			LEAF,
			"2c768523bc9142355cad1cd8d6d83ed13bcb2c54bf04ca8705f99c9b980c7d7d",
			None,
		),
		"refused-leaf" => (
			REFUSED,
			"c33f309ac5cc244d77534cb3118d93e736cb280ccb9c5b04ebc47e93db1b88c5",
			Some(Refusal::ByteStr),
		),
		_ => return Err("unknown module-tree census case".into()),
	};
	let prefix = Path::new("module-tree");
	let files = [
		(
			"main.rn",
			ENTRY,
			"fe1d1777856dc5459b5ead286a2d310aca4a1ab5309e86b960a353d1a12807f8",
		),
		(
			"pkg/mod.rn",
			MODULE,
			"947a099001edec78dda75d4e401c86d4012ec2843d5753ad0aaff19cfdf6aed8",
		),
		("pkg/leaf.rn", leaf, leaf_pin),
	];
	let mut tree = Tree(BTreeMap::new());
	for (relative, text, pin) in files {
		if hash_bytes(text.as_bytes())? != pin {
			return Err(format!("module-tree fixture digest differs: {relative}").into());
		}
		tree.0.insert(prefix.join(relative), text);
	}

	let entry = prefix.join("main.rn");
	let context = rune::Context::with_default_modules()?;
	let mut original = Sources::new();
	original.insert(tree.source(&entry)?)?;
	let mut stock_diagnostics = rune::Diagnostics::new();
	rune::prepare(&mut original)
		.with_context(&context)
		.with_diagnostics(&mut stock_diagnostics)
		.with_source_loader(&mut tree)
		.build()
		.map_err(|error| format!("stock rejected {case}: {error} {stock_diagnostics:?}"))?;

	let mut inputs = Sources::new();
	inputs.insert(tree.source(&entry)?)?;
	let mut diagnostics = rune::Diagnostics::new();
	let observed =
		frontend::compile_file_with_loader(&mut inputs, &context, &mut diagnostics, 1, &mut tree);
	let mut retained = Vec::new();
	match (observed, refusal) {
		(Ok((program, _)), None) => {
			if program.sources().len() != files.len()
				|| super::warning_multiset(&stock_diagnostics)
					!= super::warning_multiset(&diagnostics)
			{
				return Err("wrong source count or warning multiset for pinned module tree".into());
			}
			for (id, (relative, text, pin)) in files.into_iter().enumerate() {
				let record = &program.sources()[id];
				if record.id as usize != id
					|| record.name.as_str() != prefix.join(relative).to_string_lossy()
					|| record.text.as_str() != text
				{
					return Err(format!("retained source differs for {relative}").into());
				}
				retained.push(json!({"id": record.id, "path": relative, "sha256": pin}));
			}
		}
		(Err(error), Some(refusal)) if error.refusal == Some(refusal) => {
			let location = error.location.ok_or("module refusal is unlocated")?;
			let source = inputs
				.get(SourceId::new(location.source))
				.ok_or("module refusal source is absent")?;
			if source.path() != Some(prefix.join("pkg/leaf.rn").as_path()) {
				return Err("module refusal is located on the wrong source".into());
			}
		}
		(result, _) => {
			return Err(format!(
				"unexpected module-tree admission in {case}: {:?} {diagnostics:?}",
				result.err()
			)
			.into());
		}
	}
	Ok(
		json!({"status": "PASS", "case": case, "mode": "compile-only", "stock_compiled": true,
		"execution_compiled": refusal.is_none(), "refusal": refusal.map(|r| r.name()), "sources": retained}),
	)
}

pub(super) fn run(executable: &Path, out: &Path) -> CensusResult<Value> {
	let mut rows = Vec::new();
	for case in CASES {
		let child = Command::new("/usr/bin/timeout")
			.args(["--signal=TERM", "--kill-after=5s", "20s"])
			.arg(executable)
			.args(["--child", "census-module-tree", case])
			.env_clear()
			.env("HOME", out)
			.env("PATH", "/usr/bin:/bin")
			.env("LANG", "C.UTF-8")
			.env("TZ", "UTC")
			.current_dir(out)
			.stdin(Stdio::null())
			.output()?;
		write_new(
			&out.join(format!("module-tree-{case}.stdout")),
			&child.stdout,
		)?;
		write_new(
			&out.join(format!("module-tree-{case}.stderr")),
			&child.stderr,
		)?;
		if !child.status.success() || !child.stderr.is_empty() {
			return Err(format!("module-tree census child failed: {case}").into());
		}
		let row: Value = serde_json::from_slice(&child.stdout)?;
		if row["status"] != "PASS" || row["case"] != case || row["mode"] != "compile-only" {
			return Err(format!("module-tree census receipt differs: {case}").into());
		}
		rows.push(row);
	}
	Ok(json!({"status": "PASS", "checks": CASES.len(), "cases": rows}))
}
