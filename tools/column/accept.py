#!/usr/bin/env python3
"""0205 fresh-clone acceptance: inherited gates plus column and production CLI controls."""
import gate_policy
import child_result
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import traceback

from stage_outputs import finish_stage

ROOT = Path(__file__).resolve().parents[2]
MANIFESTS = ["Cargo.toml", "tools/project/Cargo.toml", "adapters/candle/Cargo.toml",
	"adapters/polars/Cargo.toml", "adapters/postgres/Cargo.toml", "demos/candle/Cargo.toml",
	"demos/polars/Cargo.toml", "servers/http-postgres/Cargo.toml"]


# Inputs produced by another canonical stage, not supplied by the checkout.
# Selection closes transitively over this relation and preserves command order.
PREREQUISITES = {
	'polars-suite': ('polars-inventories',),
	'suite-adapter-polars': ('polars-inventories',),
	'production-cli-controls': ('production-cli-build',),
}


def commands(out, root=ROOT):
	# The verifier only permits ignored in-tree output below this directory.
	cross_out = root / "target/rnx-verify" / (out.name + "-" + hashlib.sha256(str(out.resolve()).encode()).hexdigest()[:16])
	cargo = ["cargo", "+1.98.1"]
	steps = [("commit-discipline", ["python3", "-B", "ci/preflight.py"])]
	steps += [("fetch-" + m.replace("/", "-"), cargo + ["fetch", "--locked", "--manifest-path", m]) for m in MANIFESTS]
	steps += [
		("controller-controls", ["python3", "-B", "-m", "unittest", "discover", "-s", "tools/column", "-p", "test_*.py", "-v"]),
		("polars-inventory-preflight", ["python3", "-B", "tools/column/inventories.py", "--preflight", "--out", str(out / "polars-inventory-preflight")]),
		("format", cargo + ["fmt", "--all", "--check"]),
		("adapter-format", cargo + ["fmt", "--manifest-path", "adapters/polars/Cargo.toml", "--", "--check"]),
		("pin-controls", ["python3", "-I", "-B", "tools/test_verify_pins.py"]),
		("benchmark-protocol", ["python3", "-B", "-m", "unittest", "discover", "-s", "tools/routing-bench", "-p", "test_*.py", "-v"]),
		("clippy", ["python3", "-B", "tools/column/clippy.py", "--out", str(out / "clippy")]),
		("adapter-clippy", ["python3", "-B", "tools/column/clippy.py", "--adapter", "--out", str(out / "adapter-clippy")]),
		("inventory", cargo + ["test", "--locked", "--features", "project-sources", "--lib", "routing_inventory", "--", "--test-threads=4"]),
		("column-host", cargo + ["test", "--locked", "--no-default-features", "--lib", "column::"]),
		("column-adapter-unit", cargo + ["test", "--locked", "--manifest-path", "adapters/polars/Cargo.toml", "--features", "test-support", "--lib", "column::"]),
		("column-adapter-integration", cargo + ["test", "--locked", "--manifest-path", "adapters/polars/Cargo.toml", "--features", "test-support", "--test", "column"]),
		("root-tests", cargo + ["test", "--no-fail-fast", "--locked", "--features", "project-sources", "--", "--test-threads=4"]),
		("async-test-support", cargo + ["test", "--locked", "--features", "project-sources,test-support", "--test", "async_execution", "--", "--test-threads=4"]),
		("generated-projects", cargo + ["test", "--locked", "--features", "project-sources", "--test", "routing_projects", "--", "--ignored", "--test-threads=1"]),
	]
	steps += [
		('jupyter-format', cargo + ['fmt', '--manifest-path', 'jupyter/Cargo.toml', '--check']),
		('jupyter-clippy', cargo + ['clippy', '--locked', '--manifest-path', 'jupyter/Cargo.toml', '--all-targets', '--features', 'transport-probe', '--', '-D', 'warnings']),
		('jupyter-tests', cargo + ['test', '--locked', '--manifest-path', 'jupyter/Cargo.toml', '--features', 'transport-probe']),
		('jupyter-kernel', ['python3', '-B', 'tools/column/jupyter_controls.py', '--out', str(out/'jupyter-kernel')]),
	]
	for adapter in ("candle", "polars", "postgres"):
		steps.append(("adapter-" + adapter, cargo + ["test", "--locked", "--manifest-path", f"adapters/{adapter}/Cargo.toml", "--test", "execution_routing", "--", "--test-threads=1"]))
	steps.append(("polars-inventories", ["python3", "-B", "tools/column/inventories.py", "--out", str(out / "polars-inventories")]))
	steps.append(("polars-suite", cargo + ["test", "--locked", "--manifest-path", "adapters/polars/Cargo.toml", "--features", "test-support", "--", "--test-threads=4"]))
	for manifest in MANIFESTS[-3:]:
		steps.append(("check-" + manifest.replace("/", "-"), cargo + ["check", "--locked", "--all-targets", "--manifest-path", manifest]))
	steps += [
		("cross", ["python3", "-B", "tools/verify.py", "cross", "--suite", "all", "--jobs", "4", "--out", str(cross_out)]),
		("production-cli-build", cargo + ["build", "--locked", "--release", "--manifest-path", "adapters/polars/Cargo.toml", "--bin", "rnx-polars"]),
		("production-cli-controls", ["python3", "-B", "tools/column/cli.py", "--binary", str(root / "adapters/polars/target/release/rnx-polars"), "--out", str(out / "cli")]),
	]
	for name in ('rune-suite', 'rune-clippy', 'project-suite', 'project-clippy',
		'suite-adapter-candle', 'suite-adapter-polars', 'suite-adapter-postgres',
		'clippy-adapter-candle', 'clippy-adapter-polars', 'clippy-adapter-postgres'):
		steps.append((name, ['python3', '-B', 'tools/column/suite_stage.py', '--stage', name, '--out', str(out/name)]))
	from suite_stage import PACKAGE_CHECKS
	for name, _ in PACKAGE_CHECKS.values():
		steps.append((name, ['python3', '-B', 'tools/column/suite_stage.py', '--stage', name, '--out', str(out/name)]))
	steps.append(('fixed-route', ['python3', '-B', 'tools/column/fixed_route_gate.py', '--out', str(out/'fixed-route')]))
	assert len({name for name, _ in steps}) == len(steps)
	return steps


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--candidate", required=True)
	parser.add_argument("--out", required=True, type=Path)
	parser.add_argument("--remote-context", type=Path)
	parser.add_argument("--plan", action="store_true", help="emit commands without running any jobs")
	args = parser.parse_args()
	out = args.out.resolve()
	steps = commands(out)
	if args.plan:
		print(json.dumps(steps, indent=2))
		return 0
	out.mkdir(parents=True, exist_ok=False)
	start = time.monotonic()
	result = {"status": "FAIL", "candidate": args.candidate, "checks": []}
	env = dict(os.environ, CARGO_BUILD_JOBS="4", PYTHONDONTWRITEBYTECODE="1")
	def exact():
		assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() == args.candidate
		assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip(), "source changed"
	try:
		subprocess.run(["python3", "-B", str(ROOT / "ci/disk_space.py"), "--path", str(ROOT), "--path", str(out)], check=True)
		exact()
		if args.remote_context:
			context = json.loads(args.remote_context.read_text())
			if context["candidate"] != args.candidate or context["source"] != child_result.source(ROOT):
				raise ValueError("remote source context differs")
			result["remote_context"] = context
		selection = gate_policy.select(ROOT, out, candidate=args.candidate)
		child_result.atomic_json(out / "SELECTION.json", selection)
		assert not env.get("CARGO_TARGET_DIR"), "fresh acceptance requires native workspace targets"
		for manifest in MANIFESTS:
			assert not (ROOT / manifest).parent.joinpath("target").exists(), "fresh target already exists"
		for name, command in steps:
			exact()
			before = time.monotonic()
			with (out / (name + ".stdout")).open("x") as stdout, (out / (name + ".stderr")).open("x") as stderr:
				done = child_result.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=10800, after=lambda: finish_stage(ROOT, name, out))
			result["checks"].append({"name": name, "exit": done.returncode, "command": command, "seconds": time.monotonic() - before, "validation": done.validation})
			output = done.declared_output
			if output is not None:
				result["checks"][-1]["declared_output"] = output
			exact()
			assert done.returncode == 0, f"{name} failed"
		result["finalization"] = gate_policy.finalize(selection, {r["name"]: r["validation"] for r in result["checks"]}, ROOT)
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - start
		result["controller_sha256"] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
		child_result.publish(out / "RESULT.json", result)
	print(result["status"], str(out / "RESULT.json"))
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
