"""Negative controls for the reviewed old-path policy."""
import hashlib
from pathlib import Path
import tempfile
import unittest

from old_path_score import evidence, interval, layout_delta, routine, score, slope_pass


class ScoreControls(unittest.TestCase):
	def setUp(self):
		self.tmp = tempfile.TemporaryDirectory()
		self.addCleanup(self.tmp.cleanup)
		self.root = Path(self.tmp.name)
		(self.root / "proof").write_text("reviewed")
		self.pins = {"proof": hashlib.sha256(b"reviewed").hexdigest()}
		self.policy = {"review": "control", "binaries": {"published": "a" * 64, "candidate": "b" * 64},
			"cases": [["example", "execution-on"]], "evidence": self.pins, "constants": {}}
		self.samples = []
		for n in (1, 5):
			for role in ("published", "candidate"):
				for t in range(3):
					self.samples.append({"key": ["example", "execution-on", n, role, t],
						"binary_sha256": self.policy["binaries"][role],
						"accounting": {"rnx_Ir": 100 * n, "other_Ir": 30 * n, "dynamic_Ir": 10 * n,
							"rnx_functions": {"work": 100 * n}}})

	def change(self, name, amount, n=None):
		for s in self.samples:
			if s["key"][3] == "candidate" and (n is None or s["key"][2] == n):
				s["accounting"][name] += amount

	def verdict(self):
		return score(self.samples, self.policy, self.root)["status"]

	def test_equal_passes_and_growth_fails(self):
		self.assertEqual(self.verdict(), "PASS")
		self.change("rnx_Ir", 1, 5)
		self.assertEqual(self.verdict(), "FAIL")

	def test_constant_requires_function_and_hash(self):
		self.change("rnx_Ir", 12)
		for s in self.samples:
			if s["key"][3] == "candidate":
				s["accounting"]["rnx_functions"]["cold_setup"] = 12
		self.assertEqual(self.verdict(), "FAIL")
		self.policy["constants"]["example/execution-on"] = {"functions": {"cold_setup": 12}, "evidence": self.pins}
		self.assertEqual(self.verdict(), "PASS")
		self.policy["constants"]["example/execution-on"]["evidence"] = {}
		with self.assertRaisesRegex(ValueError, "missing attribution"):
			self.verdict()

	def test_wrong_attribution_fails(self):
		self.policy["constants"]["example/execution-on"] = {"functions": {"absent": 12}, "evidence": self.pins}
		self.assertEqual(self.verdict(), "FAIL")

	def test_unknown_outside_spread_fails(self):
		self.change("other_Ir", 1)
		self.assertEqual(self.verdict(), "FAIL")

	def test_system_slope_not_constant_is_gated(self):
		self.change("dynamic_Ir", 5000)
		self.assertEqual(self.verdict(), "PASS")
		self.change("dynamic_Ir", 1, 5)
		self.assertEqual(self.verdict(), "FAIL")

	def test_missing_hash_and_wrong_binary_fail(self):
		with self.assertRaises(ValueError):
			evidence(self.root, {})
		with self.assertRaises(ValueError):
			evidence(self.root, {"proof": "0" * 64})
		self.samples[0]["binary_sha256"] = "c" * 64
		with self.assertRaises(ValueError):
			self.verdict()

	def test_native_off_exact(self):
		self.policy["cases"] = [["example", "execution-off"]]
		for s in self.samples:
			s["key"][1] = "execution-off"
		self.assertEqual(self.verdict(), "PASS")
		self.change("rnx_Ir", -1)
		self.assertEqual(self.verdict(), "FAIL")

	def test_layout_counts_residual_and_positive(self):
		a = {"calls": 10, "callers": {"lookup": 10}, "self_Ir": 100, "taken": 0}
		b = {"calls": 10, "callers": {"lookup": 10}, "self_Ir": 124, "taken": 3}
		rule = {"extra_Ir_per_taken": 8}
		self.assertEqual(layout_delta(a, b, rule), 24)
		for field, value in [("calls", 11), ("callers", {"lookup": 9, "other": 1}), ("self_Ir", 125), ("taken", 2)]:
			bad = dict(b, **{field: value})
			with self.subTest(field=field), self.assertRaises(ValueError):
				layout_delta(a, bad, rule)

	def test_observed_envelope_uses_all_samples(self):
		self.assertEqual(interval([100, 110, 100], [105, 115, 105]), (-5, 15))
		self.assertTrue(slope_pass((-5, 15), (15, 20)))
		self.assertFalse(slope_pass((-5, 15), (16, 20)))

	def test_profile_branch_and_caller_parser(self):
		p = self.root / "profile"
		p.write_text("fn=(1) lookup\ncfn=(2) cmp\ncalls=10 0x100 1\n0x40 1 124\nfn=(2) cmp\n0x100 1 10\n+4 2 10\njcnd=3/10 0x120 3\n* *\n+4 3 104\n")
		self.assertEqual(routine(p, "cmp", 0x104), {"calls": 10, "callers": {"lookup": 10}, "self_Ir": 124, "taken": 3})


if __name__ == "__main__":
	unittest.main()
