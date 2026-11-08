#!/usr/bin/env python3
"""Ordered 0205 prerequisites and controls; terminal receipts, no job polling."""
import child_result
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import time
import tokenize
import traceback

ROOT = Path(__file__).resolve().parents[2]


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--out", required=True, type=Path)
	parser.add_argument("--scope", choices=("host", "adapter", "all"), default="all")
	parser.add_argument("--cli", action="store_true", help="build and check the real adapter CLI after all prerequisites")
	parser.add_argument("--release", action="store_true", help="use the production release profile for --cli")
	parser.add_argument("--remote", help="preflight BatchMode access before remote gates")
	parser.add_argument("--after-pid", type=int, help="block on this already-known job's exit; never poll it")
	parser.add_argument("--after-receipt", type=Path, help="require this predecessor's terminal PASS")
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	start = time.monotonic()
	env = dict(os.environ, CARGO_BUILD_JOBS="4", PYTHONDONTWRITEBYTECODE="1")
	result = {"acceptance_scope": "development", "record": "0205", "status": "FAIL", "checks": [], "source_pins": {}}
	pins = result["source_pins"]
	def exact():
		for name, digest in pins.items():
			assert hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == digest, f"source changed: {name}"
	def run(name, command):
		exact()
		with (out / (name + ".stdout")).open("x") as stdout, (out / (name + ".stderr")).open("x") as stderr:
			completed = child_result.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=7200)
		result["checks"].append({"name": name, "exit": completed.returncode, "validation": completed.validation})
		exact()
		if completed.returncode:
			raise RuntimeError(f"{name} failed: {completed.returncode}")
	try:
		assert not args.cli or args.scope != "host", "CLI controls require adapter prerequisites"
		assert not args.release or args.cli, "--release requires --cli"
		for executable in ("cargo", "git", "python3"):
			assert shutil.which(executable), f"missing executable: {executable}"
		result["commit"] = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
		files = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
		pins.update({name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest() for name in files if (ROOT / name).is_file()})
		if args.after_pid:
			assert args.after_receipt, "an exit dependency needs its terminal receipt"
			subprocess.run(["tail", "--pid=" + str(args.after_pid), "-f", "/dev/null"], check=True)
		if args.after_receipt:
			data = args.after_receipt.read_bytes()
			result["predecessor"] = {"path": str(args.after_receipt), "sha256": hashlib.sha256(data).hexdigest()}
			assert json.loads(data)["status"] == "PASS", "predecessor failed"
			exact()
		for source in Path(__file__).parent.glob("*.py"):
			compile(source.read_text(), str(source), "exec")
			with source.open("rb") as stream:
				for token in tokenize.tokenize(stream.readline):
					assert token.type != tokenize.INDENT or " " not in token.string, f"Python indentation: {source}:{token.start[0]}"
		if args.remote:
			assert env.get("SSH_AUTH_SOCK") and Path(env["SSH_AUTH_SOCK"]).exists(), "missing SSH agent socket"
			run("remote-preflight", ["ssh", "-oBatchMode=yes", "-oConnectTimeout=15", args.remote, "true"])
		spec = importlib.util.spec_from_file_location("rnx_verify", ROOT / "tools/verify.py")
		verify = importlib.util.module_from_spec(spec)
		spec.loader.exec_module(verify)
		result["rune_pins"] = verify.check_rune_pins(ROOT)
		run("controller-controls", ["python3", "-B", "-m", "unittest", "discover", "-s", "tools/column", "-p", "test_*.py", "-v"])
		run("fmt", ["cargo", "fmt", "--all", "--", "--check"])
		if args.scope != "host":
			run("adapter-fmt", ["cargo", "fmt", "--manifest-path", "adapters/polars/Cargo.toml", "--", "--check"])
		# No test runs until every selected Clippy invocation passes.
		run("host-clippy", ["python3", "-B", "tools/column/clippy.py", "--out", str(out / "clippy")])
		if args.scope != "host":
			run("adapter-clippy", ["python3", "-B", "tools/column/clippy.py", "--adapter", "--out", str(out / "adapter-clippy")])
		run("host-tests", ["cargo", "test", "--locked", "--no-default-features", "--lib", "column::"])
		run("routing-inventory", ["cargo", "test", "--locked", "--features", "project-sources", "--lib", "routing_inventory"])
		if args.scope != "host":
			run("adapter-unit-tests", ["cargo", "test", "--locked", "--manifest-path", "adapters/polars/Cargo.toml", "--features", "test-support", "--lib", "column::"])
			run("adapter-tests", ["cargo", "test", "--locked", "--manifest-path", "adapters/polars/Cargo.toml", "--features", "test-support", "--test", "column"])
		if args.cli:
			manifest = "adapters/polars/Cargo.toml"
			metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--no-deps", "--format-version", "1", "--manifest-path", manifest], cwd=ROOT, env=env, text=True))
			command = ["cargo", "build", "--locked", "--manifest-path", manifest, "--bin", "rnx-polars"]
			if args.release:
				command.append("--release")
			run("cli-build", command)
			binary = Path(metadata["target_directory"]) / ("release" if args.release else "debug") / "rnx-polars"
			run("cli-controls", ["python3", "-B", "tools/column/cli.py", "--binary", str(binary), "--out", str(out / "cli")])
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - start
		receipt = out / "RESULT.json"
		child_result.publish(receipt, result)
		notice = out / "NOTICE.txt"
		notice.write_text(f'0205 {args.scope} development prerequisites/controls EXIT {result["status"]}. Receipt: {receipt}\n')
		print(notice.read_text().strip())
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
