#!/usr/bin/env python3
"""CK1 exact signed-source kernel gate; consumes a passed prerequisite receipt."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import traceback

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[2]
sys.path.insert(0, str(HERE.parent / "measure"))
import callgrind
import measurement_runtime as runtime
import measurement_readiness as readiness


def digest(path):
	return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument("--prerequisite", required=True, type=Path)
	parser.add_argument("--out", required=True, type=Path)
	parser.add_argument("--valgrind-root", type=Path, default=Path("/home/me/opt/valgrind"))
	parser.add_argument("--notify", action="store_true")
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	started = time.monotonic()
	result = {"status": "FAIL", "record": "CK1", "records": []}
	try:
		pre = json.loads(args.prerequisite.read_text())
		assert pre["status"] == "PASS" and "probe" in pre, "probe prerequisites required"
		candidate = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
		assert pre["commit"] == candidate, "prerequisites must bind the signed commit"
		subprocess.run(["git", "verify-commit", candidate], cwd=ROOT, check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
		def exact():
			assert subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip() == candidate
			assert not subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT, text=True).strip(), "source must remain clean"
			for name, value in pre["source_pins"].items():
				assert digest(Path(pre.get("source_root", ROOT)) / name) == value, name
			assert digest(pre["probe"]["path"]) == pre["probe"]["sha256"]
		exact()
		policy = json.loads((HERE / "CK1.json").read_text())
		assert policy["record"] == "CK1" and policy["kernel"]["rows"] == 100000000
		binary = Path(pre["probe"]["path"])
		vg = args.valgrind_root / "bin/valgrind"
		result.update(candidate=candidate, prerequisite={"path": str(args.prerequisite), "sha256": digest(args.prerequisite)}, policy=policy,
			probe=pre["probe"], source_pins=pre["source_pins"], dependencies=runtime.dependency_snapshot([binary]),
			profiler_tree=runtime.tool_tree(args.valgrind_root), perf_policy=runtime.check_perf_policy(os.environ["HOME"]))
		plan = [(op, rows, mode, profiled) for rows in (4096, 10000000, 100000000) for op in policy["kernel"]["ir_per_element_max"] for mode in (("auto", "scalar") if rows == 4096 else ("auto",)) for profiled in (False, True)]
		(out / "PLAN.json").write_text(json.dumps(plan, indent=2) + "\n")
		env = {"PATH": "/usr/bin:/bin", "HOME": os.environ["HOME"], "LANG": "C", "LC_ALL": "C"}
		checksums, failures = {}, []
		# Fixed compilation cooldown, not a readiness retry loop.
		time.sleep(180)
		with runtime.lease("/tmp/rnx-runtime-bench.lock", 2) as lease:
			result["lease"] = lease.receipt
			for op, rows, mode, profiled in plan:
				time.sleep(5)
				exact()
				before = lease.ready(max_load=2, min_available_bytes=8 * 1024**3)
				name = f'{op}-{rows}-{mode}-{"ir" if profiled else "cycles"}'
				command = [str(binary), op, str(rows), mode]
				if profiled:
					command = [str(vg), "--tool=callgrind", "--instr-atstart=no", "--collect-jumps=yes", "--dump-instr=yes", "--cache-sim=no", "--branch-sim=no", "--callgrind-out-file=" + str(out / (name + ".callgrind")), *command]
				execution = lease.execute(command, cwd=ROOT, env=env, stdout_path=out / (name + ".stdout"), stderr_path=out / (name + ".stderr"), timeout=1800, deadline=started + 14400)
				execution.update(readiness=before, post_observation=readiness.snapshot(2))
				(out / (name + ".execution.json")).write_text(json.dumps(execution, indent=2) + "\n")
				assert execution["status"] == 0, name
				data = json.loads(Path(execution["stdout"]).read_text())
				assert data["status"] == "PASS" and data["rows"] == rows and data["op"] == op
				assert data["path"] == ("Avx2" if mode == "auto" else "Scalar"), name
				assert data["invoke_alloc"]["alloc"] == data["invoke_alloc"]["realloc"] == 0
				checksums.setdefault((op, rows), set()).add(data["checksum"])
				assert len(checksums[(op, rows)]) == 1, "cross-path checksum"
				stable = data["thermal_before"] == data["thermal_after"] and data["thermal_before"] != (1 << 64) - 1
				row = {"name": name, "data": data, "profiled": profiled, "kernel_thermal_stable": stable}
				if profiled:
					parts = sorted(out.glob(name + ".callgrind.*"))
					assert len(parts) == 1, "expected exactly one measured phase"
					parsed = callgrind.parse(parts[0])
					cost = parsed["total_Ir"] / rows
					row["profile"] = {"path": str(parts[0]), "sha256": digest(parts[0]), "whole_measured_phase_Ir": parsed["total_Ir"], "Ir_per_row": cost, "functions": parsed["functions"]}
					if mode == "auto" and rows == policy["kernel"]["rows"]:
						ceiling = policy["kernel"]["ir_per_element_max"][op]
						row["gate"] = {"ceiling": ceiling, "status": "PASS" if cost <= ceiling else "FAIL"}
						if cost > ceiling:
							failures.append({"name": name, "Ir_per_row": cost, "ceiling": ceiling})
				else:
					valid = stable and data["enabled"] == data["running"] and data["enabled"] > 0
					row["cycles_valid"] = valid
					if valid:
						row["user_cycles_per_row"] = data["cycles"] / rows
						row["logical_GBps"] = rows * (0.25 if op == "boolnot" else 24 if op == "axpy" else 16) / data["measured_ns"]
				path = out / (name + ".record.json")
				path.write_text(json.dumps(row, indent=2) + "\n")
				result["records"].append({"name": name, "path": str(path), "sha256": digest(path)})
		exact()
		assert runtime.dependency_snapshot([binary]) == result["dependencies"], "runtime dependencies changed"
		assert runtime.tool_tree(args.valgrind_root) == result["profiler_tree"], "profiler changed"
		result["failures"] = failures
		assert not failures, "kernel ceiling failed; attribute saved profiles read-only before changes"
		result["status"] = "PASS"
	except BaseException as error:
		result["error"] = repr(error)
		(out / "TRACEBACK.txt").write_text(traceback.format_exc())
	finally:
		result["seconds"] = time.monotonic() - started
		receipt = out / "RESULT.json"
		receipt.write_text(json.dumps(result, indent=2) + "\n")
		notice = out / "NOTICE.txt"
		notice.write_text(f'CK1 signed kernel gate EXIT {result["status"]}. Receipt: {receipt}. Attribute saved failure evidence read-only before code/rule changes.\n')
		if args.notify:
			subprocess.run(["chatctl", "send", "--as", "sluice", "--to", "codex", "--conversation", "rune-native", "--summary", f'CK1 signed kernel gate EXIT {result["status"]}', "--body-file", str(notice)], check=True)
		print(notice.read_text().strip())
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
