#!/usr/bin/env python3
"""Admit a durable Ir service under detected CPU and ancestor memory limits."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys

from ir_resources import launch_budget, resources


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument("--unit", required=True)
	p.add_argument("--manifest", type=Path, required=True)
	p.add_argument("--peak-bytes", required=True, type=int)
	p.add_argument("--peak-evidence", required=True)
	p.add_argument("command", nargs=argparse.REMAINDER)
	args = p.parse_args()
	command = args.command[1:] if args.command[:1] == ["--"] else args.command
	assert command and "--resources" not in command
	root = Path(__file__).resolve().parents[2]
	subprocess.run([sys.executable, "-B", str(root / "ci/preflight.py"), "--repo", str(root)], check=True)
	# The new service lives in app.slice, which may differ from this process.
	group = subprocess.check_output(["systemctl", "--user", "show", "app.slice", "--property=ControlGroup", "--value"], text=True).strip()
	assert group.startswith("/"), "cannot locate destination slice"
	cpus, available, headroom = resources(group)
	limits = launch_budget(cpus, available, headroom, args.peak_bytes, 2 * 1024**3)
	assert limits["workers"] >= 2, "two workers required for determinism control"
	limits.update(peak_evidence=args.peak_evidence, destination_cgroup=group)
	args.manifest.parent.mkdir(parents=True, exist_ok=True)
	with args.manifest.open("x") as stream:
		json.dump(limits, stream, indent=2); stream.flush(); os.fsync(stream.fileno())
	launch = ["systemd-run", "--user", "--unit=" + args.unit, "--slice=app.slice",
		"--working-directory=" + str(Path.cwd()), "--setenv=PATH=" + os.environ["PATH"],
		"--property=MemoryMax=" + str(limits["memory_max_bytes"]), "--property=MemorySwapMax=0",
		"--property=OOMPolicy=kill", "--property=KillMode=control-group"]
	if os.environ.get("SSH_AUTH_SOCK"):
		launch.append("--setenv=SSH_AUTH_SOCK=" + os.environ["SSH_AUTH_SOCK"])
	subprocess.run(launch + command + ["--resources", str(args.manifest.resolve())], check=True)


if __name__ == "__main__":
	main()
