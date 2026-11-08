#!/usr/bin/env python3
"""Actual CLI routing/fuel controls; called after ordered prerequisites/build."""
import child_result
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import traceback

ROOT = Path(__file__).resolve().parents[2]


def validate(code, stdout, stderr, exhausted, stock):
	assert "rnx: engine=legacy" in stderr, "outer script did not select stock VM"
	assert "rnx: engine=execution" not in stderr, "opaque script entered execution engine"
	assert "COLUMN_ENTER" in stdout, "never reached the map call"
	assert "COLUMN_CAUGHT" not in stdout, "fuel halt became a catchable Result"
	if exhausted:
		assert code != 0, "fuel exhaustion returned success"
		assert "COLUMN_OK" not in stdout, "completed despite insufficient fuel"
		assert "work units" in stderr and "--budget" in stderr, "missing CLI fuel diagnostic"
	else:
		assert code == 0, "successful control failed"
		assert "COLUMN_OK" in stdout, "output equality/length checks never completed"
		assert "completed=true; rows=10000; valid_rows=10000;" in stdout
		if stock:
			assert "path=scalar-fallback;" in stdout
			assert "attempts=0; replays=0;" in stdout
		else:
			assert "path=avx2;" in stdout or "path=scalar-primitives;" in stdout
			assert "attempts=1; replays=0; refusal=None" in stdout


def controls():
	good = "COLUMN_ENTER\nCOLUMN_OK path=avx2; completed=true; rows=10000; valid_rows=10000; attempts=1; replays=0; refusal=None"
	legacy = "rnx: engine=legacy refusal=HostContract"
	validate(0, good, legacy, False, False)
	validate(1, "COLUMN_ENTER", legacy + " 2000 work units exceeded; --budget N raises it", True, False)
	for code, stdout, stderr, exhausted in (
		(0, good, "rnx: engine=execution", False),
		(0, good.replace("rows=10000", "rows=0"), legacy, False),
		(0, good.replace("COLUMN_OK", "SKIPPED"), legacy, False),
		(0, good.replace("path=avx2", "path=scalar-fallback"), legacy, False),
		(0, "COLUMN_ENTER\nCOLUMN_CAUGHT fuel", legacy, True),
		(1, "", legacy + "work units --budget", True),
	):
		try:
			validate(code, stdout, stderr, exhausted, False)
		except AssertionError:
			continue
		raise AssertionError("invalid CLI evidence passed the checker")


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--binary", required=True, type=Path)
	parser.add_argument("--out", required=True, type=Path)
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	start = time.monotonic()
	result = {"status": "FAIL", "cases": []}
	try:
		controls()
		binary = args.binary.resolve(strict=True)
		result["binary"] = str(binary)
		result["binary_sha256"] = hashlib.sha256(binary.read_bytes()).hexdigest()
		for engine in ("default", "legacy"):
			for stock in (False, True):
				fixture = Path(__file__).parent / "fixtures" / ("cli-stock.rn" if stock else "cli-map.rn")
				for work in (2000, 1000000):
					name = f'{engine}-{"stock" if stock else "primitives"}-{work}'
					command = [str(binary), "run", "--debug-engine", "--budget", str(work)]
					if engine == "legacy":
						command.append("--engine=legacy")
					command.append(str(fixture.resolve()))
					done = subprocess.run(command, cwd=ROOT, capture_output=True, text=True, timeout=120)
					(out / (name + ".stdout")).write_text(done.stdout)
					(out / (name + ".stderr")).write_text(done.stderr)
					result["cases"].append({"name": name, "command": command, "exit": done.returncode, "fixture_sha256": hashlib.sha256(fixture.read_bytes()).hexdigest()})
					validate(done.returncode, done.stdout, done.stderr, work == 2000, stock)
		assert hashlib.sha256(binary.read_bytes()).hexdigest() == result["binary_sha256"]
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - start
		child_result.publish(out / "RESULT.json", result)
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
