//! Fixed protocol fixture for the unchanged journey, never a notebook implementation.
use super::Result;
use rune::experimental_core::{
	heap::Store,
	value::{Root, Type},
};
use serde_json::{Value, json};
use std::{fs, path::Path};

const ARGS: [&str; 6] = [
	"/work/run/journey-mock",
	"fixture-window",
	"/work/run/fixture-data.csv",
	"journey-output",
	"data",
	"plot",
];
const DIGEST: &str = "journey-fixture-digest";

pub(super) fn stock_arguments() -> Result<rune::Value> {
	Ok(rune::to_value(
		ARGS.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>(),
	)?)
}

pub(super) fn fresh_arguments(store: &mut Store) -> Result<Root> {
	let mut strings = Vec::new();
	for text in ARGS {
		strings.push(
			store
				.root_string(text)
				.map_err(super::census_execution::opaque_error)?,
		);
	}
	let borrowed = strings.iter().collect::<Vec<_>>();
	let result = store
		.root_list(Type::Vec, &borrowed)
		.map_err(super::census_execution::opaque_error);
	for root in strings {
		store
			.root_release(root)
			.map_err(super::census_execution::opaque_error)?;
	}
	Ok(result?)
}

pub(super) fn setup(repo: &Path, work: &Path) -> Result<()> {
	use std::os::unix::fs::PermissionsExt;
	let executable = work.join("run/journey-mock");
	fs::copy(
		repo.join("tools/verify/fixtures/census/journey_mock.py"),
		&executable,
	)?;
	fs::set_permissions(&executable, fs::Permissions::from_mode(0o755))?;
	fs::write(
		work.join("run/fixture-data.csv"),
		"x,y,label\n1,2,fixture\n",
	)?;
	Ok(())
}

pub(super) fn check(work: &Path, stdout: &[u8], observation: &Value) -> Result<()> {
	let state: Value = serde_json::from_slice(&fs::read(work.join("run/journey-state.json"))?)?;
	let digest_hex: String = DIGEST.bytes().map(|b| format!("{b:02x}")).collect();
	let expected = json!({"status":"value","typed":{"variant":"Result","success":true,"payload":{"string_hex":digest_hex}}});
	if state != json!({"next":20})
		|| observation["outcome"] != expected
		|| stdout != b"Completed 19 acknowledged actions: /work/run/journey-output\n"
	{
		return Err("journey protocol fixture did not finish 19 actions with the final digest and expected stdout".into());
	}
	Ok(())
}

pub(super) fn self_check() -> Result<()> {
	let fixture = include_str!("../../tools/verify/fixtures/census/journey_mock.py");
	let result = std::process::Command::new("python3").args(["-I", "-B", "-c", &format!(r#"
import types
m = types.ModuleType('journey_mock_control')
exec(compile({fixture:?}, 'journey_mock.py', 'exec'), m.__dict__)
for n in range(1,20):
    m.answer(n, m.expected(n))
for n, bad in [(1, {{'unexpected': {{}}}}), (2, {{'run_cell': {{'target': {{}}}}}}), (20, {{'publish': {{}}}})]:
    try:
        m.answer(n,bad)
    except ValueError as error:
        assert 'unexpected command' in str(error)
    else:
        raise AssertionError('fixture accepted an unexpected command')
"#)]).output()?;
	if !result.status.success() {
		return Err(format!(
			"journey fixture controls: {}",
			String::from_utf8_lossy(&result.stderr)
		)
		.into());
	}
	Ok(())
}
