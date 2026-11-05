"""Rescore immutable ADR0211 captures; never rerun or discard failed measurements."""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
import sys
from migration_rules import evaluate, BASELINE, CANDIDATE
from ir_pool import PIN, terminal_json


def main():
	p = argparse.ArgumentParser()
	for name in ("capture", "builds", "proof", "rune", "out"):
		p.add_argument("--" + name, type=Path, required=True)
	a = p.parse_args()
	assert not a.out.exists(), "new receipt required"
	build = json.loads(a.builds.read_text())
	capture = json.loads(a.capture.read_text())
	assert capture["identity"]["builds"] == hashlib.sha256(a.builds.read_bytes()).hexdigest()
	assert build["status"] == "PASS"
	roles = {r["role"]: r for r in build["builds"]}
	assert roles["baseline"]["source_commit"] == BASELINE and roles["candidate"]["source_commit"] == CANDIDATE
	for row in roles.values():
		assert hashlib.sha256(Path(row["binary"]).read_bytes()).hexdigest() == row["sha256"]
	assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=a.rune, text=True).strip() == PIN
	sys.path.insert(0, str(a.rune / "tools/xtask/column"))
	import cli_rules
	policy = json.loads(Path(__file__).with_name("CK2.json").read_text())
	result = evaluate(capture["records"], policy, cli_rules.evaluate, BASELINE, CANDIDATE,
		a.proof, {role: row["sha256"] for role, row in roles.items()})
	result["inputs"] = {str(f): hashlib.sha256(f.read_bytes()).hexdigest() for f in (a.capture, a.builds, a.proof, Path(__file__), Path(__file__).with_name("migration_rules.py"))}
	terminal_json(a.out, result)
	print(result["status"], a.out)
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
