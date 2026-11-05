"""Admission and determinism controls; no expensive capture is launched here."""
import unittest
from ir_resources import compare_determinism, launch_budget


class Controls(unittest.TestCase):
	def test_memory_and_affinity_both_bound_workers(self):
		gib = 1024**3
		v = launch_budget(range(32), 64*gib, 16*gib, 4*gib, 2*gib)
		self.assertEqual(v["memory_max_bytes"], 14*gib)
		self.assertEqual(v["workers"], 2)
		self.assertEqual(launch_budget([0], 64*gib, 64*gib, 4*gib, 0)["workers"], 1)

	def test_insufficient_memory_fails(self):
		with self.assertRaises(ValueError):
			launch_budget([0, 1], 100, 100, 100, 1)

	def test_one_instruction_drift_fails(self):
		self.assertEqual(compare_determinism({"a": [10, 9]}, {"a": [10, 9]})["status"], "PASS")
		self.assertEqual(compare_determinism({"a": [10, 9]}, {"a": [10, 10]})["status"], "FAIL")
		with self.assertRaises(ValueError):
			compare_determinism({"a": [10, 9]}, {})

	def test_capture_environment_does_not_inherit_worker_variables(self):
		import os
		from unittest.mock import patch
		from ir_pool import capture_environment, slot_name
		before = capture_environment()
		with patch.dict(os.environ, {"MALLOC_PERTURB_": "123", "WORKER_ID": "varying"}):
			self.assertEqual(capture_environment(), before)
		self.assertIn("glibc.malloc.mmap_threshold=131072", before["GLIBC_TUNABLES"])
		self.assertEqual(len(slot_name("i64add", 4096, "flat", "baseline")), 64)
		self.assertEqual(len(slot_name("f64add", 100000000, "mixed", "candidate")), 64)

	def test_optimized_python_refuses(self):
		import subprocess
		import sys
		from pathlib import Path
		p = subprocess.run([sys.executable, "-O", str(Path(__file__).with_name("ir_pool.py")), "--help"], capture_output=True, text=True)
		self.assertNotEqual(p.returncode, 0)
		self.assertIn("requires assertions", p.stderr)

	def test_tooling_failure_writes_receipt_and_sends_notice(self):
		import json
		import tempfile
		from pathlib import Path
		from unittest.mock import patch
		import ir_pool
		with tempfile.TemporaryDirectory() as tmp:
			out = Path(tmp) / "failed-setup"
			argv = ["ir_pool.py", "--rune", tmp, "--builds", tmp, "--out", str(out),
				"--valgrind", tmp, "--resources", tmp, "--mode", "preflight"]
			with patch("sys.argv", argv), patch.object(ir_pool, "tooling", side_effect=RuntimeError("injected tooling setup failure")), patch.object(ir_pool.subprocess, "run") as send:
				self.assertEqual(ir_pool.main(), 1)
				send.assert_called_once()
				self.assertEqual(send.call_args.args[0][:2], ["chatctl", "send"])
				self.assertIn(str(out / "NOTICE.txt"), send.call_args.args[0])
			self.assertEqual(json.loads((out / "RESULT.json").read_text())["status"], "FAIL")
			self.assertIn("injected tooling setup failure", (out / "RESULT.json").read_text())
			self.assertIn("EXIT FAIL", (out / "NOTICE.txt").read_text())

	def test_aslr_wrapper_covers_direct_and_child_processes(self):
		from ir_pool import measured_command
		identity = {"aslr": {"disabled": True, "setarch": "/usr/bin/setarch", "machine": "x86_64"}}
		for command in [["binary", "run"], ["python3", "cli_child.py", "CONFIG.json"]]:
			self.assertEqual(measured_command(command, identity), ["/usr/bin/setarch", "x86_64", "-R", *command])
		identity["aslr"]["disabled"] = False
		self.assertEqual(measured_command(["binary"], identity), ["binary"])

	def test_object_accounting_does_not_classify_by_symbol_name(self):
		import tempfile
		from pathlib import Path
		from ir_accounting import account, comparison
		with tempfile.TemporaryDirectory() as tmp:
			p = Path(tmp) / "profile"
			binary = Path(tmp) / "rnx"
			def profile(app, libc):
				p.write_text(f"positions: instr line\nevents: Ir\nsummary: {app + libc}\nob=(1) {binary}\nfn=(1) malloc\n0x1 0 {app}\nob=(2) /lib/libc.so.6\nfn=(2) application_like_name\n0x2 0 {libc}\n")
				return account(p, binary)
			a = profile(10, 20)
			self.assertEqual(a["rnx_Ir"], 10)
			self.assertEqual(a["dynamic_Ir"], 20)
			self.assertTrue(comparison({"case": [a]}, {"case": [profile(10, 25)]})["exact_partitions_match"])
			self.assertFalse(comparison({"case": [a]}, {"case": [profile(11, 20)]})["exact_partitions_match"])

	def test_system_band_boundaries(self):
		from ir_accounting import system_band
		self.assertTrue(system_band(4096, 4096)["passes"])
		self.assertFalse(system_band(4097, 4097)["passes"])
		self.assertFalse(system_band(0, 90001)["passes"])

	def test_combined_scoring_keeps_program_growth_and_raw_ceilings(self):
		from ir_accounting import evaluate
		import copy
		records = []
		for rows in [10000000, 100000000]:
			for role in ["baseline", "candidate"]:
				accounting = {"total_Ir": rows + 100, "rnx_Ir": rows, "dynamic_Ir": 100, "other_Ir": 0}
				records.append({"kind": "i64add", "rows": rows, "layout": "flat", "role": role, "costs": [{"accounting": dict(accounting)} for _ in range(2)]})
		policy = {"rows": [10000000, 100000000], "system_band_scope": {"i64add": {"flat": "unchanged"}}}
		def raw(_records, _policy):
			return {"status": "FAIL", "failures": [{"rule": "existing flat zero growth"}]}
		self.assertEqual(evaluate(records, policy, raw)["status"], "PASS")
		changed = copy.deepcopy(records)
		v = changed[1]["costs"][0]["accounting"]
		v["rnx_Ir"] += 1; v["total_Ir"] += 1
		self.assertEqual(evaluate(changed, policy, raw)["failures"][0]["rule"], "existing flat program zero growth")
		def ceiling(_records, _policy):
			return {"status": "FAIL", "failures": [{"rule": "flat ceiling"}]}
		self.assertEqual(evaluate(records, policy, ceiling)["status"], "FAIL")
		changed = copy.deepcopy(records)
		for row in changed:
			if row["role"] == "candidate":
				for cost in row["costs"]:
					cost["accounting"]["dynamic_Ir"] += row["rows"]
					cost["accounting"]["total_Ir"] += row["rows"]
		self.assertEqual(evaluate(changed, policy, raw)["status"], "FAIL")
		policy["system_band_scope"]["i64add"]["flat"] = "changed"
		self.assertEqual(evaluate(changed, policy, raw)["status"], "PASS")
		self.assertEqual(evaluate(changed, policy, ceiling)["status"], "FAIL")
		policy["system_band_scope"] = {}
		with self.assertRaises(ValueError):
			evaluate(records, policy, raw)

