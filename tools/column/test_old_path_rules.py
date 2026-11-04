"""Reject the former 1,514-Ir false PASS and independently missing work."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("old_path_rules", Path(__file__).with_name("old_path_rules.py"))
rules = importlib.util.module_from_spec(spec)
spec.loader.exec_module(rules)


class OldPathRules(unittest.TestCase):
	def profile(self):
		return {"total_Ir": 10000, "functions": {
			"<rune::runtime::vm::Vm>::run": {"calls": 64, "self_Ir": 3000},
			"rnx_polars::generated::support::callback::bridge::<(i64,), i64>": {"calls": 64, "self_Ir": 2000},
			"std::sys::backtrace::__rust_begin_short_backtrace::<rnx_polars::engine::run<in_place_polars_core__datatypes__int64chunked>>": {"calls": 1, "self_Ir": 400},
		}}

	def test_original_vacuous_profile_is_rejected(self):
		bad = {"total_Ir": 1514, "functions": {
			"<rune::runtime::vm::Vm>::run": {"self_Ir": 291, "calls": 1}}}
		with self.assertRaises(ValueError):
			rules.require_workload(bad, "callback", 64)
		with self.assertRaises(ValueError):
			rules.compare_growth([1514, 1514], [1514, 1514])

	def test_callback_worker_and_call_count_each_required(self):
		rules.require_workload(self.profile(), "callback", 64)
		for name in self.profile()["functions"]:
			bad = self.profile()
			bad["functions"][name] = {"calls": 0, "self_Ir": 0}
			with self.subTest(missing=name), self.assertRaises(ValueError):
				rules.require_workload(bad, "callback", 64)
		with self.assertRaises(ValueError):
			rules.require_workload(self.profile(), "callback", 65)

	def test_growth_guard_preserves_nonpositive_extra_rule(self):
		self.assertTrue(rules.compare_growth([100, 200], [130, 230])["pass"])
		self.assertTrue(rules.compare_growth([100, 200], [130, 229])["pass"])
		self.assertFalse(rules.compare_growth([100, 200], [130, 231])["pass"])
		with self.assertRaises(ValueError):
			rules.compare_growth([100, 200], [130, 130])
		with self.assertRaises(ValueError):
			rules.compare_growth([100, 100], [130, 230])


if __name__ == "__main__":
	unittest.main()
