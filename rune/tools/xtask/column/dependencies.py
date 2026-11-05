#!/usr/bin/env python3
"""CK1: reject unreviewed normal/build dependencies and feature growth."""
import argparse
import json
import pathlib
import subprocess
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[3]
BASELINE = pathlib.Path(__file__).with_name("dependencies.json")
MATRIX = [(target, features, defaults) for target in ("x86_64-unknown-linux-gnu", "aarch64-unknown-linux-gnu") for features, defaults in (("execution", False), ("execution", True), ("native-code", True))]


def closure(metadata):
	packages = {p["id"]: p for p in metadata["packages"]}
	nodes = {p["id"]: p for p in metadata["resolve"]["nodes"]}
	root = next(k for k, p in packages.items() if p["name"] == "rune")
	seen, pending = set(), [root]
	while pending:
		key = pending.pop()
		if key in seen:
			continue
		seen.add(key)
		for dep in nodes[key]["deps"]:
			if any(d["kind"] in (None, "build") for d in dep["dep_kinds"]):
				pending.append(dep["pkg"])
	result = []
	for key in seen:
		p = packages[key]
		result.append({"name": p["name"], "version": p["version"], "source": p["source"] or "workspace", "features": sorted(nodes[key]["features"])})
	return sorted(result, key=lambda p: (p["name"], p["version"], p["source"]))


def manifest():
	data = tomllib.loads((ROOT / "crates/rune/Cargo.toml").read_text())
	return {"dependencies": data["dependencies"], "features": data["features"], "target": data.get("target", {})}


def capture():
	rows = []
	for target, features, defaults in MATRIX:
		command = ["cargo", "+1.98.1", "metadata", "--locked", "--format-version", "1", "--manifest-path", str(ROOT / "crates/rune/Cargo.toml"), "--filter-platform", target, "--features", features]
		if not defaults:
			command.append("--no-default-features")
		data = json.loads(subprocess.check_output(command, cwd=ROOT, text=True))
		rows.append({"target": target, "features": features, "default_features": defaults, "closure": closure(data)})
	return {"record": "CK1", "exceptions": [], "manifest": manifest(), "matrix": rows}


def check(expected, actual):
	if expected != actual:
		raise ValueError("dependency/feature set changed; an explicit reviewed CK1 exception and baseline amendment are required")


def controls():
	import copy
	fixture = {"packages": [{"id": "r", "name": "rune", "version": "1", "source": None}, {"id": "d", "name": "test-only", "version": "1", "source": None}], "resolve": {"nodes": [{"id": "r", "features": [], "deps": [{"pkg": "d", "dep_kinds": [{"kind": "dev"}]}]}, {"id": "d", "features": [], "deps": []}]}}
	base = closure(fixture)
	assert len(base) == 1
	for change in ("normal", "build", "feature"):
		altered = copy.deepcopy(fixture)
		if change == "feature":
			altered["resolve"]["nodes"][0]["features"].append("unapproved")
		else:
			altered["resolve"]["nodes"][0]["deps"][0]["dep_kinds"][0]["kind"] = None if change == "normal" else "build"
		try:
			check(base, closure(altered))
		except ValueError:
			pass
		else:
			raise AssertionError(f"negative control accepted {change}")


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--capture-new", type=pathlib.Path, help="write a NEW review artifact; never replaces the accepted baseline")
	args = parser.parse_args()
	controls()
	actual = capture()
	if args.capture_new:
		with args.capture_new.open("x") as out:
			json.dump(actual, out, indent=2, sort_keys=True)
			out.write("\n")
	else:
		check(json.loads(BASELINE.read_text()), actual)
	print("PASS: CK1 dependency closure and negative controls")


if __name__ == "__main__":
	main()
