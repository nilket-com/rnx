//! Weights come from committed input sources. Method spellings remain ambiguous.
use super::{Result, context, sources};
use rune::compile::{CompileVisitor, Located, MetaError, meta::MetaRef};
use rune::{Context, Diagnostics, SourceId, ast, parse::Parser};
use serde_json::{Value as Json, json};
use std::{
	collections::BTreeMap,
	path::{Component, Path},
};
#[derive(Default)]
struct Mentions(BTreeMap<String, usize>);
impl CompileVisitor for Mentions {
	fn visit_meta(
		&mut self,
		_at: &dyn Located,
		m: MetaRef<'_>,
	) -> rune::compile::Result<(), MetaError> {
		if m.context {
			*self.0.entry(format!("{} {}", m.hash, m.item)).or_default() += 1;
		}
		Ok(())
	}
}
#[derive(Default)]
struct Cohort {
	cases: usize,
	legacy_admits: usize,
	execution_admits: usize,
	methods: BTreeMap<String, usize>,
	resolved: BTreeMap<String, usize>,
	lex_errors: usize,
}
fn method_spellings(source: &str) -> Result<BTreeMap<String, usize>> {
	let mut p = Parser::new(source, SourceId::empty(), true);
	let mut tokens = Vec::new();
	while !p.is_eof()? {
		tokens.push(p.parse::<ast::Token>()?);
	}
	let mut counts = BTreeMap::new();
	for (index, pair) in tokens.windows(2).enumerate() {
		if pair[0].kind != ast::Kind::Dot || !matches!(pair[1].kind, ast::Kind::Ident(_)) {
			continue;
		}
		let begin = pair[1].span.range();
		let mut next = index + 2;
		if tokens
			.get(next)
			.is_some_and(|t| t.kind == ast::Kind::ColonColon)
		{
			next += 1;
			let mut depth = 0i32;
			while let Some(token) = tokens.get(next) {
				let text = &source[token.span.range()];
				depth += text.chars().filter(|c| *c == '<').count() as i32;
				depth -= text.chars().filter(|c| *c == '>').count() as i32;
				next += 1;
				if depth <= 0 {
					break;
				}
			}
		}
		if tokens
			.get(next)
			.is_some_and(|t| t.kind == ast::Kind::Open(ast::Delimiter::Parenthesis))
		{
			*counts.entry(source[begin].to_owned()).or_default() += 1;
		}
	}
	Ok(counts)
}
fn add(cohort: &mut Cohort, context: &Context, source: &str, script: bool) -> Result<()> {
	cohort.cases += 1;
	match method_spellings(source) {
		Ok(names) => {
			for (name, n) in names {
				*cohort.methods.entry(name).or_default() += n
			}
		}
		Err(_) => cohort.lex_errors += 1,
	}
	let text = if script {
		format!("pub fn main(){{\n{source}\n}}")
	} else {
		source.to_owned()
	};
	let mut visitor = Mentions::default();
	let mut diagnostics = Diagnostics::new();
	let mut src = sources(&text);
	let legacy = rune::prepare(&mut src)
		.with_context(context)
		.with_diagnostics(&mut diagnostics)
		.with_visitor(&mut visitor)?
		.build();
	cohort.legacy_admits += usize::from(legacy.is_ok());
	for (name, n) in visitor.0 {
		*cohort.resolved.entry(name).or_default() += n;
	}
	cohort.execution_admits += usize::from(super::compile(context, &text).is_ok());
	Ok(())
}
fn child(root: &Path, name: &str) -> Result<std::path::PathBuf> {
	let p = Path::new(name);
	if !p.components().all(|c| matches!(c, Component::Normal(_))) {
		return Err("unsafe inventory input path".into());
	}
	Ok(root.join(p))
}
fn hash(path: &Path) -> Result<String> {
	let out = std::process::Command::new("sha256sum")
		.arg("--")
		.arg(path)
		.output()?;
	if !out.status.success() {
		return Err("cannot hash inventory source".into());
	}
	Ok(std::str::from_utf8(&out.stdout)?[..64].to_owned())
}
fn rn_files(dir: &Path, out: &mut Vec<std::path::PathBuf>) -> Result<()> {
	for e in std::fs::read_dir(dir)? {
		let p = e?.path();
		if p.is_dir() {
			rn_files(&p, out)?;
		} else if p.extension().is_some_and(|x| x == "rn") {
			out.push(p);
		}
	}
	Ok(())
}
pub(super) fn run(root: &Path, rune_root: &Path) -> Result<Json> {
	let standard = Context::with_default_modules()?;
	let (rnx, life, _) = context()?;
	let mut cohorts = BTreeMap::<&str, Cohort>::new();
	let corpus = rune_root.join("tools/xtask/corpus");
	let index = std::fs::read_to_string(corpus.join("l1-cases.tsv"))?;
	let mut lines = index.lines();
	assert_eq!(lines.next(), Some("id\tsource\tmode\tentry\targs\tsha256"));
	for line in lines {
		let cols: Vec<_> = line.split('\t').collect();
		assert_eq!(cols.len(), 6);
		let p = child(&corpus, cols[1])?;
		assert_eq!(hash(&p)?, cols[5]);
		add(
			cohorts.entry("L1").or_default(),
			&standard,
			&std::fs::read_to_string(p)?,
			cols[2] == "script",
		)?;
	}
	let fixtures = root.join("tools/verify/fixtures");
	let manifest: Json =
		serde_json::from_slice(&std::fs::read(fixtures.join("census_manifest.json"))?)?;
	let rows = manifest["rows"].as_array().ok_or("missing census inputs")?;
	assert_eq!(rows.len() as u64, manifest["count"].as_u64().unwrap());
	for row in rows {
		let p = child(&fixtures, row["input"].as_str().unwrap())?;
		assert_eq!(hash(&p)?, row["input_sha256"].as_str().unwrap());
		add(
			cohorts.entry("census").or_default(),
			&rnx,
			&std::fs::read_to_string(p)?,
			false,
		)?;
	}
	let mut demos = Vec::new();
	rn_files(&root.join("demos"), &mut demos)?;
	demos.sort();
	for p in demos {
		add(
			cohorts.entry("demos-base-context").or_default(),
			&rnx,
			&std::fs::read_to_string(p)?,
			false,
		)?;
	}
	let mut notebooks: Vec<_> = std::fs::read_dir(root.join("demos/notebooks"))?
		.map(|e| e.map(|e| e.path()))
		.collect::<std::io::Result<_>>()?;
	notebooks.sort();
	for p in notebooks {
		if !p.extension().is_some_and(|x| x == "ipynb") {
			continue;
		}
		let notebook: Json = serde_json::from_slice(&std::fs::read(&p)?)?;
		for cell in notebook["cells"]
			.as_array()
			.ok_or("notebook has no cells")?
		{
			if cell["cell_type"] != "code" {
				continue;
			}
			let source = cell["source"]
				.as_array()
				.ok_or("notebook source must be a line array")?
				.iter()
				.map(|s| s.as_str().unwrap())
				.collect::<String>();
			if source.trim_start().starts_with(':') {
				continue;
			}
			add(
				cohorts
					.entry("notebook-raw-cells-base-context")
					.or_default(),
				&rnx,
				&source,
				true,
			)?;
		}
	}
	let mut report = BTreeMap::new();
	for (name, c) in cohorts {
		report.insert(name,json!({"source_count":c.cases,"legacy_compiles":c.legacy_admits,"execution_compiles":c.execution_admits,"resolved_context_mentions":c.resolved,"ambiguous_method_spellings":c.methods,"lexer_failures":c.lex_errors,"weight_semantics":"compiler-resolved references and lexical method names are separate; no runtime-frequency or per-overload attribution","context_qualification":"base contexts; missing adapter modules and prior notebook cell bindings are not whole-assembly/session admission results"}));
	}
	life.close()?;
	Ok(json!(report))
}
pub(super) fn self_check() {
	let source = "pub fn main(){let s=\"a.fake()\"; // b.comment()\n [1].iter().collect::<Vec>(); `value ${s.len()}`}";
	let names = method_spellings(source).unwrap();
	assert_eq!(
		names,
		BTreeMap::from([("iter".into(), 1), ("collect".into(), 1), ("len".into(), 1)])
	);
}

#[test]
fn rune_tokens_ignore_comment_and_string_mentions_but_include_generics_and_templates() {
	self_check();
}
