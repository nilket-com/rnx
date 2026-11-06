"""Compatibility entry point; ADR0209 root preflight owns commit policy."""
import argparse
from pathlib import Path
import subprocess
import sys


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[3])
	args = parser.parse_args()
	root = Path(subprocess.check_output(["git", "--no-replace-objects", "-C", str(args.repo),
		"rev-parse", "--show-toplevel"], text=True).strip())
	subprocess.run([sys.executable, "-B", str(root / "ci/preflight.py"), "--repo", str(root)], check=True)


if __name__ == "__main__":
	main()
