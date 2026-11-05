#!/usr/bin/env python3
"""One CK1/B1 prerequisite command; serial, fail-fast, exit notification and receipt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import traceback
import tokenize
import hygiene

ROOT = Path(__file__).resolve().parents[3]
HERE = Path(__file__).resolve().parent


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--out", required=True, type=Path)
	parser.add_argument("--probe", action="store_true", help="build and validate the production column measurement probe after prerequisites")
	parser.add_argument("--full", action="store_true", help="run the full Rune library suite after prerequisites")
	parser.add_argument("--notify", action="store_true", help="send only the terminal receipt to Codex via chatd")
	parser.add_argument("--ck2", action="store_true", help="CK2 validity-aware primitive record")
	parser.add_argument("--budget", action="store_true", help="B1 ambient-budget controls before the optional full suite")
	parser.add_argument("--budget-halt", action="store_true", help="B2 terminal host budget errors before the optional full suite")
	args = parser.parse_args()
	record = "B2" if args.budget_halt else "B1" if args.budget else "CK2" if args.ck2 else "CK1"
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	started = time.monotonic()
	files = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
	pins = {}
	result = {"record": record, "status": "FAIL", "commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(), "source_pins": pins, "checks": []}
	env = dict(os.environ, CARGO_BUILD_JOBS="4", PYTHONDONTWRITEBYTECODE="1")
	def exact():
		for name, digest in pins.items():
			assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest, f"source changed during {record} job: {name}"
	def run(name, command):
		exact()
		with (out / (name + ".stdout")).open("x") as stdout, (out / (name + ".stderr")).open("x") as stderr:
			p = subprocess.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=3600)
		result["checks"].append({"name": name, "exit": p.returncode})
		exact()
		if p.returncode:
			raise RuntimeError(f"{name} failed ({p.returncode})")
	try:
		hygiene.controls()
		hygiene.index_controls()
		hygiene.check(ROOT)
		pins.update({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in files if (ROOT / name).is_file()})
		result["checks"].append({"name": "staged-artifact-guard", "exit": 0})
		for source in HERE.glob("*.py"):
			compile(source.read_text(), str(source), "exec")
			with source.open("rb") as stream:
				for token in tokenize.tokenize(stream.readline):
					if token.type == tokenize.INDENT and " " in token.string:
						raise ValueError(f"Python indentation must use tabs: {source}:{token.start[0]}")
		result["checks"].append({"name": "python-syntax-indentation", "exit": 0})
		if args.ck2:
			run("cli-cache-controls", ["python3", "-B", str(HERE / "test_cli_cache.py")])
			run("cli-capture-controls", ["python3", "-B", str(HERE / "test_cli_rules.py")])
		run("adr-records", ["python3", "-B", str(HERE / "records.py"), "--repo", str(ROOT)])
		run("fmt", ["cargo", "+1.98.1", "fmt", "--all", "--", "--check"])
		run("dependencies", ["python3", "-B", str(HERE / "dependencies.py")])
		# Tests exercise every public safe wrapper; no --extern links are allowed.
		run("isolated-primitives-build", ["rustc", "+1.98.1", "--edition=2021", "--test", "-Dwarnings", "crates/rune/src/experimental_column_primitives.rs", "-o", str(out / "primitive-tests")])
		run("isolated-primitives", [str(out / "primitive-tests")])
		run("clippy", ["cargo", "+1.98.1", "clippy", "--locked", "-p", "rune", "--all-features", "--lib", "--tests", "--", "-D", "warnings"])
		if args.ck2:
			run("clippy-xtask", ["cargo", "+1.98.1", "clippy", "--locked", "-p", "xtask", "--all-targets", "--", "-D", "warnings"])
		run("column-tests", ["cargo", "+1.98.1", "test", "--locked", "-p", "rune", "--all-features", "--lib", "experimental_core::column"])
		if args.budget:
			run("budget-tests", ["cargo", "+1.98.1", "test", "--locked", "-p", "rune", "--all-features", "--lib", "runtime::budget::tests"])
		if args.budget_halt:
			run("budget-halt-tests", ["cargo", "+1.98.1", "test", "--locked", "-p", "rune", "--all-features", "--lib", "runtime::vm_error::budget_tests"])
		if args.probe:
			manifest = str(HERE / "probe/Cargo.toml")
			run("probe-fmt", ["cargo", "+1.98.1", "fmt", "--manifest-path", manifest, "--", "--check"])
			run("probe-clippy", ["cargo", "+1.98.1", "clippy", "--locked", "--manifest-path", manifest, "--", "-D", "warnings"])
			run("probe-build", ["cargo", "+1.98.1", "build", "--locked", "--release", "--manifest-path", manifest])
			binary = HERE / "probe/target/release/ck1-column-probe"
			result["probe"] = {"path": str(binary), "sha256": hashlib.sha256(binary.read_bytes()).hexdigest()}
			operations = ("i64add", "axpy", "f64add", "boolnot", "f64pair", "mul3") if args.ck2 else ("i64add", "axpy", "f64add", "boolnot")
			for operation in operations:
				for rows in (0, 37, 4096, 1000000):
					for path in ("scalar", "auto"):
						run(f"probe-{operation}-{rows}-{path}", [str(binary), operation, str(rows), path])
		if args.full:
			run("full-library", ["cargo", "+1.98.1", "test", "--locked", "-p", "rune", "--all-features", "--lib"])
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - started
		receipt = out / "RESULT.json"
		receipt.write_text(json.dumps(result, indent=2) + "\n")
		notice = out / "NOTICE.txt"
		notice.write_text(f'{record} development prerequisites/focused controls EXIT {result["status"]}. Receipt: {receipt}\n')
		if args.notify:
			subprocess.run(["chatctl", "send", "--as", "sluice", "--to", "codex", "--conversation", "rune-native", "--summary", f'{record} development prerequisites/focused controls EXIT {result["status"]}', "--body-file", str(notice)], check=True)
		print(notice.read_text().strip())
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
