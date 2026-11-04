"""Paired production CLI whole-map captures with all-thread boundary witnesses."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import traceback

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "measure"))
import callgrind
import measurement_runtime as runtime
import cli_rules
import cli_cache
from cli_fixtures import KERNELS, LAYOUTS, fixture, valid_count


def digest(path):
	return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def symbols(binary):
	raw = subprocess.check_output(["nm", str(binary)], text=True).splitlines()
	demangled = subprocess.check_output(["nm", "-C", str(binary)], text=True).splitlines()
	by_address = {}
	for line in raw:
		fields = line.split()
		if len(fields) == 3:
			by_address.setdefault(fields[0], []).append(fields[2])
	answer = {}
	for role, fragments in {
		"map": ["FunctionHandler", "::call_impl", "rnx_polars::column::script::map,"],
		"worker": ["rnx_polars::column::apply_joined", "script::map"],
	}.items():
		matches = [line.split()[0] for line in demangled if all(f in line for f in fragments)]
		assert len(matches) == 1, (role, matches)
		names = by_address[matches[0]]
		assert len(names) == 1, (role, names)
		answer[role] = names[0]
	return answer


def validate(text, kind, rows, layout):
	lines = [line for line in text.splitlines() if line.startswith("COLUMN_OK ")]
	assert len(lines) == 2, "both calls must finish full output equality"
	valid = valid_count(kind, rows, layout)
	for line in lines:
		assert f"completed=true; rows={rows}; valid_rows={valid}; attempts=1; replays=0; refusal=None" in line, line
		assert "path=avx2;" in line or "path=scalar-primitives;" in line, line


def verify_cached(record, committed, role, identity):
	case = Path(committed).parent
	kind, rows, layout, _ = cli_cache.case_key(record)
	assert digest(case / "program.rn") == record["fixture_sha256"]
	assert (case / "program.rn").read_text() == fixture(kind, rows, layout)
	config = json.loads((case / "CONFIG.json").read_text())
	assert config["sha256"] == role["sha256"]
	assert config["budget"] == identity["budget"] and config["calls"] == identity["calls"]
	assert config["symbols"] == role["symbols"]
	validate((case / "direct.stdout").read_text(), kind, rows, layout)
	validate((case / "program.stdout").read_text(), kind, rows, layout)
	assert json.loads((case / "GDB.json").read_text())["status"] == "PASS"
	ready = json.loads((case / "READINESS.json").read_text())
	assert ready["status"] == "READY" and ready == record["readiness"]
	assert ready["cooldown"]["seconds"] == 120
	assert ready["limits"] == {"max_load": 2, "min_available_bytes": 8 * 1024**3}
	assert len(record["costs"]) == 2
	for number, cost in enumerate(record["costs"], 1):
		part = case / f"map.callgrind.{number}"
		assert digest(part) == cost["sha256"]
		data = callgrind.parse(part)
		assert data["total_Ir"] == cost["Ir"] and cost["Ir_per_row"] == cost["Ir"] / rows
		def total(fragment):
			return sum(v["self_Ir"] for k, v in data["functions"].items() if fragment in k)
		assert total("rnx_polars::column::apply_joined") > 0
		assert total("experimental_core::column::run") > 0
		for forbidden in ("rnx::column::prepare::compile", "rnx_polars::column::script::column_kernel", "new_from_index_polars_core__series"):
			assert total(forbidden) == 0


def case_readiness(lease, path):
	"""One fixed cooldown followed by one sample; rejection is terminal."""
	cooldown = {"seconds": 120, "start_unix": time.time(), "start_monotonic": time.monotonic()}
	time.sleep(120)
	cooldown.update(end_unix=time.time(), end_monotonic=time.monotonic())
	try:
		receipt = lease.ready(max_load=2, min_available_bytes=8 * 1024**3)
	except ValueError as error:
		if hasattr(error, "receipt"):
			error.receipt["cooldown"] = cooldown
			path.write_text(json.dumps(error.receipt, indent=2) + "\n")
		raise
	receipt["cooldown"] = cooldown
	path.write_text(json.dumps(receipt, indent=2) + "\n")
	return receipt


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--builds", type=Path, required=True)
	parser.add_argument("--out", type=Path, required=True)
	parser.add_argument("--valgrind-root", type=Path, required=True)
	parser.add_argument("--mode", choices=["preflight", "large"], required=True)
	parser.add_argument("--preflight", type=Path)
	parser.add_argument("--resume-from", type=Path)
	parser.add_argument("--notify", action="store_true")
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	start = time.monotonic()
	result = {"status": "FAIL", "boundary": "production CLI whole map, all threads", "records": [], "cases": []}
	try:
		policy_path = HERE / "CK2.json"
		policy = json.loads(policy_path.read_text())
		result["negative_controls"] = cli_rules.controls(policy)
		build = json.loads(args.builds.read_text())
		assert build["status"] == "PASS"
		roles = build["builds"]
		assert [r["role"] for r in roles] == ["baseline", "candidate"]
		pins = {str(p): digest(p) for p in HERE.glob("cli_*.py")}
		pins[str(policy_path)] = digest(policy_path)
		pins[str(args.builds.resolve())] = digest(args.builds)
		for role in roles:
			assert digest(role["binary"]) == role["sha256"]
			role["symbols"] = symbols(role["binary"])
			pins[role["binary"]] = role["sha256"]
		if args.mode == "large":
			assert args.preflight, "expanded preflight receipt required"
			preflight = json.loads(args.preflight.read_text())
			assert preflight["status"] == "PASS"
			expected = {(k, l, r) for k in KERNELS for l in LAYOUTS for r in ("baseline", "candidate")}
			actual = [(r["kind"], r["layout"], r["role"]) for r in preflight["records"]]
			assert len(actual) == len(expected) and set(actual) == expected
			for path in [HERE / name for name in ("cli_fixtures.py", "cli_child.py", "cli_gdb.py")] + [args.builds.resolve()] + [Path(r["binary"]) for r in roles]:
				assert preflight["pins"][str(path)] == digest(path), ("preflight input changed", str(path))
			pins[str(args.preflight.resolve())] = digest(args.preflight)
		identity = {"builds": digest(args.builds), "policy": digest(policy_path),
			"inputs": {name: digest(HERE / name) for name in ("cli_fixtures.py", "cli_child.py", "cli_gdb.py")},
			"binaries": {role["role"]: role["sha256"] for role in roles},
			"budget": 1000000000, "calls": 2, "threads": 1,
			"valgrind": digest(args.valgrind_root / "bin/valgrind"),
			"readiness": {"cooldown_seconds": 120, "max_load": 2, "min_available_bytes": 8 * 1024**3}}
		if args.resume_from:
			pins[str(args.resume_from.resolve())] = digest(args.resume_from)
		cached = cli_cache.resume_index(args.resume_from, identity) if args.resume_from else {}
		result.update(pins=pins, builds=roles, identity=identity)
		cli_cache.atomic_json(out / "MANIFEST.json", {"identity": identity, "pins": pins, "resume_from": str(args.resume_from) if args.resume_from else None})
		pins[str(out / "MANIFEST.json")] = digest(out / "MANIFEST.json")
		cli_cache.atomic_json(out / "INDEX.json", {"identity": identity, "cases": []})
		def exact():
			assert all(digest(p) == sha for p, sha in pins.items()), "capture inputs changed"
		env = dict(os.environ, LC_ALL="C", POLARS_MAX_THREADS="1", PYTHONDONTWRITEBYTECODE="1")
		with runtime.lease("/tmp/rnx-runtime-bench.lock", 2) as lease:
			result["lease"] = lease.receipt
			for rows in ([4096] if args.mode == "preflight" else policy["rows"]):
				for kind in KERNELS:
					for layout in LAYOUTS:
						for role in roles:
							exact()
							key = (kind, rows, layout, role["role"])
							if key in cached:
								record, committed = cached[key]
								verify_cached(record, committed, role, identity)
								result["records"].append(record)
								result["cases"].append(committed)
								cli_cache.atomic_json(out / "INDEX.json", {"identity": identity, "cases": result["cases"]})
								continue
							case = out / f"{kind}-{rows}-{layout}-{role['role']}"
							case.mkdir()
							source = case / "program.rn"
							source.write_text(fixture(kind, rows, layout))
							config = dict(role, out=str(case), fixture=str(source), valgrind_root=str(args.valgrind_root.resolve()), calls=2, budget=1000000000, gdb_receipt=str(case / "GDB.json"))
							config_path = case / "CONFIG.json"
							config_path.write_text(json.dumps(config, indent=2) + "\n")
							observation = case_readiness(lease, case / "READINESS.json")
							run = lease.execute([role["binary"], "run", "--debug-engine", "--budget", "1000000000", str(source)], cwd=ROOT, env=env, stdout_path=case / "direct.stdout", stderr_path=case / "direct.stderr", timeout=1800, deadline=start + 43200)
							assert run["status"] == 0, case
							validate((case / "direct.stdout").read_text(), kind, rows, layout)
							run = lease.execute([sys.executable, "-B", str(HERE / "cli_child.py"), str(config_path)], cwd=ROOT, env=env, stdout_path=case / "child.stdout", stderr_path=case / "child.stderr", timeout=4000, deadline=start + 43200)
							assert run["status"] == 0, case
							validate((case / "program.stdout").read_text(), kind, rows, layout)
							parts = sorted(case.glob("map.callgrind.*"))
							assert len(parts) == 2
							costs = []
							for part in parts:
								data = callgrind.parse(part)
								def cost(fragment):
									return sum(v["self_Ir"] for k, v in data["functions"].items() if fragment in k)
								assert cost("rnx_polars::column::apply_joined") > 0
								assert cost("experimental_core::column::run") > 0
								for forbidden in ["rnx::column::prepare::compile", "rnx_polars::column::script::column_kernel", "new_from_index_polars_core__series"]:
									assert cost(forbidden) == 0, forbidden
								costs.append({"Ir": data["total_Ir"], "Ir_per_row": data["total_Ir"] / rows, "profile": str(part), "sha256": digest(part)})
							assert json.loads((case / "GDB.json").read_text())["status"] == "PASS"
							record = {"kind": kind, "rows": rows, "layout": layout, "role": role["role"], "costs": costs, "fixture_sha256": digest(source), "readiness": observation}
							cli_cache.atomic_json(case / "CAPTURE.json", record)
							result["cases"].append(cli_cache.commit_case(case, record, identity))
							cli_cache.atomic_json(out / "INDEX.json", {"identity": identity, "cases": result["cases"]})
							result["records"].append(record)
							exact()
		if args.mode == "large":
			result["acceptance"] = cli_rules.evaluate(result["records"], policy)
			result["status"] = result["acceptance"]["status"]
		else:
			result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		if "case" in locals() and not (case / "COMMITTED.json").exists():
			failure = {"status": "FAIL", "error": repr(error), "case": str(case)}
			cli_cache.atomic_json(case / "FAILED.json", failure)
			if "identity" in locals():
				cli_cache.atomic_json(out / "INDEX.json", {"identity": identity, "cases": result["cases"], "failures": [failure]})
		if hasattr(error, "receipt"):
			result["rejected_readiness"] = error.receipt
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - start
		cli_cache.atomic_json(out / "RESULT.json", result)
		notice = out / "NOTICE.txt"
		notice.write_text(f"CK2 CLI {args.mode} capture EXIT {result['status']}. Receipt: {out / 'RESULT.json'}\n")
		if args.notify:
			subprocess.run(["chatctl", "send", "--as", "sluice", "--to", "codex", "--conversation", "rune-native", "--summary", f"CK2 CLI {args.mode} capture EXIT {result['status']}", "--body-file", str(notice)], check=True)
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
