#!/usr/bin/env python3
"""Bundle a signed candidate and block on fresh acceptance under the verifier account."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import traceback

ROOT = Path(__file__).resolve().parents[2]


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--host", required=True)
	parser.add_argument("--remote-root", required=True, help="absolute verifier-owned work root")
	parser.add_argument("--out", required=True, type=Path)
	parser.add_argument("--memory-bytes", required=True, type=int)
	parser.add_argument("--signers", type=Path, default=Path.home() / ".ssh/allowed_signers")
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	result = {"status": "FAIL", "host": args.host, "remote_root": args.remote_root, "memory_bytes": args.memory_bytes}
	try:
		subprocess.run(["python3", "-B", str(ROOT / "ci/disk_space.py"), "--path", str(ROOT), "--path", str(out)], check=True)
		assert args.memory_bytes > 0
		assert os.environ.get("SSH_AUTH_SOCK") and Path(os.environ["SSH_AUTH_SOCK"]).exists(), "missing SSH agent"
		assert re.fullmatch(r"[A-Za-z0-9_.-]+", args.host), "invalid SSH host"
		assert re.fullmatch(r"[A-Za-z0-9_-]+", out.name), "output basename must be a unique safe run name"
		candidate = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
		assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
		for revision in ("HEAD", "HEAD^"):
			subprocess.run(["git", "verify-commit", revision], cwd=ROOT, check=True, capture_output=True)
		ssh = ["ssh", "-oBatchMode=yes", "-oConnectTimeout=15", args.host]
		subprocess.run(ssh + ["true"], check=True)
		label = "rnx-0205-" + candidate[:8] + "-" + out.name
		assert args.remote_root.startswith("/") and "\n" not in args.remote_root, "invalid remote work root"
		remote = args.remote_root.rstrip("/") + "/" + label
		prefix = "/tmp/" + label
		bundle = out / "source.bundle"
		subprocess.run(["git", "bundle", "create", str(bundle), "HEAD"], cwd=ROOT, check=True)
		quote = shlex.quote
		bootstrap = out / "bootstrap.sh"
		bootstrap.write_text("\n".join([
			"#!/bin/bash", "set -euo pipefail", 'export PATH="$HOME/.cargo/bin:/usr/local/bin:/usr/bin:/bin"',
			"unset CARGO_TARGET_DIR", "exec 9<>/tmp/rnx-runtime-bench.lock", "flock --shared --nonblock 9",
			"test ! -e " + quote(remote), "mkdir -p " + quote(remote),
			"git clone --no-checkout " + quote(prefix + ".bundle") + " " + quote(remote + "/rnx"),
			"cd " + quote(remote + "/rnx"), "git checkout --detach " + quote(candidate),
			"git config --local gpg.ssh.allowedSignersFile " + quote(prefix + ".signers"),
			"python3 -B ci/disk_space.py --path . --path " + quote(remote),
			"git verify-commit HEAD",
			"git -c gpg.ssh.allowedSignersFile=" + quote(prefix + ".signers") + " verify-commit HEAD^",
			"exec python3 -B tools/column/accept.py --candidate " + quote(candidate) + " --out " + quote(remote + "/acceptance"), "",
		]))
		for source, target in ((bundle, prefix + ".bundle"), (bootstrap, prefix + ".sh"), (args.signers, prefix + ".signers")):
			subprocess.run(["scp", "-q", "-oBatchMode=yes", str(source), args.host + ":" + target], check=True)
		subprocess.run(ssh + ["chmod", "644", prefix + ".signers"], check=True)
		command = ["systemd-run", "--user", "--wait", "--unit=" + label, "--service-type=exec",
			"--property=MemoryMax=" + str(args.memory_bytes), "--property=MemorySwapMax=0", "--property=OOMPolicy=kill",
			"--property=OOMScoreAdjust=1000", "sudo", "-n", "-H", "-u", "rnxverify", "bash", prefix + ".sh"]
		with (out / "remote.stdout").open("x") as stdout, (out / "remote.stderr").open("x") as stderr:
			done = subprocess.run(ssh + [shlex.join(command)], stdout=stdout, stderr=stderr)
		# Read the terminal receipt once, only after the blocking remote command exits.
		receipt = subprocess.run(ssh + [shlex.join(["sudo", "-n", "-u", "rnxverify", "cat", remote + "/acceptance/RESULT.json"])], check=True, capture_output=True)
		(out / "ACCEPTANCE.json").write_bytes(receipt.stdout)
		accepted = json.loads(receipt.stdout)
		result.update(candidate=candidate, remote=remote, memory_bytes=args.memory_bytes,
			bundle_sha256=hashlib.sha256(bundle.read_bytes()).hexdigest(), acceptance_sha256=hashlib.sha256(receipt.stdout).hexdigest(), exit=done.returncode)
		assert done.returncode == 0 and accepted["status"] == "PASS" and accepted["candidate"] == candidate
		assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() == candidate
		assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip()
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		receipt = out / "RESULT.json"
		receipt.write_text(json.dumps(result, indent=2) + "\n")
		summary = "0205 fresh acceptance EXIT " + result["status"]
		notice = out / "NOTICE.txt"
		notice.write_text(summary + ". Receipt: " + str(receipt) + "\n")
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
