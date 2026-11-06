#!/usr/bin/env python3
"""Shared disk admission for maintained build, Miri and measurement launchers."""
import argparse
import json
import os
from pathlib import Path
import shutil

MINIMUM_FREE_BYTES = 20 * 1024**3


def observe(paths, minimum=MINIMUM_FREE_BYTES):
	if minimum <= 0:
		raise ValueError("positive disk reserve required")
	rows = []
	for path in paths:
		path = Path(path).resolve()
		while not path.exists():
			path = path.parent
		free = shutil.disk_usage(path).free
		rows.append({"path": str(path), "free_bytes": free, "minimum_free_bytes": minimum})
	return {"status": "PASS" if rows and all(row["free_bytes"] >= minimum for row in rows) else "FAIL", "filesystems": rows}


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--path", type=Path, action="append", required=True)
	args = parser.parse_args()
	paths = args.path + ([Path(os.environ["CARGO_TARGET_DIR"])] if os.environ.get("CARGO_TARGET_DIR") else [])
	result = observe(paths)
	print(json.dumps(result, indent=2))
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
