#!/usr/bin/env python3
"""Prepare a private Cargo graph and launch the Rust cross-repository verifier.

Observation, source binding, comparison, and acceptance belong to the checked-in
Rust controller. This launcher never changes the product manifest or lock.
"""

import sys
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import time



def check_rune_pins(root):
	"""Compatibility entry point for the canonical-source guard."""
	from engine_source import check_tree
	return check_tree(root)


def validate_request(request):
	"""Reject incomplete source binding before any Cargo compilation."""
	if request.get("rune_rev") is None:
		if not request.get("rune_path"):
			raise ValueError("missing request rune_path")
		if Path(request["rune_path"]).resolve(strict=True) != Path(request["rune_root"]).resolve(strict=True):
			raise ValueError("requested Rune path differs from selected source")


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("command", nargs="?", default="cross", choices=["cross"])
	parser.add_argument("--suite", choices=["census", "enrollment", "all"], default="all")
	source = parser.add_mutually_exclusive_group()
	source.add_argument("--rune-rev")
	source.add_argument("--rune-path", type=Path)
	parser.add_argument("--rune-repo", type=Path)
	parser.add_argument("--fixtures-root", type=Path, help="override the checked-in test input root")
	parser.add_argument("--out", type=Path)
	available_cpus = len(os.sched_getaffinity(0))
	parser.add_argument("--jobs", type=int, default=min(8, available_cpus))
	parser.add_argument("--registration-fixture", type=Path)
	args = parser.parse_args()
	if not shutil.which("bwrap"):
		parser.error("bwrap (bubblewrap) is required; no host-filesystem fallback is available")
	cargo_path = shutil.which("cargo")
	if not cargo_path:
		parser.error("cargo is required on PATH (the runner uses toolchain 1.98.1)")
	version = subprocess.check_output(["bwrap", "--version"], text=True).strip()
	if not 1 <= args.jobs <= available_cpus:
		parser.error(f"--jobs must be between 1 and the {available_cpus} CPUs in the current affinity")
	controller_root = Path(__file__).resolve().parents[1]
	try:
		pin_consistency = check_rune_pins(controller_root)
	except (ValueError, KeyError, OSError) as error:
		parser.error(str(error))
	root = (args.registration_fixture.resolve(strict=True)
			if args.registration_fixture else controller_root)
	args.fixtures_root = (args.fixtures_root or root / "tools/verify/fixtures").resolve(strict=True)
	if args.rune_path:
		args.rune_path = args.rune_path.resolve(strict=True)
	canonical_engine = (controller_root / "rune").resolve(strict=True)
	if args.rune_repo or args.rune_rev or (args.rune_path and args.rune_path != canonical_engine):
		parser.error("historical engine overrides require a standalone historical host checkout; this verifier binds the canonical monorepo engine")
	if args.out is None:
		args.out = root / "target" / "rnx-verify" / f"{os.getpid()}-{time.time_ns()}"
	output = args.out.resolve()
	if output.is_relative_to(root):
		scratch_root = (root / "target" / "rnx-verify").resolve()
		if not output.is_relative_to(scratch_root):
			parser.error("in-tree output must be under target/rnx-verify")
		ignored = subprocess.run(["git", "-C", str(root), "check-ignore", "-q", "--", str(output.relative_to(root))])
		if ignored.returncode != 0:
			parser.error("in-tree output must be ignored by Git")
	if args.rune_path and output.is_relative_to(args.rune_path):
		parser.error("--out must be outside the Rune source tree")
	output.mkdir(parents=True, exist_ok=False)
	with (output / "PIN-CONSISTENCY.json").open("x") as stream:
		json.dump(pin_consistency, stream, indent=2)
		stream.write("\n")
	manifest = output / "Cargo.toml"
	lock = output / "Cargo.lock"
	original_manifest = output / "original-Cargo.toml"
	original_lock = output / "original-Cargo.lock"
	shutil.copyfile(root / "Cargo.toml", original_manifest)
	shutil.copyfile(root / "Cargo.lock", original_lock)
	rune_repo = None
	resolved = None
	rune = canonical_engine
	if output.is_relative_to(rune):
		parser.error("--out must be outside the Rune source tree")
	text = (root / "Cargo.toml").read_text()
	text = text.replace('[package]\n', '[package]\nautolib = false\nautobins = false\nautotests = false\nautoexamples = false\nautobenches = false\n', 1)
	text = text.replace('[features]\n', '[features]\ncross-verification = []\n', 1)
	text += '\n[[test]]\nname = "rnx"\npath = "src/lib.rs"\nharness = false\nrequired-features = ["cross-verification"]\n'
	manifest.write_text(text)
	shutil.copyfile(root / "Cargo.lock", lock)
	request = dict(vars(args))
	request.update(rnx_root=root, rune_root=rune, rune_path=rune, rune_repo=rune_repo,
				   controller_root=controller_root,
				   out=output, bwrap_version=version,
				   original_manifest=original_manifest, original_lock=original_lock,
				   rune_revision_resolved=resolved,
				   rune_archive=None)
	validate_request(request)
	request_path = output / "REQUEST.json"
	request_path.write_text(json.dumps(request, default=str, indent=2) + "\n")
	env = dict(os.environ, CARGO_TARGET_DIR=str(output / "target"),
			   RNX_VERIFY_RUNE_ROOT=str(rune),
			   CARGO_BUILD_JOBS=str(args.jobs), CARGO_INCREMENTAL="0",
			   CARGO_PROFILE_DEV_DEBUG="2", CARGO_PROFILE_TEST_DEBUG="2",
			   CARGO_PROFILE_DEV_STRIP="none", CARGO_PROFILE_TEST_STRIP="none",
			   RUSTFLAGS="-Cdebuginfo=2 -Cstrip=none")
	env.pop("CARGO_ENCODED_RUSTFLAGS", None)
	cargo_home_env = os.environ.get("CARGO_HOME")
	cargo_home = (Path(cargo_home_env) if cargo_home_env else Path.home() / ".cargo").resolve(strict=True)
	namespace = ["bwrap", "--die-with-parent", "--ro-bind", "/", "/",
				 "--tmpfs", "/tmp",
				 "--ro-bind", str(root), str(root),
				 "--bind", str(output), str(output),
				 "--ro-bind", str(rune), str(rune),
				 "--bind", str(cargo_home), str(cargo_home),
				 "--ro-bind", str(manifest), str(root / "Cargo.toml"),
				 "--bind", str(lock), str(root / "Cargo.lock"),
				 "--proc", "/proc", "--dev", "/dev", "--chdir", str(root)]
	cargo = [cargo_path, "+1.98.1"]
	# Resolve only the private lock. The Rust controller validates all seven
	# packages, including the compiler-only project dependency.
	with (output / "metadata.json").open("xb") as stdout, (output / "metadata.stderr").open("xb") as stderr:
		subprocess.run(namespace + cargo + ["metadata", "--offline", "--format-version", "1",
										   "--features", "cross-verification"],
					   env=env, stdout=stdout, stderr=stderr, check=True)
	from engine_source import check_metadata
	check_metadata(controller_root, json.loads((output / "metadata.json").read_text()))
	command = namespace + cargo + ["test", "--offline", "--locked", "--test", "rnx",
								   "--features", "cross-verification", "--no-run",
								   "--message-format=json"]
	with (output / "cargo.stdout").open("xb") as stdout, (output / "cargo.stderr").open("xb") as stderr:
		subprocess.run(command, env=env, stdout=stdout, stderr=stderr, check=True)
	# This locates a launch candidate only. Rust independently validates the
	# entire artifact stream, uniqueness and source/binary identity before use.
	records = (json.loads(line) for line in (output / "cargo.stdout").read_text().splitlines())
	executable = next(row["executable"] for row in records
					  if row.get("reason") == "compiler-artifact"
					  and row.get("target", {}).get("name") == "rnx"
					  and row.get("target", {}).get("kind") == ["test"]
					  and row.get("executable"))
	try:
		with (output / "controller.stdout").open("xb") as stdout, (output / "controller.stderr").open("xb") as stderr:
			subprocess.run([executable, "--controller", str(request_path)],
						   env=env, stdout=stdout, stderr=stderr, check=True)
	finally:
		binary = output / "bound-verification-child"
		if binary.exists():
			with (output / "host-check.stdout").open("xb") as stdout, (output / "host-check.stderr").open("xb") as stderr:
				subprocess.run([str(binary), "--host-check", str(request_path)],
							   env=env, stdout=stdout, stderr=stderr, check=True)
	if not (output / "bound-verification-child").is_file():
		raise RuntimeError("the Rust controller did not bind an executable; see controller.stderr")

	# The Rust result is authoritative; the Python launcher only binds it.
	sys.path.insert(0, str(controller_root / "tools/column"))
	import child_result
	child_result.bind(output / "RESULT.json")


if __name__ == "__main__":
	main()
