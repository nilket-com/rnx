//! Python stops growing: every tracked `.py` is on a committed list with
//! its disposition, and the list may only lose entries.
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
	let mut rows = BTreeMap::new();
	for line in text
		.lines()
		.filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
	{
		let Some((path, disposition)) = line.split_once('\t') else {
			bail!(value_error(&format!("malformed list line: {line}")))
		};
		if !DISPOSITIONS.contains(&disposition) {
			bail!(value_error(&format!(
				"unknown disposition for {path}: {disposition}"
			)));
		}
		if rows
			.insert(path.to_owned(), disposition.to_owned())
			.is_some()
		{
			bail!(value_error(&format!("listed twice: {path}")));
		}
	}
	Ok(rows)
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
	let listed = parse(&text)?;
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
	let mut counts = BTreeMap::new();
	for disposition in listed.values() {
		*counts.entry(disposition.clone()).or_insert(0) += 1;
	}
	Ok(
		json!({"status": "PASS", "files": listed.len(), "removed": parent.len() - names.len(), "dispositions": counts}),
	)
}

fn join(paths: &[&String]) -> String {
	paths
		.iter()
		.map(|p| p.as_str())
		.collect::<Vec<_>>()
		.join(", ")
}
