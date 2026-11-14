//! Explicit reviewed replays, not an extension to the literal Rust extractor.
use super::*;
use crate::session::{
	Session,
	domain_session::{
		DomainSession,
		transcripts::{parity_budget, parity_new, parity_old},
	},
};
use serde_json::{Value, json};
use std::{
	collections::BTreeMap,
	io::Write,
	path::PathBuf,
	process::{Command, Stdio},
};

fn recipes() -> Vec<Value> {
	serde_json::from_str(include_str!(
		"../../../tests/fixtures/session_witnesses.json"
	))
	.unwrap()
}
fn sha(bytes: &[u8]) -> String {
	let mut child = Command::new("sha256sum")
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()
		.unwrap();
	child.stdin.take().unwrap().write_all(bytes).unwrap();
	let out = child.wait_with_output().unwrap();
	assert!(out.status.success());
	String::from_utf8(out.stdout).unwrap()[..64].into()
}
fn bodies(source: &str) -> BTreeMap<String, String> {
	fn walk(items: &[Item], prefix: &str, out: &mut BTreeMap<String, String>) {
		for item in items {
			match item {
				Item::Fn(f) => {
					out.insert(
						format!("{prefix}{}", f.sig.ident),
						f.block.to_token_stream().to_string(),
					);
				}
				Item::Mod(m) => {
					if let Some((_, items)) = &m.content {
						walk(items, &format!("{prefix}{}::", m.ident), out)
					}
				}
				_ => {}
			}
		}
	}
	let mut out = BTreeMap::new();
	walk(&syn::parse_file(source).unwrap().items, "", &mut out);
	out
}
fn material(source: &str, recipe: &Value) -> Value {
	let functions = bodies(source);
	let name = recipe["name"].as_str().unwrap();
	let mut text = functions[name].clone();
	if name.starts_with("prelude_tests::") {
		text.push_str(&functions["prelude_tests::session"]);
		text.push_str(&functions["prelude_tests::shown"]);
	}
	json!({"source":sha(text.as_bytes()),"recipe":sha(serde_json::to_string(recipe).unwrap().as_bytes())})
}
fn ties(source: &str, specs: &[Value]) -> Value {
	Value::Object(
		specs
			.iter()
			.map(|r| (r["name"].as_str().unwrap().into(), material(source, r)))
			.collect(),
	)
}
fn validate_ties() {
	let actual = ties(include_str!("../../session.rs"), &recipes());
	let expected: Value = serde_json::from_str(include_str!(
		"../../../tests/fixtures/session_witness_pins.json"
	))
	.unwrap();
	assert_eq!(
		actual, expected,
		"reviewed witness/source correspondence changed"
	);
}
fn private() -> PathBuf {
	let dir = std::env::temp_dir().join(format!(
		"rnx-witness-{}-{}",
		std::process::id(),
		std::time::SystemTime::now()
			.duration_since(std::time::UNIX_EPOCH)
			.unwrap()
			.as_nanos()
	));
	std::fs::create_dir(&dir).unwrap();
	dir
}
fn expected(observed: &Value, expectation: &Value) {
	for (key, value) in expectation.as_object().unwrap() {
		assert_eq!(
			&observed[key], value,
			"legacy witness does not reproduce original assertion: {observed}"
		);
	}
}

#[test]
fn witness_probe() {
	let Ok(name) = std::env::var("RNX_SESSION_WITNESS") else {
		return;
	};
	validate_ties();
	let recipe = recipes().into_iter().find(|r| r["name"] == name).unwrap();
	let mut old = Session::new(context()).unwrap();
	let mut fresh = DomainSession::for_frontend(
		context(),
		crate::session::DEFAULT_CEILING,
		crate::session::remedy::FrontEnd::Terminal,
	)
	.unwrap();
	let directory = private();
	let mut external = false;
	let mut rows = Vec::new();
	for (index, action) in recipe["actions"].as_array().unwrap().iter().enumerate() {
		if let Some(b) = action.get("budget") {
			let budget = if b == "max" {
				usize::MAX
			} else {
				b.as_u64().unwrap() as usize
			};
			old.set_budget(budget);
			parity_budget(&mut fresh, budget);
			continue;
		}
		if let Some(count) = action.get("declarations") {
			assert_eq!(count, 100); // This is one reviewed expansion, not a formatter evaluator.
			for i in 0..100 {
				let text = format!("let n{i:03} = {i};");
				expected(&parity_old(&mut old, &text), &json!({"outcome":"ok"}));
				expected(&parity_new(&mut fresh, &text), &json!({"outcome":"ok"}));
			}
			continue;
		}
		if action.get("release_effect").is_some() {
			assert!(external);
			for role in ["old0", "new0"] {
				std::fs::remove_file(directory.join(role)).unwrap();
			}
			external = false;
			continue;
		}
		if action.get("independent_reset").is_some() {
			let mut reset = Session::new(context()).unwrap();
			let mut domain_reset = DomainSession::for_frontend(
				context(),
				crate::session::DEFAULT_CEILING,
				crate::session::remedy::FrontEnd::Terminal,
			)
			.unwrap();
			expected(&parity_old(&mut reset, "x"), &json!({"outcome":"compile"}));
			expected(
				&parity_new(&mut domain_reset, "x"),
				&json!({"outcome":"compile"}),
			);
			reset.close().unwrap();
			domain_reset.close().unwrap();
			continue;
		}
		if action.get("external").is_some() {
			for (role, legacy) in [("old0", true), ("new0", false)] {
				let path = directory.join(role);
				let quoted = serde_json::to_string(path.to_str().unwrap()).unwrap();
				let input = format!(
					"fs::write_new({quoted}, \"once\")?; panic!(\"after external write\");"
				);
				let result = if legacy {
					parity_old(&mut old, &input)
				} else {
					parity_new(&mut fresh, &input)
				};
				assert_eq!(result["outcome"], "runtime", "{result}");
				assert!(
					result["message"]
						.as_str()
						.unwrap()
						.contains("after external write")
				);
				assert_eq!(std::fs::read_to_string(path).unwrap(), "once");
			}
			external = true;
			continue;
		}
		let input = action["input"].as_str().unwrap();
		for _ in 0..action.get("repeat").and_then(Value::as_u64).unwrap_or(1) {
			println!("__WITNESS_OLD_{index}__");
			let a = parity_old(&mut old, input);
			println!("__WITNESS_NEW_{index}__");
			let b = parity_new(&mut fresh, input);
			println!("__WITNESS_END_{index}__");
			expected(&a, &action["expect"]);
			if let Some(target) = action.get("target") {
				rows.push(json!({"sequence":name,"ordinal":target["ordinal"],"input":input,"blocked_by":target["blocked_by"],"witness":name,"tie":material(include_str!("../../session.rs"),&recipe),"legacy":a,"domain":b,"marker":index,"expected_stdout":target["stdout"]}));
			}
			if external {
				for role in ["old0", "new0"] {
					assert_eq!(
						std::fs::read_to_string(directory.join(role)).unwrap(),
						"once"
					);
				}
			}
		}
	}
	if name.ends_with("the_generated_source_follows_the_input_not_the_session") {
		let mut narrow = Session::new(context()).unwrap();
		narrow.set_budget(usize::MAX);
		for input in ["let n000 = 0;", "let n099 = 99;", "n000 + n099"] {
			narrow.eval(input).unwrap();
		}
		assert_eq!(old.last_generated().len(), narrow.last_generated().len());
		assert_eq!(old.last_generated().matches("__rnx_state[").count(), 2);
		narrow.close().unwrap();
	}
	old.close().unwrap();
	fresh.close().unwrap();
	std::fs::remove_dir_all(directory).unwrap();
	std::fs::write(
		std::env::var_os("RNX_WITNESS_REPORT").unwrap(),
		serde_json::to_vec_pretty(&rows).unwrap(),
	)
	.unwrap();
}

pub(super) fn report() -> Vec<Value> {
	validate_ties();
	let directory = private();
	let mut rows = Vec::new();
	for (i, recipe) in recipes().iter().enumerate() {
		let path = directory.join(format!("{i}.json"));
		let name = recipe["name"].as_str().unwrap();
		let output = Command::new(std::env::current_exe().unwrap())
			.args([
				"--exact",
				"session::parity::witnesses::witness_probe",
				"--nocapture",
			])
			.env("RNX_SESSION_WITNESS", name)
			.env("RNX_WITNESS_REPORT", &path)
			.output()
			.unwrap();
		assert!(
			output.status.success(),
			"witness {name} failed: {}\n{}",
			String::from_utf8_lossy(&output.stdout),
			String::from_utf8_lossy(&output.stderr)
		);
		let stdout = String::from_utf8(output.stdout).unwrap();
		let mut measured: Vec<Value> =
			serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
		for row in &mut measured {
			let marker = row["marker"].as_u64().unwrap();
			let body = stdout
				.split_once(&format!("__WITNESS_OLD_{marker}__\n"))
				.unwrap()
				.1;
			let (old, body) = body
				.split_once(&format!("__WITNESS_NEW_{marker}__\n"))
				.unwrap();
			let new = body
				.split_once(&format!("__WITNESS_END_{marker}__\n"))
				.unwrap()
				.0;
			assert_eq!(
				old,
				row["expected_stdout"].as_str().unwrap(),
				"legacy output assertion"
			);
			row["legacy"]["stdout"] = old.into();
			row["domain"]["stdout"] = new.into();
			let (class, rule, worse) = classify(&row["legacy"], &row["domain"]);
			row["class"] = class.into();
			row["wording_rule"] = json!(rule);
			row["domain_worse"] = worse.into();
			row.as_object_mut().unwrap().remove("marker");
			row.as_object_mut().unwrap().remove("expected_stdout");
		}
		rows.extend(measured);
	}
	std::fs::remove_dir_all(directory).unwrap();
	if let Some(path) = std::env::var_os("RNX_WITNESS_RESULT") {
		std::fs::write(path, serde_json::to_vec_pretty(&rows).unwrap()).unwrap();
	}
	assert_eq!(rows.len(), 14);
	rows
}

pub(super) fn resolved(
	rows: &[Value],
	sequence: &str,
	ordinal: usize,
	input: &str,
	blocker: &str,
) -> Option<Value> {
	let matched: Vec<_> = rows
		.iter()
		.filter(|r| {
			r["sequence"] == sequence
				&& r["ordinal"] == ordinal
				&& r["input"] == input
				&& r["blocked_by"] == blocker
		})
		.collect();
	assert!(matched.len() <= 1, "duplicate witness target");
	matched.first().map(|r| (*r).clone())
}

pub(super) fn blocked_row(
	rows: &[Value],
	sequence: &str,
	ordinal: usize,
	input: &str,
	blocker: &str,
) -> Value {
	resolved(rows,sequence,ordinal,input,blocker).unwrap_or_else(||json!({"sequence":sequence,"ordinal":ordinal,"input":input,"class":"Not classified","blocked_by":blocker}))
}

#[test]
fn reviewed_ties_and_each_mutation_fail() {
	validate_ties();
	let source = include_str!("../../session.rs");
	for recipe in recipes() {
		let original = material(source, &recipe);
		let mut altered = recipe.clone();
		let action = altered["actions"]
			.as_array_mut()
			.unwrap()
			.iter_mut()
			.find(|a| a.get("input").is_some())
			.unwrap();
		let input = action["input"].as_str().unwrap().to_owned();
		action["input"] = format!("{input};0").into();
		assert_ne!(
			original,
			material(source, &altered),
			"recipe drift {}",
			recipe["name"]
		);
		// Alter this mirrored function's body token, not an unrelated location.
		let name = recipe["name"]
			.as_str()
			.unwrap()
			.rsplit("::")
			.next()
			.unwrap();
		let needle = format!("fn {name}()");
		let start = source.find(&needle).unwrap();
		let brace = start + source[start..].find('{').unwrap();
		let quote = brace + source[brace..].find('"').unwrap();
		let changed = format!(
			"{}__witness_drift{}",
			&source[..quote + 1],
			&source[quote + 1..]
		);
		assert_ne!(
			original,
			material(&changed, &recipe),
			"test drift {}",
			recipe["name"]
		);
	}
}
#[test]
fn missing_witness_stays_blocked_and_output_changes_fail() {
	let block: syn::Block =
		syn::parse_str("{for _ in 0..2 {session.eval(&computed());} session.eval(\"x\");}")
			.unwrap();
	let mut extractor = Extract {
		row: Sequence::default(),
		reason: None,
		array: None,
	};
	extractor.visit_block(&block);
	assert_eq!(extractor.row.events[0].action, "unclassified");
	let event = &extractor.row.events[1];
	assert_eq!(event.action, "eval");
	let row = blocked_row(&[], "unknown", 1, &event.input, "unclassified site 0");
	assert_eq!(row["class"], "Not classified");
	assert_eq!(row["blocked_by"], "unclassified site 0");
	let a = json!({"outcome":"ok","value":null,"stdout":"7\n"});
	let mut b = a.clone();
	b["stdout"] = "8\n".into();
	let class = classify(&a, &b).0;
	assert_eq!(class, "Different result");
	let differing = json!({"sequence":"synthetic","ordinal":1,"input":"x","blocked_by":"unclassified site 0","legacy":a,"domain":b,"class":class});
	let attached = blocked_row(
		std::slice::from_ref(&differing),
		"synthetic",
		1,
		"x",
		"unclassified site 0",
	);
	assert_eq!(attached, differing);
	assert_eq!(attached["class"], "Different result");
}
#[test]
fn measured_witnesses() {
	report();
}
