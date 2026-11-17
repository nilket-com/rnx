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


CENSUS_PLAN = Path(__file__).resolve().parent / "verify" / "fixtures" / "census" / "execution.tsv"
# What a census failure's receipt lists for retention, in priority order:
# the final keeps at most eight files, the receipt included.
CENSUS_EVIDENCE = (("stock", "report/result.json"), ("fresh", "report/result.json"),
				   ("stock", "stderr"), ("fresh", "stderr"), ("stock", "stdout"), ("fresh", "stdout"))
HEADLINE = "unreviewed census execution differences"


def regular_file(base, relative):
	"""base/relative, when every component is a plain entry and the last a regular file."""
	import stat
	path = base
	for part in Path(relative).parts:
		if part in ("", ".", "..") or "/" in part: return None
		path = path / part
		try: mode = os.lstat(path).st_mode
		except OSError: return None
		if stat.S_ISLNK(mode): return None
	return path if stat.S_ISREG(mode) else None


def bounded_read(path, limit):
	"""At most limit bytes of path, or None when it holds more."""
	with path.open("rb") as stream: data = stream.read(limit + 1)
	return None if len(data) > limit else data


def cut_bytes(text, limit):
	"""text within limit UTF-8 bytes, cut at a character boundary and marked."""
	data = text.encode()
	if len(data) <= limit: return text
	return data[:limit - 3].decode(errors="ignore") + "..."


def census_headline(differences):
	"""One line naming the differences, as the verifier's headline bounds it:
	the total, the first four cases (each within 512 bytes), "(+K more)",
	the whole within 4096 bytes, named cases dropped before the total."""
	named = [cut_bytes(f"{' '.join(str(d.get('id', '')).split())}: {' '.join(str(d.get('reason', '')).split())}", 512)
			 for d in differences[:4] if isinstance(d, dict)]
	head = f"{HEADLINE} ({len(differences)} cases): "
	while True:
		left = len(differences) - len(named)
		line = head + "; ".join(named) + (f" (+{left} more)" if left else "")
		if len(line.encode()) <= 4096 or not named: return line
		named.pop()


def census_failure(output):
	"""A census failure's concrete error, its retained evidence and the
	evidence its runs never wrote, from the census summary alone; None when
	there is no readable census difference, so any other failure keeps its
	receipt as it was."""
	summary_name = "census/live-execution/RESULT.json"
	try:
		summary_path = regular_file(output, summary_name)
		if summary_path is None: return None
		data = bounded_read(summary_path, 1 << 20)
		if data is None: return None
		differences = json.loads(data)["differences"]
		if not isinstance(differences, list) or not differences: return None
		known = {line.split("\t", 1)[0] for line in CENSUS_PLAN.read_text().splitlines()[1:] if line}
	except (OSError, ValueError, KeyError, TypeError):
		return None
	# Every row must name a known case and a reason, or the summary is malformed.
	if not all(isinstance(d, dict) and isinstance(d.get("id"), str) and d["id"] in known
			   and isinstance(d.get("reason"), str) for d in differences):
		return None
	cases = [d["id"] for d in differences]
	files, unavailable = [summary_name], []
	for case in cases:
		for engine, name in CENSUS_EVIDENCE:
			relative = f"census/live-execution/{case}/{engine}/{name}"
			if regular_file(output, relative) is None:
				unavailable.append(f"census evidence unavailable: {case} {engine} {name} (the run did not write it)")
			elif len(files) < 16:
				files.append(relative)
	return dict(error=census_headline(differences), diagnostic_files=files, causes=unavailable[:16])


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
	sys.path.insert(0, str(Path(__file__).resolve().parent / "column"))
	from host_budget import build_jobs
	parser.add_argument("--jobs", type=int, default=min(build_jobs(), available_cpus))
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
	if os.environ.get('RNX_FINAL'):
		assigned=os.environ.get('RNX_FINAL_CROSS_ROOT')
		if not assigned or output.parent != Path(assigned) or output.name != 'payload':
			parser.error('final cross output was not assigned')
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
	sys.path.insert(0, str(controller_root / "tools/column"))
	import child_result
	from owned_build import OwnedBuild, run_process
	owned = OwnedBuild(output, 'target')
	failure = None
	try:
		with owned:
			try:
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
					run_process(namespace + cargo + ["metadata", "--offline", "--format-version", "1",
													   "--features", "cross-verification"],
								   env=env, stdout=stdout, stderr=stderr, check=True)
				from engine_source import check_metadata
				check_metadata(controller_root, json.loads((output / "metadata.json").read_text()))
				command = namespace + cargo + ["test", "--offline", "--locked", "--test", "rnx",
											   "--features", "cross-verification", "--no-run",
											   "--message-format=json"]
				with (output / "cargo.stdout").open("xb") as stdout, (output / "cargo.stderr").open("xb") as stderr:
					run_process(command, env=env, stdout=stdout, stderr=stderr, check=True)
				# This locates a launch candidate only. Rust independently validates the
				# entire artifact stream, uniqueness and source/binary identity before use.
				records = (json.loads(line) for line in (output / "cargo.stdout").read_text().splitlines())
				executable = next(row["executable"] for row in records
								  if row.get("reason") == "compiler-artifact"
								  and row.get("target", {}).get("name") == "rnx"
								  and row.get("target", {}).get("kind") == ["test"]
								  and row.get("executable"))
				controller_error = None
				try:
					with (output / "controller.stdout").open("xb") as stdout, (output / "controller.stderr").open("xb") as stderr:
						run_process([executable, "--controller", str(request_path)],
									   env=env, stdout=stdout, stderr=stderr, check=True)
				except Exception as error:
					controller_error = error
					raise
				finally:
					binary = output / "bound-verification-child"
					try:
						if binary.exists():
							with (output / "host-check.stdout").open("xb") as stdout, (output / "host-check.stderr").open("xb") as stderr:
								run_process([str(binary), "--host-check", str(request_path)],
											   env=env, stdout=stdout, stderr=stderr, check=True)
					except Exception as host_error:
						if controller_error is None: raise
						controller_error.add_note("host check also failed: " + repr(host_error))
				if not (output / "bound-verification-child").is_file():
					raise RuntimeError("the Rust controller did not bind an executable; see controller.stderr")
			finally:
				# Freeze the controller's result before publishing the launcher's
				# terminal cleanup result. No previously bound receipt is rewritten.
				if (output/'RESULT.json').exists():
					(output/'RESULT.json').rename(output/'CONTROLLER-RESULT.json')
	except Exception as error:
		failure = error
	result = terminal_receipt(output, failure, owned.result)
	child_result.atomic_json(output/'RESULT.json', result)
	child_result.bind(output/'RESULT.json')
	if failure is not None: raise failure


def terminal_receipt(output, failure, cleanup):
	"""The launcher's published receipt: the controller's result, the owned
	build's cleanup, and on failure the launcher's error and, for a census
	difference, its concrete cause and bounded evidence."""
	result_path = output/'CONTROLLER-RESULT.json'
	result = json.loads(result_path.read_text()) if result_path.exists() else {'status':'FAIL'}
	result['cleanup'] = cleanup
	if failure is not None:
		result.update(status='FAIL', launcher_error=repr(failure), launcher_notes=getattr(failure, '__notes__', []))
		# A census difference names its cases and keeps their bounded
		# evidence, never at the cost of the original failure.
		try: census = census_failure(output)
		except Exception: census = None
		if census is not None: result.update(census)
	return result


if __name__ == "__main__":
	main()
