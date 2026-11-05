"""Durable case commits and exact-input resume; incomplete work is never a hit."""
import hashlib
import json
import os
from pathlib import Path
import tempfile


def digest(path):
	return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def atomic_json(path, value):
	"""Commit data, then its directory entry, before reporting completion."""
	path = Path(path)
	path.parent.mkdir(parents=True, exist_ok=True)
	fd, temporary = tempfile.mkstemp(prefix="." + path.name, dir=path.parent)
	try:
		with os.fdopen(fd, "w") as stream:
			json.dump(value, stream, indent=2)
			stream.write("\n")
			stream.flush()
			os.fsync(stream.fileno())
		os.replace(temporary, path)
		directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
		try:
			os.fsync(directory)
		finally:
			os.close(directory)
	finally:
		if os.path.exists(temporary):
			os.unlink(temporary)


def commit_case(case, record, identity, *, imported=False):
	case = Path(case).resolve()
	artifacts = {}
	for path in sorted(case.iterdir()):
		if path.is_file() and path.name not in ("COMMITTED.json", "CAPTURE.json"):
			# Raw evidence must reach stable storage before its commit marker.
			with path.open("rb") as stream:
				os.fsync(stream.fileno())
			artifacts[path.name] = digest(path)
	value = {"status": "PASS", "identity": identity, "record": record,
		"artifacts": artifacts, "legacy_import": imported}
	atomic_json(case / "COMMITTED.json", value)
	return str(case / "COMMITTED.json")


def load_case(path, identity):
	path = Path(path).resolve()
	value = json.loads(path.read_text())
	if value["status"] != "PASS":
		raise ValueError("cached case is not PASS")
	if value["identity"] != identity:
		raise ValueError("cached case input identity changed")
	for name, sha in value["artifacts"].items():
		if Path(name).name != name:
			raise ValueError("invalid artifact name")
		if digest(path.parent / name) != sha:
			raise ValueError("cached artifact changed: " + name)
	required = {"program.rn", "CONFIG.json", "GDB.json", "READINESS.json", "direct.stdout", "program.stdout"}
	if not required <= value["artifacts"].keys():
		raise ValueError("cached evidence missing")
	if not {"map.callgrind.1", "map.callgrind.2"} <= value["artifacts"].keys():
		raise ValueError("cached profiles missing")
	return value["record"]


def case_key(record):
	return record["kind"], record["rows"], record["layout"], record["role"]


def resume_index(manifest, identity):
	value = json.loads(Path(manifest).read_text())
	if value["identity"] != identity:
		raise ValueError("resume input identity changed")
	# This is an index of committed cases, never a cached whole-run verdict.
	index = {}
	if value.get("failures"):
		raise ValueError("resume contains failed cases; explicit disposition required")
	paths = set(value["cases"])
	paths.update(str(p.resolve()) for p in Path(manifest).parent.glob("*/COMMITTED.json"))
	for path in sorted(paths):
		record = load_case(path, identity)
		key = case_key(record)
		if key in index:
			raise ValueError("duplicate cached case")
		index[key] = (record, path)
	return index
