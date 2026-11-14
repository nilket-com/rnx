//! Closed, isolated host fixtures. No arbitrary script or ambient input is admitted.
use super::classify;
use crate::session::Session;
use crate::session::domain_session;
use serde_json::{Value, json};
use std::{
	collections::{BTreeMap, BTreeSet},
	io::Write,
	path::{Component, Path, PathBuf},
	process::{Command, Stdio},
};

pub(crate) const NAMES: &[&str] = &[
	"io::stdin",
	"fs::read",
	"fs::exists",
	"fs::absolute",
	"fs::mkdir",
	"fs::mkdir_all",
	"fs::remove_file",
	"fs::remove_dir",
	"fs::remove_dir_all",
	"fs::cwd",
	"fs::temp_dir",
	"fs::copy",
	"fs::rename",
];
struct Call {
	args: Vec<&'static str>,
	expected: String,
}
fn calls(name: &str, root: &Path, empty: bool) -> Vec<Call> {
	let ok = |v: &str| format!("Ok({v})");
	let err = |v: &str| format!("Err({v:?})");
	let rows: Vec<(Vec<&str>, String)> = match name {
		"io::stdin" => vec![
			(vec![], ok(if empty { "\"\"" } else { "\"fixture\\n\"" })),
			(
				vec![],
				err("cannot read standard input: it has already been read"),
			),
		],
		"fs::read" => vec![
			(vec!["seed"], ok("\"abc\\né\"")),
			(
				vec!["missing"],
				err("cannot read missing: No such file or directory (os error 2)"),
			),
		],
		"fs::exists" => vec![(vec!["seed"], ok("true")), (vec!["missing"], ok("false"))],
		"fs::absolute" => vec![
			(
				vec!["seed"],
				ok(&format!("{:?}", root.join("work/seed").to_str().unwrap())),
			),
			(
				vec!["missing"],
				err("cannot resolve missing: No such file or directory (os error 2)"),
			),
		],
		"fs::cwd" => vec![(
			vec![],
			ok(&format!("{:?}", root.join("work").to_str().unwrap())),
		)],
		"fs::temp_dir" => vec![(
			vec![],
			ok(&format!("{:?}", root.join("tmp").to_str().unwrap())),
		)],
		"fs::mkdir" => vec![
			(vec!["made"], ok("()")),
			(
				vec!["seed"],
				err("cannot create directory seed: File exists (os error 17)"),
			),
		],
		"fs::mkdir_all" => vec![
			(vec!["nested/leaf"], ok("()")),
			(
				vec!["seed/leaf"],
				err("cannot create directories seed/leaf: Not a directory (os error 20)"),
			),
		],
		"fs::remove_file" => vec![
			(vec!["seed"], ok("()")),
			(
				vec!["missing"],
				err("cannot remove file missing: No such file or directory (os error 2)"),
			),
		],
		"fs::remove_dir" => vec![
			(vec!["empty"], ok("()")),
			(
				vec!["full"],
				err("cannot remove directory full: Directory not empty (os error 39)"),
			),
		],
		"fs::remove_dir_all" => vec![
			(vec!["full"], ok("()")),
			(
				vec!["missing"],
				err("cannot remove tree missing: No such file or directory (os error 2)"),
			),
		],
		"fs::copy" => vec![
			(vec!["seed", "copied"], ok("()")),
			(
				vec!["missing", "never"],
				err("cannot copy missing to never: No such file or directory (os error 2)"),
			),
		],
		"fs::rename" => vec![
			(vec!["seed", "renamed"], ok("()")),
			(
				vec!["missing", "never"],
				err("cannot rename missing to never: No such file or directory (os error 2)"),
			),
		],
		_ => panic!("unknown fixture"),
	};
	rows.into_iter()
		.map(|(args, expected)| Call { args, expected })
		.collect()
}
fn private() -> PathBuf {
	let p = std::env::temp_dir().join(format!(
		"rnx-external-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	std::fs::create_dir(&p).unwrap();
	p.canonicalize().unwrap()
}
fn setup(root: &Path) {
	for dir in ["work/empty", "work/full", "tmp"] {
		std::fs::create_dir_all(root.join(dir)).unwrap();
	}
	std::fs::write(root.join("work/seed"), "abc\né").unwrap();
	std::fs::write(root.join("work/full/data"), "retained").unwrap();
}
fn snapshot(root: &Path) -> BTreeMap<String, Value> {
	fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Value>) {
		for entry in std::fs::read_dir(dir).unwrap() {
			let p = entry.unwrap().path();
			let m = std::fs::symlink_metadata(&p).unwrap();
			assert!(!m.file_type().is_symlink(), "unexpected symlink");
			let value = if m.is_dir() {
				json!({"kind":"directory"})
			} else {
				assert!(m.is_file());
				json!({"kind":"file","bytes":std::fs::read(&p).unwrap()})
			};
			out.insert(
				p.strip_prefix(root).unwrap().to_str().unwrap().into(),
				value,
			);
			if m.is_dir() {
				walk(root, &p, out);
			}
		}
	}
	let mut out = BTreeMap::new();
	walk(root, root, &mut out);
	out
}
fn expected_effect(state: &mut BTreeMap<String, Value>, name: &str) {
	match name {
		"fs::mkdir" => {
			state.insert("work/made".into(), json!({"kind":"directory"}));
		}
		"fs::mkdir_all" => {
			for p in ["work/nested", "work/nested/leaf"] {
				state.insert(p.into(), json!({"kind":"directory"}));
			}
		}
		"fs::remove_file" => {
			state.remove("work/seed");
		}
		"fs::remove_dir" => {
			state.remove("work/empty");
		}
		"fs::remove_dir_all" => {
			state.remove("work/full");
			state.remove("work/full/data");
		}
		"fs::copy" => {
			state.insert("work/copied".into(), state["work/seed"].clone());
		}
		"fs::rename" => {
			let v = state.remove("work/seed").unwrap();
			state.insert("work/renamed".into(), v);
		}
		_ => {}
	}
}
fn admit(root: &Path, args: &[&str], id: usize, used: &mut BTreeSet<usize>) -> Result<(), String> {
	for arg in args {
		let mut current = root.join("work");
		for c in Path::new(arg).components() {
			let Component::Normal(part) = c else {
				return Err("non-private argument".into());
			};
			current.push(part);
			match std::fs::symlink_metadata(&current) {
				Ok(m) if m.file_type().is_symlink() => return Err("symlink argument".into()),
				Ok(_) => {}
				Err(e)
					if matches!(
						e.kind(),
						std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
					) => {}
				Err(e) => return Err(e.to_string()),
			}
		}
		if arg.is_empty() {
			return Err("empty argument".into());
		}
	}
	if !used.insert(id) {
		return Err("duplicate call".into());
	}
	Ok(())
}
// Values are Debug-rendered strings: quotes bound a path; descendants continue with '/'.
fn mark(text: &str, root: &str) -> String {
	let mut out = String::new();
	let mut from = 0;
	for (at, _) in text.match_indices(root) {
		let before = text[..at].chars().next_back();
		let after = text[at + root.len()..].chars().next();
		if (at == 0 || before == Some('"')) && matches!(after, None | Some('"' | '/')) {
			out.push_str(&text[from..at]);
			out.push_str("<PRIVATE>");
			from = at + root.len();
		}
	}
	out.push_str(&text[from..]);
	out
}
fn normalize(value: &mut Value, root: &str) {
	if let Some(text) = value
		.get("value")
		.and_then(Value::as_str)
		.map(str::to_owned)
	{
		value["value"] = mark(&text, root).into();
	}
	if let Some(text) = value
		.get("message")
		.and_then(Value::as_str)
		.map(str::to_owned)
	{
		value["message"] = mark(&text, root).into();
	}
}
#[test]
fn child() {
	let Ok(name) = std::env::var("RNX_EXTERNAL_NAME") else {
		return;
	};
	let root = PathBuf::from(std::env::var_os("RNX_EXTERNAL_ROOT").unwrap());
	assert_eq!(std::env::current_dir().unwrap(), root.join("work"));
	assert_eq!(std::env::var_os("HOME").unwrap(), root.as_os_str());
	assert_eq!(std::env::temp_dir(), root.join("tmp"));
	let legacy = std::env::var("RNX_EXTERNAL_ENGINE").unwrap() == "legacy";
	let empty = std::env::var("RNX_EXTERNAL_EMPTY").unwrap() == "yes";
	let context = || crate::routing_inventory::session_parity_context().unwrap();
	let mut old = legacy.then(|| Session::new(context()).unwrap());
	let mut new = (!legacy).then(|| {
		domain_session::DomainSession::for_frontend(
			context(),
			crate::session::DEFAULT_CEILING,
			crate::session::remedy::FrontEnd::Terminal,
		)
		.unwrap()
	});
	let mut used = BTreeSet::new();
	let mut expected = snapshot(&root);
	let mut rows = Vec::new();
	for (id, call) in calls(&name, &root, empty).into_iter().enumerate() {
		admit(&root, &call.args, id, &mut used).unwrap();
		let args = call
			.args
			.iter()
			.map(|a| format!("{a:?}"))
			.collect::<Vec<_>>()
			.join(",");
		let input = format!("{name}({args})");
		let mut result = if let Some(s) = old.as_mut() {
			domain_session::transcripts::parity_old(s, &input)
		} else {
			domain_session::transcripts::parity_new(new.as_mut().unwrap(), &input)
		};
		// Every fixture lists its successful call first; later calls must preserve its state.
		if id == 0 {
			expected_effect(&mut expected, &name);
		}
		let state = snapshot(&root);
		if legacy {
			assert_eq!(result["outcome"], "ok");
			assert_eq!(result["value"], call.expected);
			assert_eq!(state, expected, "legacy state {name}/{id}");
		}
		normalize(&mut result, root.to_str().unwrap());
		rows.push(json!({"input":input,"result":result,"state":state}));
	}
	if let Some(s) = old {
		s.close().unwrap();
	}
	if let Some(mut s) = new {
		s.close().unwrap();
	}
	std::fs::write(
		std::env::var_os("RNX_EXTERNAL_RECEIPT").unwrap(),
		serde_json::to_vec(&rows).unwrap(),
	)
	.unwrap();
}
fn engine(
	root: &Path,
	receipt: &Path,
	name: &str,
	engine: &str,
	bytes: &[u8],
	empty: bool,
) -> Value {
	setup(root);
	let mut child = Command::new(std::env::current_exe().unwrap())
		.args(["--exact", "session::parity::external::child", "--nocapture"])
		.env_clear()
		.env("RNX_EXTERNAL_NAME", name)
		.env("RNX_EXTERNAL_ROOT", root)
		.env("RNX_EXTERNAL_ENGINE", engine)
		.env("RNX_EXTERNAL_EMPTY", if empty { "yes" } else { "no" })
		.env("RNX_EXTERNAL_RECEIPT", receipt)
		.env("HOME", root)
		.env("TMPDIR", root.join("tmp"))
		.current_dir(root.join("work"))
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.take().unwrap().write_all(bytes).unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(
		out.status.success(),
		"external witness retained at {}: {}\n{}",
		root.display(),
		String::from_utf8_lossy(&out.stdout),
		String::from_utf8_lossy(&out.stderr)
	);
	serde_json::from_slice(&std::fs::read(receipt).unwrap()).unwrap()
}
pub(crate) fn report(name: &str) -> Value {
	assert!(NAMES.contains(&name));
	let dir = private();
	let mut old = Vec::new();
	let mut new = Vec::new();
	for empty in if name == "io::stdin" {
		vec![false, true]
	} else {
		vec![false]
	} {
		let suffix = if empty { "empty" } else { "data" };
		for (role, kind, rows) in [
			("old0", "legacy", &mut old),
			("new0", "execution", &mut new),
		] {
			let root = dir.join(format!("{suffix}-{role}"));
			rows.push(engine(
				&root,
				&dir.join(format!("{suffix}-{role}.json")),
				name,
				kind,
				if empty { b"" } else { b"fixture\n" },
				empty,
			));
		}
	}
	let a = json!({"outcome":"ok","calls":old});
	let b = json!({"outcome":"ok","calls":new});
	let (class, rule, worse) = classify(&a, &b);
	let row = json!({"function":name,"input":"isolated per-call witness","legacy":a,"domain":b,"class":class,"wording_rule":rule,"domain_worse":worse});
	if class == "Equal" {
		std::fs::remove_dir_all(&dir).unwrap();
	} else {
		eprintln!("differing witness retained at {}", dir.display());
	}
	row
}
#[test]
fn guards() {
	let dir = private();
	setup(&dir);
	let mut used = BTreeSet::new();
	let mut invocations = 0;
	for args in [vec!["../escape"], vec!["/outside"]] {
		if admit(&dir, &args, 0, &mut used).is_ok() {
			invocations += 1;
		}
	}
	assert_eq!(invocations, 0);
	admit(&dir, &["seed"], 0, &mut used).unwrap();
	invocations += 1;
	if admit(&dir, &["seed"], 0, &mut used).is_ok() {
		invocations += 1;
	}
	assert_eq!(invocations, 1);
	#[cfg(unix)]
	{
		std::os::unix::fs::symlink("../outside", dir.join("work/link")).unwrap();
		assert!(admit(&dir, &["link/file"], 1, &mut used).is_err());
		std::fs::remove_file(dir.join("work/link")).unwrap();
	}
	let root = "/private/root";
	assert_eq!(
		mark("Ok(\"/private/root/work\")", root),
		"Ok(\"<PRIVATE>/work\")"
	);
	for s in [
		"Ok(\"/private/root-other\")",
		"Ok(\"/else/private/root\")",
		"not /private/root",
		"Ok(\"/receipt\")",
	] {
		assert_eq!(mark(s, root), s);
	}
	let a = json!({"outcome":"ok","result":"Ok(())","state":{}});
	let mut b = a.clone();
	b["state"] = json!({"extra":"file"});
	assert_eq!(classify(&a, &b).0, "Different result");
	std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn measured() {
	for name in NAMES {
		let row = report(name);
		assert!(
			matches!(
				row["class"].as_str(),
				Some(
					"Equal"
						| "Refused at admission"
						| "Different result"
						| "Domain-only failure"
						| "Intentional difference"
						| "Wording" | "Unreviewed wording"
				)
			),
			"{row}"
		);
	}
}

#[test]
fn supplied_input_control() {
	let dir = private();
	let rejected = std::panic::catch_unwind(|| {
		engine(
			&dir.join("old0"),
			&dir.join("receipt.json"),
			"io::stdin",
			"legacy",
			b"different supplied bytes",
			false,
		);
	});
	assert!(
		rejected.is_err(),
		"wrong supplied stdin must fail the legacy expectation"
	);
	std::fs::remove_dir_all(dir).unwrap();
}
