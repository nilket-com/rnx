#!/usr/bin/env python3
"""Launch a durable gate under the exact declared memory ceiling, never a percentage."""
import argparse
import json
import os
from pathlib import Path
import subprocess


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--unit", required=True)
	parser.add_argument("--cwd", required=True, type=Path)
	parser.add_argument("--hosts", required=True, type=Path)
	parser.add_argument("command", nargs=argparse.REMAINDER)
	args = parser.parse_args()
	command = args.command[1:] if args.command[:1] == ["--"] else args.command
	assert command, "missing gate command"
	subprocess.run(["python3", "-B", str(Path(__file__).resolve().parents[2] / "ci/disk_space.py"), "--path", str(args.cwd)], check=True)
	budget = json.loads(args.hosts.read_text())["measurement"]["memory_bytes"]
	assert type(budget) is int and budget > 0, "invalid declared memory budget"
	launch = ["systemd-run", "--user", "--unit=" + args.unit,
		"--working-directory=" + str(args.cwd.resolve(strict=True)),
		"--setenv=PATH=" + os.environ["PATH"],
		"--property=MemoryMax=" + str(budget),
		"--property=MemorySwapMax=0", "--property=OOMPolicy=kill"]
	if os.environ.get("SSH_AUTH_SOCK"):
		launch.append("--setenv=SSH_AUTH_SOCK=" + os.environ["SSH_AUTH_SOCK"])
	subprocess.run(launch + command, check=True)


if __name__ == "__main__":
	main()
