#!/usr/bin/env python3
"""ADR 0212 proportional controls with durable per-stage receipts and no retries."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "rune/tools/xtask/supplemental"))
from receipts import Run, atomic_json, digest


def source_identity():
	files = subprocess.check_output(["git", "ls-files", "--cached", "--others", "--exclude-standard"], cwd=ROOT, text=True).splitlines()
	return {name: digest(ROOT / name) for name in files if (ROOT / name).is_file()}


def commands(scope):
	manifest = ROOT / "rune/tools/xtask/manifests"
	native = next(row["features"] for row in json.loads((manifest / "native.json").read_text())["libraries"] if row["name"] == "native")
	miri = json.loads((manifest / "miri.json").read_text())
	base = ["cargo", "+1.98.1"]
	engine = ["--locked", "-p", "rune", "--profile", "engine-dev", "--features", native, "--lib"]
	rows = [
		("preflight", [sys.executable, "-B", "ci/preflight.py"], False),
		("format", base + ["fmt", "--all", "--check"], False),
		("engine-clippy", base + ["clippy"] + engine + ["--tests", "--", "-D", "warnings"], False),
	]
	if scope in {"core", "all"}:
		rows += [
			("column-tests", base + ["test"] + engine + ["experimental_core::column"], True),
			("primitive-tests", base + ["test"] + engine + ["experimental_column_primitives"], True),
		]
	if scope in {"host", "all"}:
		rows += [
			("host-clippy", [sys.executable, "-B", "tools/column/clippy.py", "--out", "{attempt}/clippy"], False),
			("host-tests", base + ["test", "--locked", "--no-default-features", "--lib", "column::tests"], True),
		]
	if scope in {"miri", "all"}:
		for name, selector in [("miri-borrows", "experimental_core::column::tests::borrowed_"), ("miri-views", "experimental_column_primitives::borrowed_view_tests"), ("miri-integer-operands", "experimental_core::column::run::operands::tests")]:
			rows.append((name, ["cargo", "+" + miri["toolchain"], "miri", "test", "--locked", "-p", "rune", "--profile", "engine-dev", "--features", miri["features"], "--lib", selector], True))
	return rows


def positive_tests(log):
	counts = re.findall(r"test result: ok\. (\d+) passed;", log)
	return bool(counts) and sum(map(int, counts)) > 0


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--out", required=True, type=Path)
	parser.add_argument("--scope", choices=["core", "host", "miri", "all"], default="all")
	parser.add_argument("--resume", action="store_true")
	parser.add_argument("--notify-chatd", action="store_true")
	args = parser.parse_args()
	out = args.out.resolve()
	result = {"status": "FAIL", "scope": args.scope, "stages": []}
	try:
		subprocess.run([sys.executable, "-B", str(ROOT / "ci/disk_space.py"), "--path", str(ROOT), "--path", str(out)], check=True)
		stages = commands(args.scope)
		pins = source_identity()
		env = dict(os.environ, CARGO_BUILD_JOBS="4", CARGO_INCREMENTAL="0", PYTHONDONTWRITEBYTECODE="1")
		description = {"sources": pins, "commands": stages, "scope": args.scope, "head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(), "environment": {key: env.get(key) for key in ["RUSTFLAGS", "RUSTDOCFLAGS", "MIRIFLAGS", "CARGO_TARGET_DIR", "CARGO_BUILD_JOBS", "CARGO_INCREMENTAL"]}, "toolchains": subprocess.check_output(["rustup", "toolchain", "list"], text=True)}
		description = json.loads(json.dumps(description))
		with Run(out, description, args.resume) as run:
			for name, command, tests in stages:
				if source_identity() != pins:
					raise RuntimeError("source changed before " + name)
				attempt, reused = run.begin(name)
				if reused is not None:
					result["stages"].append(reused)
					continue
				command = [part.replace("{attempt}", str(attempt)) for part in command]
				log = attempt / "output.log"
				start = time.monotonic()
				with log.open("x") as stream:
					try:
						child = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=7200)
					except (subprocess.TimeoutExpired, OSError) as error:
						stream.write("\nLaunch/timeout failure: " + repr(error) + "\n")
						child = subprocess.CompletedProcess(command, 124)
				passed = child.returncode == 0 and source_identity() == pins and (not tests or positive_tests(log.read_text()))
				row = run.finish(attempt, "PASS" if passed else "FAIL", [log], command=command, exit=child.returncode, seconds=time.monotonic() - start)
				result["stages"].append(row)
				if not passed:
					raise RuntimeError(name + " failed")
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
	finally:
		out.mkdir(parents=True, exist_ok=True)
		receipt = out / ("INVOCATION-" + uuid.uuid4().hex + ".json")
		atomic_json(receipt, result)
		if args.notify_chatd:
			notice = receipt.with_suffix(".txt")
			notice.write_text(str(receipt) + "\n")
			subprocess.run(["/home/me/.local/bin/chatctl", "send", "--as", "sluice", "--to", "codex", "--conversation", "rune-native", "--summary", "ADR0212 " + args.scope + " controls EXIT " + result["status"], "--body-file", str(notice)], check=True)
		print(result["status"], receipt)
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
