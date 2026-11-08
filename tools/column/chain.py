#!/usr/bin/env python3
"""Run declared gate stages from one pinned input manifest; block until each exits."""
import gate_policy
import child_result
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import time
import traceback


def pin(path):
	path = Path(path)
	data = path.read_bytes()
	return {"path": str(path), "sha256": hashlib.sha256(data).hexdigest(), "bytes": len(data)}


def check(reference):
	actual = pin(reference["path"])
	assert all(actual[key] == value for key, value in reference.items()), f"artifact changed: {reference['path']}"
	return actual


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--config", type=Path, required=True)
	parser.add_argument("--out", type=Path, help="explicit binding for a parent controller; must equal config directory")
	args = parser.parse_args()
	config = json.loads(args.config.read_text())
	config_pin = pin(args.config)
	out = args.config.parent
	if args.out is not None and args.out.resolve() != out.resolve():
		raise ValueError("chain output must equal config directory")
	assert not (out / "RESULT.json").exists(), "terminal receipt already exists"
	result = {"status": "FAIL", "acceptance_scope": config.get("acceptance_scope", "development"), "candidate": config["candidate"], "tip": config["tip"], "checks": [], "config": config_pin}
	started = time.monotonic()
	def exact():
		check(config_pin)
		assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=config["repo"], text=True).strip() == config["candidate"]
		assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=config["repo"], text=True).strip()
		for reference in config["controllers"]:
			check(reference)
	try:
		exact()
		selection = config.get("selection")
		if result["acceptance_scope"] not in {"development", "final"}: raise ValueError("unknown chain scope")
		if result["acceptance_scope"] == "final":
			gate_policy.check_selection(selection, config["repo"])
		for reference in config["prerequisites"]:
			check(reference)
			assert json.loads(Path(reference["path"]).read_text())["status"] == "PASS", reference["path"]
		check(config["builds_receipt"])
		builds = json.loads(Path(config["builds_receipt"]["path"]).read_text())
		assert builds["status"] == "PASS" and builds["candidate"] == config["candidate"] and builds["tip"] == config["tip"]
		assert set(builds["paths"]) == {role + "-cgu1-" + mode for role in ("tip", "candidate") for mode in ("on", "off")}
		for name, path in builds["paths"].items():
			build = json.loads(Path(path).read_text())
			role = name.split("-")[0]
			assert build["status"] == "PASS" and build["source"]["commit"] == config[role] and build["role"] == role
			check(build["binary"])
		result["reused_builds"] = config["builds_receipt"]
		for stage in config["stages"]:
			assert not Path(stage["receipt"]).exists(), "stage output already exists"
			assert not (out / (stage["name"] + ".stdout")).exists(), "stage log already exists"
		for stage in config["stages"]:
			exact()
			if stage.get("cooldown_seconds"):
				time.sleep(stage["cooldown_seconds"])
			with (out / (stage["name"] + ".stdout")).open("x") as stdout, (out / (stage["name"] + ".stderr")).open("x") as stderr:
				completed = child_result.run(stage["command"], root=config["repo"], cwd=config["repo"], stdout=stdout, stderr=stderr, timeout=stage["timeout"])
			row = {"name": stage["name"], "exit": completed.returncode, "validation": completed.validation}
			result["checks"].append(row)
			if Path(stage["receipt"]).exists():
				row["receipt"] = pin(stage["receipt"])
			assert completed.returncode == 0, stage["name"]
			assert json.loads(Path(stage["receipt"]).read_text())["status"] == "PASS", stage["name"]
			exact()
		if result["acceptance_scope"] == "final": result["finalization"] = gate_policy.finalize(selection, {r["name"]: r["validation"] for r in result["checks"]}, config["repo"])
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - started
		receipt = out / "RESULT.json"
		child_result.publish(receipt, result)
		notice = out / "NOTICE.txt"
		summary = config["label"] + " EXIT " + result["status"]
		notice.write_text(summary + ". Receipt " + str(receipt) + ".\n")
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
