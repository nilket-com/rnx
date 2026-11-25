#!/usr/bin/env python3
"""Run checker controls and adopted-history validation before expensive jobs."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
from commit_subjects import git


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
	args = parser.parse_args()
	root = Path(git(args.repo, "rev-parse", "--show-toplevel").strip())
	guard = ["cargo", "run", "--locked", "--quiet"]
	if route := os.environ.pop("RNX_FINAL_GUARD_ROUTE", None):
		guard = [os.environ.pop("RNX_FINAL_GUARD_CONTROLLER"), "build-cargo", route, *guard]
	else:
		guard.extend(["--target-dir", str(root / "target/execute-guard")])
	subprocess.run([*guard, "--manifest-path", str(root / "ci/execute_guard/Cargo.toml"), "--", str(root)], cwd=root, check=True)

	subprocess.run([sys.executable, "-B", str(root / "ci/hygiene.py"), "--repo", str(root)], cwd=root, check=True)
	subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", str(root / "ci"), "-p", "test_hygiene.py"], cwd=root, check=True)
	subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", str(root / "ci"),
		"-p", "test_commit_subjects.py"], cwd=root, check=True)
	subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", str(root / "ci"), "-p", "test_archive_data.py"], cwd=root, check=True)
	subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", str(root / "ci"), "-p", "test_history_ids.py"], cwd=root, check=True)
	subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", str(root / "ci"), "-p", "test_disk_space.py"], cwd=root, check=True)
	subprocess.run([sys.executable, "-B", "-m", "unittest", "discover", "-s", str(root / "ci"), "-p", "test_disk_launchers.py"], cwd=root, check=True)

	subprocess.run([sys.executable, "-B", str(root / "ci/commit_subjects.py"), "--repo", str(root)], cwd=root, check=True)


if __name__ == "__main__":
	main()
