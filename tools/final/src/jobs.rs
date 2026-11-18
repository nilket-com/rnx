//! `rnx-final jobs` (ADR 0295): what is running, and how far a final has
//! come, in one snapshot for a person at a terminal. It reads the user
//! manager and the finals' own receipts and changes nothing.
//!
//! - `rnx-final jobs`: every running `rnx-*` user service with its elapsed
//!   time and memory; a service tied to a final also shows its stages and
//!   a time estimate against a labeled earlier PASS. With nothing running,
//!   the latest final's verdict and receipt.
//! - `rnx-final jobs -w`: the same view every 30 seconds, for a person
//!   watching; agents wait on a job's exit and read its terminal receipt.
//! - `rnx-final jobs --final DIR`: one final directory.
//!
//! Finals are found by their receipts (a directory holding
//! `SELECTION.json`) under `RNX_FINALS`, by default the checkout's
//! `target/`.
use crate::schedule::Schedule;
use anyhow::{Context as _, Result, bail};
use serde_json::Value;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

/// One running service, as the user manager and its main process tell it.
#[derive(Clone, Debug, PartialEq)]
pub struct Unit {
	pub name: String,
	pub elapsed: Option<Duration>,
	pub memory: Option<u64>,
	/// The final directory its main process names with `--out`, if any.
	pub out: Option<PathBuf>,
}

/// What the user manager answered.
#[derive(Clone, Debug, PartialEq)]
pub enum Units {
	Unavailable(String),
	Running(Vec<Unit>),
}

/// The live source of running services, so a control can answer instead.
pub trait Manager {
	fn units(&self) -> Units;
}

/// A process's argv from `/proc/PID/cmdline`: NUL-terminated arguments,
/// an empty one kept as empty, so no boundary moves.
pub fn argv_from_cmdline(bytes: &[u8]) -> Vec<OsString> {
	let Some(body) = bytes.strip_suffix(b"\0") else {
		return vec![];
	};
	body.split(|b| *b == 0)
		.map(|a| {
			// SAFETY: bytes from the kernel's argv, a valid OsStr on Unix.
			unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(a) }.to_owned()
		})
		.collect()
}

/// The final directory a process's argv names with `--out DIR` or
/// `--out=DIR`, relative to its working directory: only when that is
/// unambiguous. A missing or empty value, or two different ones, names
/// nothing.
pub fn out_from_argv(argv: &[OsString], cwd: Option<&Path>) -> Option<PathBuf> {
	let mut values: Vec<&[u8]> = Vec::new();
	for (i, arg) in argv.iter().enumerate() {
		let bytes = arg.as_encoded_bytes();
		if bytes == b"--out" {
			values.push(argv.get(i + 1)?.as_encoded_bytes());
		} else if let Some(rest) = bytes.strip_prefix(b"--out=") {
			values.push(rest);
		}
	}
	let first = *values.first()?;
	if first.is_empty() || values.iter().any(|v| *v != first) {
		return None;
	}
	// SAFETY: bytes taken whole from a valid OsStr, or after an ASCII prefix.
	let out = PathBuf::from(unsafe { std::ffi::OsStr::from_encoded_bytes_unchecked(first) });
	if out.is_absolute() {
		Some(out)
	} else {
		cwd.map(|c| c.join(out))
	}
}

/// `systemctl show` output: one property map per unit, blank-line separated.
pub fn parse_show(text: &str) -> Vec<std::collections::BTreeMap<String, String>> {
	let mut units = Vec::new();
	let mut current = std::collections::BTreeMap::new();
	for line in text.lines() {
		if line.is_empty() {
			if !current.is_empty() {
				units.push(std::mem::take(&mut current));
			}
		} else if let Some((key, value)) = line.split_once('=') {
			current.insert(key.to_owned(), value.to_owned());
		}
	}
	if !current.is_empty() {
		units.push(current);
	}
	units
}

/// The user manager through `systemctl --user`, and each service's main
/// process through `/proc`: one listing and one `show` per view.
pub struct Live;

fn monotonic_now() -> Duration {
	let mut now = libc::timespec {
		tv_sec: 0,
		tv_nsec: 0,
	};
	// SAFETY: clock_gettime fills a local timespec.
	unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut now) };
	Duration::new(now.tv_sec as u64, now.tv_nsec as u32)
}

impl Manager for Live {
	fn units(&self) -> Units {
		let systemctl = |args: &[&str]| -> std::result::Result<String, String> {
			let output = std::process::Command::new("systemctl")
				.arg("--user")
				.args(args)
				.output()
				.map_err(|e| format!("systemctl: {e}"))?;
			if !output.status.success() {
				let stderr = String::from_utf8_lossy(&output.stderr);
				return Err(stderr
					.lines()
					.next()
					.unwrap_or("systemctl failed")
					.to_owned());
			}
			Ok(String::from_utf8_lossy(&output.stdout).into_owned())
		};
		let listed = match systemctl(&[
			"list-units",
			"--type=service",
			"--state=running",
			"--no-legend",
			"--plain",
			"rnx-*",
		]) {
			Ok(text) => text,
			Err(reason) => return Units::Unavailable(reason),
		};
		let names: Vec<&str> = listed
			.lines()
			.filter_map(|l| l.split_whitespace().next())
			.collect();
		if names.is_empty() {
			return Units::Running(vec![]);
		}
		let mut args = vec![
			"show",
			"-p",
			"Id",
			"-p",
			"ActiveEnterTimestampMonotonic",
			"-p",
			"MemoryCurrent",
			"-p",
			"MainPID",
		];
		args.extend(&names);
		let shown = match systemctl(&args) {
			Ok(text) => text,
			Err(reason) => return Units::Unavailable(reason),
		};
		let now = monotonic_now();
		let units = parse_show(&shown)
			.into_iter()
			.map(|p| {
				let entered = p
					.get("ActiveEnterTimestampMonotonic")
					.and_then(|v| v.parse::<u64>().ok())
					.filter(|v| *v > 0)
					.map(Duration::from_micros);
				let pid = p
					.get("MainPID")
					.and_then(|v| v.parse::<u32>().ok())
					.filter(|v| *v > 0);
				let out = pid.and_then(|pid| {
					let cmdline = std::fs::read(format!("/proc/{pid}/cmdline")).ok()?;
					let argv = argv_from_cmdline(&cmdline);
					let cwd = std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
					out_from_argv(&argv, cwd.as_deref())
				});
				Unit {
					name: p
						.get("Id")
						.map(|n| n.trim_end_matches(".service").to_owned())
						.unwrap_or_default(),
					elapsed: entered.map(|e| now.saturating_sub(e)),
					memory: p.get("MemoryCurrent").and_then(|v| v.parse().ok()),
					out,
				}
			})
			.collect();
		Units::Running(units)
	}
}

/// Every final under `root`, by its receipts: a directory one or two
/// levels down holding `SELECTION.json`, newest start first; Cargo's build
/// directories are not listed.
pub fn finals(root: &Path) -> Vec<PathBuf> {
	let dirs = |path: &Path| -> Vec<PathBuf> {
		std::fs::read_dir(path)
			.map(|entries| {
				entries
					.filter_map(|e| e.ok())
					.filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
					.map(|e| e.path())
					.collect()
			})
			.unwrap_or_default()
	};
	let mut found: Vec<(SystemTime, PathBuf)> = Vec::new();
	for first in dirs(root) {
		// A Cargo build directory (it holds Cargo's `.cargo-lock`) is never
		// listed: a snapshot does not walk build caches.
		let children = if first.join(".cargo-lock").exists() {
			vec![]
		} else {
			dirs(&first)
		};
		for candidate in std::iter::once(first.clone()).chain(children) {
			if let Some(started) = started(&candidate) {
				found.push((started, candidate));
			}
		}
	}
	found.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
	found.into_iter().map(|(_, p)| p).collect()
}

/// When a final began: its selection receipt's time.
fn started(dir: &Path) -> Option<SystemTime> {
	let meta = dir.join("SELECTION.json").symlink_metadata().ok()?;
	meta.is_file().then(|| meta.modified().ok()).flatten()
}

/// A receipt, only as the regular file it must be: never through a link,
/// and never a pipe or device a read could block on.
fn read(path: &Path) -> Option<Value> {
	use std::os::unix::fs::OpenOptionsExt as _;
	if !path.symlink_metadata().ok()?.is_file() {
		return None;
	}
	let file = std::fs::OpenOptions::new()
		.read(true)
		.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
		.open(path)
		.ok()?;
	if !file.metadata().ok()?.is_file() {
		return None;
	}
	serde_json::from_reader(std::io::BufReader::new(file)).ok()
}

fn regular(path: &Path) -> bool {
	path.symlink_metadata().is_ok_and(|m| m.is_file())
}

/// A final's terminal receipt, when it has a readable one.
struct Terminal {
	status: String,
	complete: bool,
	seconds: f64,
	failed: Vec<String>,
}

fn terminal(dir: &Path) -> Option<Terminal> {
	let r = read(&dir.join("RESULT.json"))?;
	Some(Terminal {
		status: r["status"].as_str()?.to_owned(),
		complete: r["complete_final"] == true,
		seconds: r["seconds"].as_f64().unwrap_or(0.0),
		failed: r["checks"]
			.as_array()
			.map(|c| {
				c.iter()
					.filter(|c| c["exit"] != 0)
					.filter_map(|c| c["name"].as_str().map(String::from))
					.collect()
			})
			.unwrap_or_default(),
	})
}

/// The selected stages, and which have finished or begun without a
/// receipt, by each stage's own child directory; anything else in the
/// directory is ignored.
struct Stages {
	total: usize,
	done: usize,
	begun: Vec<String>,
	/// The selection's scope and stage names, for a comparable reference.
	scope: Option<String>,
	names: std::collections::BTreeSet<String>,
	fix_label: Option<String>,
}

fn stages(dir: &Path) -> Option<Stages> {
	let selection = read(&dir.join("SELECTION.json"))?;
	let names: Vec<&str> = selection["stages"]
		.as_array()?
		.iter()
		.filter_map(|s| s[0].as_str())
		.collect();
	let mut done = 0;
	let mut begun = Vec::new();
	for name in &names {
		let child = dir.join(format!("{name}-child"));
		if regular(&child.join("RESULT.json")) {
			done += 1;
		} else if child.symlink_metadata().is_ok_and(|m| m.is_dir()) {
			begun.push((*name).to_owned());
		}
	}
	Some(Stages {
		total: names.len(),
		done,
		begun,
		scope: selection["scope"].as_str().map(String::from),
		names: names.iter().map(|n| (*n).to_owned()).collect(),
		fix_label: serde_json::from_value::<crate::types::Fix>(selection["record"].clone())
			.ok()
			.filter(|f| {
				crate::selection::fix_subject(&f.subject)
					&& f.candidate.len() == 40
					&& f.candidate.bytes().all(|b| b.is_ascii_hexdigit())
			})
			.map(|f| format!("{} ({})", f.subject, &f.candidate[..8])),
	})
}

fn bar(done: f64, total: f64) -> String {
	let width = 30;
	let fill = if total > 0.0 {
		((done / total) * width as f64)
			.floor()
			.clamp(0.0, width as f64) as usize
	} else {
		0
	};
	format!("{}{}", "#".repeat(fill), "-".repeat(width - fill))
}

fn minutes(seconds: f64) -> String {
	format!("{}m", (seconds / 60.0).floor() as u64)
}

fn clock(seconds: f64) -> String {
	let s = seconds.floor() as u64;
	format!("{}m{:02}s", s / 60, s % 60)
}

fn names(running: &[String]) -> String {
	let shown: Vec<&str> = running.iter().take(4).map(String::as_str).collect();
	let more = running.len().saturating_sub(4);
	let mut text = shown.join(", ");
	if more > 0 {
		text.push_str(&format!(" (+{more} more)"));
	}
	text
}

/// The lines describing one final at `now`, against the most recent
/// complete PASS among `others`. Only a final tied to an active service
/// (`live`) has running stages and an estimate; otherwise a stage begun
/// without a receipt is unfinished, and the final has no terminal receipt.
pub fn final_view(dir: &Path, others: &[PathBuf], now: SystemTime, live: bool) -> Vec<String> {
	let mut lines = vec![format!("final {}", dir.display())];
	let Some(stages) = stages(dir) else {
		lines.push("no readable selection receipt".into());
		return lines;
	};
	if let Some(label) = stages.fix_label {
		lines.push(label);
	}
	if let Some(t) = terminal(dir) {
		let mut line = format!(
			"{} in {}, {}/{} stages done",
			t.status,
			clock(t.seconds),
			stages.done,
			stages.total
		);
		if !t.failed.is_empty() {
			line.push_str(&format!("; failed: {}", t.failed.join(", ")));
		}
		lines.push(line);
		lines.push(format!("receipt {}", dir.join("RESULT.json").display()));
		return lines;
	}
	let mut line = format!(
		"stages [{}] {}/{} done",
		bar(stages.done as f64, stages.total as f64),
		stages.done,
		stages.total
	);
	if !stages.begun.is_empty() {
		line.push_str(&format!(
			", {} {}: {}",
			stages.begun.len(),
			if live { "running" } else { "unfinished" },
			names(&stages.begun)
		));
	}
	lines.push(line);
	let elapsed = started(dir)
		.and_then(|s| now.duration_since(s).ok())
		.unwrap_or_default()
		.as_secs_f64();
	if !live {
		lines.push(format!(
			"no terminal receipt; began {} ago",
			minutes(elapsed)
		));
		return lines;
	}
	// Like with like: a complete final against the last complete PASS;
	// any other known scope against the last PASS of that scope that
	// selected exactly the same stages. A reference began before this
	// final, and has a usable duration.
	let scope = stages
		.scope
		.as_deref()
		.filter(|s| matches!(*s, "complete" | "required" | "explicit" | "docs"));
	let began = started(dir);
	let reference = others.iter().filter(|o| o.as_path() != dir).find_map(|o| {
		let t = terminal(o)?;
		let theirs = self::stages(o)?;
		let earlier = matches!((started(o), began), (Some(a), Some(b)) if a < b);
		let comparable = t.status == "PASS"
			&& earlier
			&& t.seconds.is_finite()
			&& t.seconds > 0.0
			&& scope.is_some()
			&& theirs.scope.as_deref() == scope
			&& match scope {
				Some("complete") => t.complete,
				_ => theirs.names == stages.names,
			};
		comparable.then_some((o, t))
	});
	let kind = match scope {
		Some("complete") => "complete PASS".to_string(),
		Some(other) => format!("{other} PASS of the same {} stages", stages.names.len()),
		None => "comparable PASS".to_string(),
	};
	match reference {
		Some((path, t)) => {
			let paced = read(&path.join("SCHEDULE.json"))
				.and_then(|v| serde_json::from_value::<Schedule>(v).ok())
				.map(|s| {
					s.stages
						.iter()
						.filter(|st| st.ended.is_some_and(|e| e <= elapsed))
						.count()
				});
			lines.push(format!(
				"time   [{}] {} / ~{} est",
				bar(elapsed, t.seconds),
				minutes(elapsed),
				minutes(t.seconds)
			));
			let mut against = format!(
				"against the last {kind}, {} ({})",
				path.display(),
				clock(t.seconds)
			);
			if let Some(paced) = paced {
				against.push_str(&format!(", which had {paced} done by now"));
			}
			lines.push(against);
		}
		None => lines.push(format!(
			"time   {} elapsed; no earlier {kind} to compare with",
			minutes(elapsed)
		)),
	}
	if stages.begun.is_empty() && stages.done < stages.total {
		lines.push("no stage is running".into());
	}
	lines
}

fn memory(bytes: Option<u64>) -> String {
	match bytes {
		Some(b) => format!("{}G", b / (1 << 30)),
		None => "?".into(),
	}
}

/// One view: the running services, or the latest final when none runs.
pub fn snapshot(manager: &dyn Manager, root: &Path, now: SystemTime) -> String {
	let all = finals(root);
	let mut out = Vec::new();
	match manager.units() {
		Units::Unavailable(reason) => {
			out.push(format!("user manager unavailable: {reason}"));
			out.push(String::new());
			out.extend(idle(&all, root, now));
		}
		Units::Running(units) if units.is_empty() => {
			out.push("no rnx-* services running".into());
			out.push(String::new());
			out.extend(idle(&all, root, now));
		}
		Units::Running(units) => {
			for unit in units {
				let elapsed = unit
					.elapsed
					.map_or("?".into(), |e| minutes(e.as_secs_f64()));
				out.push(format!(
					"{}  running {}, memory {}",
					unit.name,
					elapsed,
					memory(unit.memory)
				));
				match unit.out.as_deref().filter(|o| started(o).is_some()) {
					Some(dir) => out.extend(
						final_view(dir, &all, now, true)
							.into_iter()
							.map(|l| format!("  {l}")),
					),
					None => out.push("  (not tied to a final)".into()),
				}
				out.push(String::new());
			}
			out.pop();
		}
	}
	out.join("\n") + "\n"
}

fn idle(all: &[PathBuf], root: &Path, now: SystemTime) -> Vec<String> {
	match all.first() {
		Some(latest) => {
			let mut lines = vec!["latest:".to_string()];
			lines.extend(
				final_view(latest, all, now, false)
					.into_iter()
					.map(|l| format!("  {l}")),
			);
			lines
		}
		None => vec![format!("no finals under {}", root.display())],
	}
}

/// Whether an active service's final is `dir`.
fn active(manager: &dyn Manager, dir: &Path) -> bool {
	let canonical = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
	match manager.units() {
		Units::Running(units) => units
			.iter()
			.filter_map(|u| u.out.as_deref())
			.any(|o| canonical(o) == canonical(dir)),
		Units::Unavailable(_) => false,
	}
}

/// `rnx-final jobs [-w] [--final DIR]`. `root` is the controller's
/// checkout as built, used only for the default `RNX_FINALS` and never
/// required to exist.
pub fn main(arguments: &[OsString], root: &Path) -> Result<i32> {
	let finals_root = std::env::var_os("RNX_FINALS")
		.filter(|v| !v.is_empty())
		.map(PathBuf::from)
		.unwrap_or_else(|| root.join("target"));
	let (mut watch, mut only) = (false, None);
	let mut args = arguments.iter();
	while let Some(arg) = args.next() {
		match arg.to_str() {
			Some("-w") => watch = true,
			Some("--final") => {
				only = Some(PathBuf::from(
					args.next().context("--final needs a directory")?,
				))
			}
			_ => bail!("usage: rnx-final jobs [-w] [--final DIR]"),
		}
	}
	loop {
		let view = match &only {
			Some(dir) => {
				final_view(
					dir,
					&finals(&finals_root),
					SystemTime::now(),
					active(&Live, dir),
				)
				.join("\n") + "\n"
			}
			None => snapshot(&Live, &finals_root, SystemTime::now()),
		};
		if !watch {
			print!("{view}");
			return Ok(0);
		}
		print!("\x1b[H\x1b[2J{view}");
		std::io::Write::flush(&mut std::io::stdout())?;
		std::thread::sleep(Duration::from_secs(30));
	}
}
