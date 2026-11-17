//! Python stops growing: every tracked `.py` is on a committed list with
//! its disposition, and the list may only lose entries. Every Python test
//! file also names its route, the entry point that runs it, which the
//! check holds to that entry point.
use crate::child::{git_command, value_error};
use anyhow::{Context as _, Result, bail};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

pub const LIST: &str = "tools/python-files.txt";
/// The published commit whose tracked Python files the first list holds.
pub const BASELINE: &str = "515e5211ba38ceb7506c82bd844df3e80e686a8b";
const DISPOSITIONS: [&str; 3] = ["port", "delete", "jupyter"];

fn git(root: &Path, args: &[&str]) -> Result<Option<Vec<u8>>> {
	let output = git_command().arg("-C").arg(root).args(args).output()?;
	Ok(output.status.success().then_some(output.stdout))
}

fn paths(bytes: &[u8]) -> Result<BTreeSet<String>> {
	bytes
		.split(|b| *b == 0 || *b == b'\n')
		.filter(|p| !p.is_empty())
		.map(|p| {
			Ok(std::str::from_utf8(p)
				.context("a path is not UTF-8")?
				.to_owned())
		})
		.collect()
}

/// path -> disposition, refusing duplicates and unknown dispositions.
pub fn parse(text: &str) -> Result<BTreeMap<String, String>> {
	Ok(rows(text)?
		.into_iter()
		.map(|(path, (disposition, _))| (path, disposition))
		.collect())
}

/// path -> (disposition, route), the route a third column when present.
pub fn rows(text: &str) -> Result<BTreeMap<String, (String, Option<String>)>> {
	let mut rows = BTreeMap::new();
	for line in text
		.lines()
		.filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
	{
		let mut fields = line.splitn(3, '\t');
		let (Some(path), Some(disposition)) = (fields.next(), fields.next()) else {
			bail!(value_error(&format!("malformed list line: {line}")))
		};
		let route = fields.next().map(str::to_owned);
		if !DISPOSITIONS.contains(&disposition) {
			bail!(value_error(&format!(
				"unknown disposition for {path}: {disposition}"
			)));
		}
		if rows
			.insert(path.to_owned(), (disposition.to_owned(), route))
			.is_some()
		{
			bail!(value_error(&format!("listed twice: {path}")));
		}
	}
	Ok(rows)
}

/// A Python test file: `test_*.py`.
pub fn is_test(path: &str) -> bool {
	Path::new(path)
		.file_name()
		.and_then(|n| n.to_str())
		.is_some_and(|n| n.starts_with("test_") && n.ends_with(".py"))
}

/// The list the change started from: the nearest first-parent ancestor
/// that has one, or, before any had, the baseline's tracked Python files.
fn parent_list(root: &Path, baseline: &str) -> Result<BTreeSet<String>> {
	let ancestors = git(
		root,
		&["rev-list", "--first-parent", "--max-count=64", "HEAD^"],
	)?
	.unwrap_or_default();
	for commit in String::from_utf8(ancestors)?.lines() {
		if let Some(text) = git(root, &["show", &format!("{commit}:{LIST}")])? {
			return Ok(parse(std::str::from_utf8(&text)?)?.into_keys().collect());
		}
		if commit == baseline {
			break;
		}
	}
	let tracked = git(root, &["ls-tree", "-r", "-z", "--name-only", baseline])?
		.context("the baseline commit is not in this checkout")?;
	Ok(paths(&tracked)?
		.into_iter()
		.filter(|p| p.ends_with(".py"))
		.collect())
}

pub fn check(root: &Path, baseline: &str) -> Result<Value> {
	let text = std::fs::read_to_string(root.join(LIST))
		.map_err(|_| value_error(&format!("{LIST} is missing")))?;
	let all = rows(&text)?;
	let listed: BTreeMap<String, String> = all
		.iter()
		.map(|(p, (d, _))| (p.clone(), d.clone()))
		.collect();
	let tracked: BTreeSet<String> =
		paths(&git(root, &["ls-files", "-z", "*.py"])?.context("git ls-files failed")?)?;
	let names: BTreeSet<String> = listed.keys().cloned().collect();
	let missing: Vec<&String> = tracked.difference(&names).collect();
	if !missing.is_empty() {
		bail!(value_error(&format!(
			"tracked Python files not listed in {LIST}: {}",
			join(&missing)
		)));
	}
	let stale: Vec<&String> = names.difference(&tracked).collect();
	if !stale.is_empty() {
		bail!(value_error(&format!(
			"listed Python files that are not tracked: {}",
			join(&stale)
		)));
	}
	// Path by path, so a renamed file counts as an addition.
	let parent = parent_list(root, baseline)?;
	let added: Vec<&String> = names.difference(&parent).collect();
	if !added.is_empty() {
		bail!(value_error(&format!(
			"the Python list may only shrink; new: {}",
			join(&added)
		)));
	}
	// Every test file names its route, and only test files do; each route
	// is held to its entry point's active declarations.
	let measure = all
		.values()
		.any(|(_, r)| r.as_deref() == Some("xtask measure-checks"));
	let entrypoints = crate::routes::Entrypoints::load(root, measure)?;
	let mut errors = Vec::new();
	for (path, (_, route)) in &all {
		match (is_test(path), route) {
			(true, Some(route)) => errors.extend(entrypoints.error(path, route)),
			(true, None) => errors.push(format!("{path}: a test file names its route")),
			(false, Some(_)) => errors.push(format!("{path}: only a test file has a route")),
			(false, None) => {}
		}
	}
	if !errors.is_empty() {
		bail!(value_error(&format!("test routes: {}", errors.join("; "))));
	}
	let mut routes = BTreeMap::new();
	for route in all.values().filter_map(|(_, r)| r.as_ref()) {
		let kind = match route.split_once(' ') {
			Some(("final", _)) => "final",
			Some(("deferred:", _)) => "deferred",
			_ => route.as_str(),
		};
		*routes.entry(kind.to_owned()).or_insert(0) += 1;
	}
	let mut counts = BTreeMap::new();
	for disposition in listed.values() {
		*counts.entry(disposition.clone()).or_insert(0) += 1;
	}
	Ok(
		json!({"status": "PASS", "files": listed.len(), "removed": parent.len() - names.len(), "dispositions": counts, "test_routes": routes}),
	)
}

fn join(paths: &[&String]) -> String {
	paths
		.iter()
		.map(|p| p.as_str())
		.collect::<Vec<_>>()
		.join(", ")
}
