#!/usr/bin/env python3
"""Durable parallel whole-map instruction counts; cycles are reported separately."""
import argparse
import hashlib
from concurrent.futures import ProcessPoolExecutor, as_completed
import json
import multiprocessing
import os
import platform
from pathlib import Path
import subprocess
import sys
import tempfile
import traceback

import ir_accounting
from cli_symbols import symbols

from ir_resources import compare_determinism, launch_budget, resources

if not __debug__:
	raise SystemExit("requires assertions")

PIN = "fed878b5aea151a878462d71626c9a4d589a4273"
CAPTURE = None
ALLOCATOR = "glibc.malloc.arena_max=1:glibc.malloc.mmap_threshold=131072:glibc.malloc.trim_threshold=131072:glibc.malloc.top_pad=0:glibc.malloc.perturb=0"


def capture_environment():
	# An allowlist prevents service/worker variables from changing startup layout.
	return {"PATH": "/usr/local/bin:/usr/bin:/bin", "HOME": "/tmp", "LC_ALL": "C",
		"POLARS_MAX_THREADS": "1", "PYTHONDONTWRITEBYTECODE": "1", "GLIBC_TUNABLES": ALLOCATOR}


def measured_command(command, identity):
	if identity["aslr"]["disabled"]:
		return [identity["aslr"]["setarch"], identity["aslr"]["machine"], "-R", *command]
	return command


def slot_name(kind, rows, layout, role):
	return hashlib.sha256(json.dumps([kind, rows, layout, role]).encode()).hexdigest()


def terminal_json(path, value):
	"""Receipt writer that does not depend on external tooling setup succeeding."""
	path.parent.mkdir(parents=True, exist_ok=True)
	fd, temporary = tempfile.mkstemp(prefix=".receipt-", dir=path.parent)
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


def tooling(root):
	global CAPTURE
	assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=root, text=True).strip() == PIN
	assert not subprocess.check_output(["git", "status", "--porcelain", "--", "tools/xtask"], cwd=root, text=True).strip()
	sys.path.insert(0, str(root / "tools/xtask/column"))
	import cli_capture
	# Keep the frozen capture recipe; resolve current sibling names exactly.
	cli_capture.symbols = symbols
	CAPTURE = cli_capture
	return cli_capture


def verify(record, directory, role, identity):
	c = CAPTURE
	assert (directory / "program.rn").read_text() == c.fixture(record["kind"], record["rows"], record["layout"])
	config = json.loads((directory / "CONFIG.json").read_text())
	assert config["sha256"] == role["sha256"] and config["symbols"] == role["symbols"]
	assert config["budget"] == identity["budget"] and config["calls"] == 2
	for name in ("direct.stdout", "program.stdout"):
		c.validate((directory / name).read_text(), record["kind"], record["rows"], record["layout"])
	assert json.loads((directory / "GDB.json").read_text())["status"] == "PASS"
	parts = sorted(directory.glob("map.callgrind.*"))
	assert len(parts) == 2
	costs = []
	for part in parts:
		data = c.callgrind.parse(part)
		def cost(fragment):
			return sum(v["self_Ir"] for k, v in data["functions"].items() if fragment in k)
		assert cost("rnx_polars::column::apply_joined") > 0
		assert cost("experimental_core::column::run") > 0
		for forbidden in ("rnx::column::prepare::compile", "rnx_polars::column::script::column_kernel", "new_from_index_polars_core__series"):
			assert cost(forbidden) == 0, forbidden
		costs.append({"accounting": ir_accounting.account(part, role["binary"]), "Ir": data["total_Ir"], "Ir_per_row": data["total_Ir"] / record["rows"], "profile": str(part), "sha256": c.digest(part)})
	if "costs" in record:
		assert record["costs"] == costs
	return costs


def execute(job):
	c = CAPTURE
	d = Path(job["directory"])
	role, identity = job["role"], job["identity"]
	assert c.digest(role["binary"]) == role["sha256"]
	if (d / "COMMITTED.json").exists():
		record = c.cli_cache.load_case(d / "COMMITTED.json", identity)
		assert c.cli_cache.case_key(record) == (job["kind"], job["rows"], job["layout"], role["role"])
		verify(record, d, role, identity)
		return record, str(d / "COMMITTED.json")
	if job.get("import_directory") and not d.exists():
		old = Path(job["import_directory"])
		if (old / "COMMITTED.json").exists():
			prior = json.loads((old / "COMMITTED.json").read_text())
			old_identity = prior["identity"]
			for key in old_identity:
				if key != "tooling":
					assert old_identity[key] == identity[key], ("import identity changed", key)
			for path, digest in old_identity["tooling"].items():
				if Path(path) != Path(__file__):
					assert c.digest(path) == digest, ("import dependency changed", path)
			record = c.cli_cache.load_case(old / "COMMITTED.json", old_identity)
			assert c.cli_cache.case_key(record) == (job["kind"], job["rows"], job["layout"], role["role"])
			old_costs = record.pop("costs")
			new_costs = verify(record, old, role, identity)
			for a, b in zip(old_costs, new_costs, strict=True):
				assert a == b or a == {k: v for k, v in b.items() if k != "accounting"}
			d.mkdir(parents=True)
			for path in old.iterdir():
				if path.is_file() and path.name not in ("COMMITTED.json", "CAPTURE.json"):
					(d / path.name).symlink_to(path.resolve())
			record["costs"] = verify(record, d, role, identity)
			c.cli_cache.atomic_json(d / "IMPORT.json", {"source": str(old), "approval": "user approved ELF-object scoring; raw completed cases revalidated; old failure retained"})
			c.cli_cache.atomic_json(d / "CAPTURE.json", record)
			return record, c.cli_cache.commit_case(d, record, identity, imported=True)
	if d.exists():
		raise ValueError("incomplete or failed case requires explicit disposition: " + str(d))
	d.mkdir(parents=True)
	try:
		kind, rows, layout = job["kind"], job["rows"], job["layout"]
		source = d / "program.rn"
		source.write_text(c.fixture(kind, rows, layout))
		(d / "binary").symlink_to(role["binary"])
		config = dict(role, out=str(d), fixture=str(source), valgrind_root=job["valgrind"], calls=2, budget=identity["budget"], gdb_receipt=str(d / "GDB.json"))
		config["binary"] = str(d / "binary")
		c.cli_cache.atomic_json(d / "CONFIG.json", config)
		c.cli_cache.atomic_json(d / "READINESS.json", {"status": "IR_RESOURCE_ADMITTED", "cooldown": False})
		env = identity["environment"]
		commands = [("direct", [str(d / "binary"), "run", "--debug-engine", "--budget", str(identity["budget"]), "program.rn"]),
			("child", [sys.executable, "-B", str(c.HERE / "cli_child.py"), str(d / "CONFIG.json")])]
		for name, command in commands:
			with (d / (name + ".stdout")).open("x") as out, (d / (name + ".stderr")).open("x") as err:
				done = subprocess.run(measured_command(command, identity), cwd=d, env=env, stdout=out, stderr=err, timeout=4500)
			assert done.returncode == 0, (name, done.returncode)
		record = {"kind": kind, "rows": rows, "layout": layout, "role": role["role"], "fixture_sha256": c.digest(source)}
		record["costs"] = verify(record, d, role, identity)
		c.cli_cache.atomic_json(d / "CAPTURE.json", record)
		return record, c.cli_cache.commit_case(d, record, identity)
	except BaseException as error:
		c.cli_cache.atomic_json(d / "FAILED.json", {"status": "FAIL", "error": repr(error), "traceback": traceback.format_exc()})
		raise


def run_jobs(jobs, workers):
	assert len({j["directory"] for j in jobs}) == len(jobs)
	if workers == 1:
		return [execute(j) for j in jobs]
	with ProcessPoolExecutor(max_workers=workers, mp_context=multiprocessing.get_context("fork")) as pool:
		futures = [pool.submit(execute, j) for j in jobs]
		return [future.result() for future in as_completed(futures)]


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument("--rune", type=Path, required=True)
	p.add_argument("--builds", type=Path, required=True)
	p.add_argument("--out", type=Path, required=True)
	p.add_argument("--valgrind", type=Path, required=True)
	p.add_argument("--resources", type=Path, required=True)
	p.add_argument("--mode", choices=("preflight", "large"), required=True)
	p.add_argument("--preflight", type=Path)
	p.add_argument("--import-cases-from", type=Path, help="explicitly revalidate completed cases after the approved scoring change")
	args = p.parse_args()
	o = args.out
	result = {"status": "FAIL"}
	try:
		o = args.out.resolve()
		o.mkdir(parents=True, exist_ok=True)
		c = tooling(args.rune.resolve())
		build = json.loads(args.builds.read_text()); assert build["status"] == "PASS"
		roles = build["builds"]; assert [r["role"] for r in roles] == ["baseline", "candidate"]
		for role in roles:
			assert c.digest(role["binary"]) == role["sha256"]
			role["symbols"] = c.symbols(role["binary"])
		policy = json.loads(Path(__file__).with_name("CK2.json").read_text())
		identity = {"builds": c.digest(args.builds), "rune_tooling": PIN,
			"tooling": {str(f): c.digest(f) for f in [*c.HERE.glob("cli_*.py"), *c.HERE.parent.joinpath("measure").glob("*.py"), c.HERE / "CK2.json", Path(__file__), Path(__file__).with_name("ir_resources.py"), Path(__file__).with_name("ir_accounting.py"), Path(__file__).with_name("CK2.json")]},
			"valgrind": c.digest(args.valgrind / "bin/valgrind"), "budget": 1000000000, "calls": 2, "threads": 1, "scoring": "user-approved-ELF-object-exact-system-slope-0.001-constant-4096-v1", "readiness": "Ir-no-cooldown-v1", "environment": capture_environment(), "path_recipe": "equal-width-group-and-sha256-slot-with-binary-symlink-v1", "aslr": {"disabled": True, "setarch": "/usr/bin/setarch", "machine": platform.machine(), "sha256": c.digest("/usr/bin/setarch") }}
		if args.mode == "large":
			assert args.preflight
			pre = json.loads(args.preflight.read_text()); assert pre["status"] == "PASS" and pre["identity"] == identity
			expected = {(k, 4096, l, role["role"]) for k in c.KERNELS for l in c.LAYOUTS for role in roles}
			assert len(pre["records"]) == len(expected) and {c.cli_cache.case_key(record) for record in pre["records"]} == expected
		probe = subprocess.check_output(measured_command([sys.executable, "-B", "-c", "from pathlib import Path; print(Path('/proc/self/personality').read_text().strip())"], identity), env=capture_environment(), text=True).strip()
		assert int(probe, 16) & 0x40000, "ADDR_NO_RANDOMIZE was not set"
		limits = json.loads(args.resources.read_text())
		cpus, available, headroom = resources()
		workers = min(limits["workers"], launch_budget(cpus, available, headroom, limits["measured_case_peak_bytes"], 0)["workers"])
		assert workers >= 2, "parallel determinism control requires two admitted workers"
		manifest = {"identity": identity, "mode": args.mode, "resources": limits, "effective_workers": workers}
		if (o / "MANIFEST.json").exists():
			prior = json.loads((o / "MANIFEST.json").read_text()); assert prior["identity"] == identity and prior["mode"] == args.mode
		else:
			c.cli_cache.atomic_json(o / "MANIFEST.json", manifest)
		def job(kind, rows, layout, role, group, capture_identity=None):
			return dict(kind=kind, rows=rows, layout=layout, role=role, identity=capture_identity or identity, valgrind=str(args.valgrind.resolve()), directory=str(o / group / slot_name(kind, rows, layout, role["role"])), import_directory=str(args.import_cases_from.resolve() / group / slot_name(kind, rows, layout, role["role"])) if args.import_cases_from else None)
		# Fixed representative subset; compare exact first and warm totals, no tolerance.
		control = [("i64add", 4096, c.LAYOUTS[0]), ("f64add", 4096, c.LAYOUTS[-1])]
		# Reviewed recipe revalidation includes mmap-sized allocations in both modes.
		control.append(("i64add", max(policy["rows"]), c.LAYOUTS[0]))
		def control_jobs(group):
			return [job(k, rows, l, role, group) for k, rows, l in control for role in roles]
		def totals(records):
			return {str(c.cli_cache.case_key(r)): [v["Ir"] for v in r["costs"]] for r, _ in records}
		# Classification evidence only: same small case twice without ASLR control.
		random_identity = dict(identity, aslr=dict(identity["aslr"], disabled=False))
		random_first = run_jobs([job("i64add", 4096, c.LAYOUTS[0], roles[1], "random01", random_identity)], 1)
		random_again = run_jobs([job("i64add", 4096, c.LAYOUTS[0], roles[1], "random02", random_identity)], 1)
		c.cli_cache.atomic_json(o / "CLASSIFICATION.json", compare_determinism(totals(random_first), totals(random_again)))
		serial = run_jobs(control_jobs("serial00"), 1)
		repeat = run_jobs(control_jobs("serial01"), 1)
		def partitions(records):
			return {str(c.cli_cache.case_key(r)): [v["accounting"] for v in r["costs"]] for r, _ in records}
		repeat_check = ir_accounting.comparison(partitions(serial), partitions(repeat))
		c.cli_cache.atomic_json(o / "SERIAL-REPEAT.json", repeat_check)
		assert repeat_check["exact_partitions_match"], "ASLR-off serial repeat program mismatch"
		parallel = run_jobs(control_jobs("parall00"), workers)
		determinism = ir_accounting.comparison(partitions(serial), partitions(parallel))
		c.cli_cache.atomic_json(o / "DETERMINISM.json", determinism)
		assert determinism["exact_partitions_match"], "serial/parallel program instruction-count mismatch"
		jobs = [job(k, n, l, role, "matrix00") for n in ([4096] if args.mode == "preflight" else policy["rows"]) for k in c.KERNELS for l in c.LAYOUTS for role in roles]
		records = run_jobs(jobs, workers)
		for path, digest in identity["tooling"].items(): assert c.digest(path) == digest
		result = {"status": "PASS", "identity": identity, "records": [r for r, _ in records], "cases": [p for _, p in records], "determinism": determinism}
		if args.mode == "large":
			result["acceptance"] = ir_accounting.evaluate(result["records"], policy, c.cli_rules.evaluate)
			result["status"] = result["acceptance"]["status"]
	except BaseException as error:
		result.update(status="FAIL", error=repr(error), traceback=traceback.format_exc())
	finally:
		terminal_json(o / "RESULT.json", result)
		n = o / "NOTICE.txt"; n.write_text(f"rnx0206 parallel Ir {args.mode} EXIT {result['status']}. Receipt: {o / 'RESULT.json'}")
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
