#!/usr/bin/env python3
"""Reject private tooling/locations; grandfather only exact historical doc blobs."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

RULES = (("agent executable", b"chat" + b"ctl"), ("absolute home path", b"/" + b"home/"))
HISTORICAL = ("plans/", "probes/", "rune/plans/")


def blob(data):
	return hashlib.sha1(b"blob " + str(len(data)).encode() + b"\0" + data).hexdigest()


def violations(data):
	return [name for name, token in RULES if token in data]


def scan(root, exemptions, revision=None, indexed=False):
	root = Path(root)
	def git(*args):
		return subprocess.check_output(["git", "-C", str(root), *args])
	if revision:
		paths = git("ls-tree", "-r", "--name-only", "-z", revision).split(b"\0")
	else:
		paths = git("ls-files", "-z").split(b"\0")
	failures = []
	for raw in filter(None, paths):
		path = raw.decode("utf-8", "surrogateescape")
		if revision or indexed:
			data = git("show", (revision or "") + ":" + path)
		else:
			p = root / path
			if not p.exists() and not p.is_symlink():
				continue  # A tracked deletion cannot introduce forbidden content.
			data = str(p.readlink()).encode() if p.is_symlink() else p.read_bytes()
		bad = violations(raw) + violations(data)
		if not bad:
			continue
		if path.startswith(HISTORICAL) and exemptions.get(path) == blob(data):
			continue
		failures.append({"path": path, "rules": sorted(set(bad))})
	return failures


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
	mode = p.add_mutually_exclusive_group()
	mode.add_argument("--revision")
	mode.add_argument("--index", action="store_true")
	args = p.parse_args()
	manifest = args.repo / "ci/hygiene_history.json"
	if args.revision or args.index:
		data = json.loads(subprocess.check_output(["git", "-C", str(args.repo), "show", (args.revision or "") + ":ci/hygiene_history.json"]))
	else:
		data = json.loads(manifest.read_text())
	exemptions = data["documents"]
	if any(not k.startswith(HISTORICAL) or not re.fullmatch("[0-9a-f]{40}", v) for k, v in exemptions.items()):
		raise ValueError("invalid historical-document exemption")
	for path, expected in exemptions.items():
		original = subprocess.check_output(["git", "-C", str(args.repo), "show", data["baseline"] + ":" + path])
		if blob(original) != expected or not violations(original):
			raise ValueError("historical exemption does not match baseline: " + path)
	failures = scan(args.repo, exemptions, args.revision, args.index)
	import data_hygiene
	failures += [dict(path=p, rules=[reason]) for p, reason in data_hygiene.scan(args.repo, args.revision, args.index)]
	print(json.dumps({"status": "FAIL" if failures else "PASS", "failures": failures}, indent=2))
	return bool(failures)


if __name__ == "__main__":
	raise SystemExit(main())
