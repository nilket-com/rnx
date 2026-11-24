//! Exercise the production debugger and object accounting across a real ELF call.
use rnx_final::receipt::{atomic_json, digest, invocation_id};
use serde_json::{Value, json};
use std::{
	fs,
	path::{Path, PathBuf},
	process::Command,
};

struct Scratch(PathBuf);
impl Drop for Scratch {
	fn drop(&mut self) {
		let _ = fs::remove_dir_all(&self.0);
	}
}

#[test]
fn finish_in_launcher_and_product_object_counts() {
	let root = Path::new(env!("CARGO_MANIFEST_DIR"))
		.join("../..")
		.canonicalize()
		.unwrap();
	let fixtures = root.join("tools/final/tests/image_fixture");
	let path = std::env::temp_dir().join(format!("rnx-image-{}", invocation_id()));
	fs::create_dir(&path).unwrap();
	let scratch = Scratch(path);
	let d = &scratch.0;
	let engine = d.join("libengine_image.so");
	let runner = d.join("rnx");
	for (name, args, out) in [
		("engine.c", vec!["-fPIC", "-shared"], &engine),
		(
			"runner.c",
			vec!["-L.", "-lengine_image", "-Wl,-rpath,$ORIGIN"],
			&runner,
		),
	] {
		assert!(
			Command::new("cc")
				.current_dir(d)
				.args(["-g", "-O0"])
				.arg(fixtures.join(name))
				.args(args)
				.arg("-o")
				.arg(out)
				.status()
				.unwrap()
				.success()
		);
	}
	let measured = json!({"version":1,"target":"x86_64-unknown-linux-gnu",
		"runner":{"path":"/run/rnx-route/rnx","sha256":digest(&runner).unwrap()},
		"engine":{"path":"/run/rnx-route/libengine_image.so","sha256":digest(&engine).unwrap()}});
	let vg = rnx_final::stage::valgrind_root(&std::env::vars().collect()).unwrap();
	let col = root.join("tools/column");
	for case in [
		"valid",
		"decoy",
		"wrong-image",
		"missing-image",
		"wrong-mapping",
	] {
		let out = d.join(case);
		fs::create_dir(&out).unwrap();
		let fixture = out.join("program.rn");
		fs::write(&fixture, "").unwrap();
		let mut cfg = json!({"binary":runner,"engine_binary":engine,"measured":measured,"canonical_layout":true,
			"fixture":fixture,"valgrind_root":vg,"out":out,"options":[],"entry":"fixture_entry","phase_symbols":{"fixture_split":"fixture_split"}});
		match case {
			"decoy" => cfg["entry"] = json!("in_runner"),
			"missing-image" => cfg["engine_binary"] = json!(out.join("missing")),
			"wrong-mapping" => {
				let other = out.join("libengine_image.so");
				let mut bytes = fs::read(&engine).unwrap();
				bytes.push(0);
				fs::write(&other, bytes).unwrap();
				let alternate = out.join("runner");
				assert!(
					Command::new("cc")
						.current_dir(d)
						.args(["-g", "-O0"])
						.arg(fixtures.join("runner.c"))
						.args(["-L.", "-lengine_image", "-Wl,-rpath,$ORIGIN/out", "-o"])
						.arg(&alternate)
						.status()
						.unwrap()
						.success()
				);
				cfg["binary"] = json!(alternate);
				cfg["measured"]["runner"]["sha256"] = json!(digest(&alternate).unwrap());
			}
			"wrong-image" => {
				let wrong = out.join("libengine_image.so");
				fs::copy(&runner, &wrong).unwrap();
				cfg["engine_binary"] = json!(wrong);
			}
			_ => {}
		}
		let config = out.join("CONFIG.json");
		atomic_json(&config, &cfg).unwrap();
		let output = Command::new("/usr/bin/python3")
			// Use the gate's measured environment inside its canonical namespace.
			// The outer stage's temporary path is read-only in that namespace.
			.env_clear()
			.envs(rnx_final::fixed_route_stage::capture_environment())
			.arg("-B")
			.arg(col.join("old_path_child.py"))
			.arg(config)
			.output()
			.unwrap();
		if case != "valid" {
			assert!(!output.status.success(), "{case}");
			if case == "wrong-mapping" || case == "decoy" {
				let error = fs::read_to_string(out.join("gdb.stderr")).unwrap();
				let expected = if case == "wrong-mapping" {
					"bound engine has no executable mapping"
				} else {
					"selector outside bound engine text"
				};
				assert!(error.contains(expected), "{case}: {error}");
			}
			continue;
		}
		assert!(
			output.status.success(),
			"{:?} {:?}",
			output,
			fs::read_to_string(out.join("gdb.stderr"))
		);
		let boundary: Value =
			serde_json::from_slice(&fs::read(out.join("GDB.json")).unwrap()).unwrap();
		assert_eq!(boundary["status"], "PASS");
		assert_eq!(boundary["return_pc"], boundary["caller_pc"]);
		let pc = boundary["return_pc"].as_u64().unwrap();
		assert!(
			boundary["engine"]["text"]
				.as_array()
				.unwrap()
				.iter()
				.all(|range| pc < range[0].as_u64().unwrap() || pc >= range[1].as_u64().unwrap())
		);
		let check=Command::new("/usr/bin/python3").current_dir(&col).args(["-B","-c",r#"
from ir_accounting import phase_accounts
import sys
runner, engine = '/run/rnx-route/rnx', '/run/rnx-route/libengine_image.so'
whole, parts = phase_accounts(sys.argv[1],runner,engine)
a = parts['execution']['accounting']
assert a['product_objects'][runner] == a['rnx_functions']['in_runner'] == 3
assert a['product_objects'][engine] == a['rnx_functions']['fixture_entry'] + a['rnx_functions']['fixture_split'] > 0
assert whole['product_objects'][runner] == 3
"#]).arg(&out).output().unwrap();
		assert!(check.status.success(), "{check:?}");
	}
}
