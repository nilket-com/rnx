#!/usr/bin/env python3
"""Fail on any Clippy diagnostic absent from the exact published rnx source."""
import child_result
import argparse
from collections import Counter
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile

ROOT = Path(__file__).resolve().parents[2]
BASE = "c32a6ba77e8318a8c763b0c2eaac336cdebfde48"


def diagnostics(path, root):
	found = Counter()
	for line in path.read_text().splitlines():
		try:
			item = json.loads(line)
		except json.JSONDecodeError:
			continue
		if item.get("reason") != "compiler-message":
			continue
		message = item["message"]
		if message["level"] not in ("warning", "error"):
			continue
		# Code movement is not a new diagnostic. Keep file/lint/message identity,
		# and normalize absolute paths from clean exports before comparison.
		spans = tuple(sorted({str(Path(span["file_name"]).relative_to(root)) if Path(span["file_name"]).is_absolute() and Path(span["file_name"]).is_relative_to(root) else span["file_name"] for span in message["spans"] if span["is_primary"]}))
		found[(item["target"]["name"], message["level"], (message.get("code") or {}).get("code"), message["message"], spans)] += 1
	return found


def controls():
	with tempfile.TemporaryDirectory() as directory:
		root = Path(directory)
		path = root / "diagnostics.jsonl"
		item = {"reason": "compiler-message", "target": {"name": "rnx"}, "message": {"level": "warning", "code": {"code": "clippy::example"}, "message": "example", "spans": [{"file_name": "src/example.rs", "is_primary": True, "line_start": 1, "column_start": 1}]}}
		path.write_text(json.dumps(item) + "\n")
		before = diagnostics(path, root)
		item["message"]["spans"][0].update(file_name=str(root / "src/example.rs"), line_start=17)
		path.write_text(json.dumps(item) + "\n")
		assert diagnostics(path, root) == before, "path/line normalization changed a diagnostic"
		path.write_text((json.dumps(item) + "\n") * 2)
		assert diagnostics(path, root) - before, "a second identical diagnostic escaped the guard"
		item["message"]["message"] = "changed"
		path.write_text(json.dumps(item) + "\n")
		assert diagnostics(path, root) - before, "a changed diagnostic escaped the guard"


def main():
	controls()
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--out", type=Path, required=True)
	parser.add_argument("--adapter", action="store_true")
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	base = out / "published"
	base.mkdir()
	archive = out / "published.tar"
	with archive.open("xb") as stream:
		subprocess.run(["git", "archive", BASE], cwd=ROOT, stdout=stream, check=True)
	with tarfile.open(archive) as packed:
		packed.extractall(base, filter="data")
	env = dict(os.environ, CARGO_BUILD_JOBS="4")
	sets = {}
	for role, directory in (("published", base), ("candidate", ROOT)):
		env["CARGO_TARGET_DIR"] = str(ROOT / "target" / ("column-clippy-" + ("adapter-" if args.adapter else "") + role))
		with (out / (role + ".jsonl")).open("x") as stdout, (out / (role + ".stderr")).open("x") as stderr:
			command = ["cargo", "clippy", "--locked", "--lib", "--tests", "--message-format=json"]
			if args.adapter:
				command += ["--manifest-path", "adapters/polars/Cargo.toml", "--features", "test-support"]
				# Review 26075: retain the existing preview test diagnostic in the
				# comparison instead of letting its default deny stop the baseline.
				# New column modules still deny warnings and all Clippy lints.
				command += ["--", "-Wclippy::erasing_op"]
			else:
				command += ["--no-default-features"]
			completed = subprocess.run(command, cwd=directory, env=env, stdout=stdout, stderr=stderr, timeout=7200)
		sets[role] = diagnostics(out / (role + ".jsonl"), directory)
		if completed.returncode:
			raise RuntimeError(f"{role} Clippy failed; see {out}")
	added = sets["candidate"] - sets["published"]
	result = {"status": "FAIL" if added else "PASS", "published": BASE, "existing": sorted(sets["published"].items(), key=repr), "candidate": sorted(sets["candidate"].items(), key=repr), "added": sorted(added.items(), key=repr)}
	child_result.publish(out / "RESULT.json", result)
	if added:
		raise RuntimeError(f"new Clippy diagnostics: {added!r}")


if __name__ == "__main__":
	main()
