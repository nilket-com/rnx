"""Cheap controller controls run before builds, tests, or measurements."""
import importlib.util
from pathlib import Path
import unittest


def module(name):
	spec = importlib.util.spec_from_file_location("column_" + name, Path(__file__).with_name(name + ".py"))
	loaded = importlib.util.module_from_spec(spec)
	spec.loader.exec_module(loaded)
	return loaded


class Protocol(unittest.TestCase):
	def test_cross_recovery_rejects_missing_failed_or_wrong_stage(self):
		import copy
		r = module('recover_cross')
		prior = {'status': 'FAIL', 'checks': [dict(name=n, exit=0) for n in r.PASSED_TAIL] + [{'name': 'cross', 'exit': 1}], 'reused_checks': [{'name': 'polars-suite', 'exit': 0}]}
		r.validate_tail(prior)
		for index in range(3):
			bad = copy.deepcopy(prior)
			bad['checks'][index]['exit'] = 1
			with self.assertRaises(AssertionError):
				r.validate_tail(bad)
		for name in ('polars-suite', 'production-cli-build'):
			bad = copy.deepcopy(prior)
			bad['checks'][-1]['name'] = name
			with self.assertRaises(AssertionError):
				r.validate_tail(bad)

	def test_completed_suite_needs_exact_success_and_prior_identity(self):
		import copy
		r = module('resume')
		prior = {'candidate': 'source'}
		done = {'status': 'FAIL', 'error': "AssertionError('source changed')", 'candidate': 'source', 'prior_sha256': 'hash', 'checks': [{'name': 'polars-suite', 'exit': 0, 'command': ['test']}]}
		r.validate_completed_suite(done, prior, 'hash', ['test'])
		for key, value in [('candidate', 'other'), ('prior_sha256', 'other'), ('error', 'other'), ('status', 'PASS')]:
			bad = copy.deepcopy(done)
			bad[key] = value
			with self.subTest(key=key), self.assertRaises(AssertionError):
				r.validate_completed_suite(bad, prior, 'hash', ['test'])
		bad = copy.deepcopy(done)
		bad['checks'][0]['exit'] = 101
		with self.assertRaises(AssertionError):
			r.validate_completed_suite(bad, prior, 'hash', ['test'])

	def test_resume_cannot_skip_failed_or_changed_prior_stages(self):
		import copy
		steps = [('first', ['first']), ('polars-suite', ['test'])]
		prior = {'status': 'FAIL', 'checks': [
			{'name': 'first', 'command': ['first'], 'exit': 0},
			{'name': 'polars-suite', 'command': ['test'], 'exit': 101}]}
		resume = module('resume')
		self.assertEqual(resume.reusable_prefix(prior, steps), 1)
		for field, value in [('exit', 1), ('command', ['changed']), ('name', 'other')]:
			bad = copy.deepcopy(prior)
			bad['checks'][0][field] = value
			with self.subTest(field=field), self.assertRaises(AssertionError):
				resume.reusable_prefix(bad, steps)
		with self.assertRaises(AssertionError):
			resume.reusable_prefix(dict(prior, status='PASS'), steps)

	def test_cli_rejects_wrong_routing_skips_counts_paths_and_caught_fuel(self):
		module("cli").controls()

	def test_fresh_command_plan_preserves_inherited_gates_and_early_lints(self):
		steps = module("accept").commands(Path("/unused/acceptance"))
		names = [name for name, _ in steps]
		inherited = {"format", "pin-controls", "benchmark-protocol", "clippy", "inventory",
			"root-tests", "async-test-support", "generated-projects", "cross"}
		inherited.update("adapter-" + name for name in ("candle", "polars", "postgres"))
		for manifest in module("accept").MANIFESTS:
			inherited.add("fetch-" + manifest.replace("/", "-"))
		for manifest in module("accept").MANIFESTS[-3:]:
			inherited.add("check-" + manifest.replace("/", "-"))
		self.assertEqual(len(inherited), 23)
		self.assertTrue(inherited.issubset(names))
		self.assertEqual(len(names), len(set(names)))
		for name in ("column-host", "column-adapter-unit", "column-adapter-integration",
			"polars-suite", "production-cli-build", "production-cli-controls"):
			self.assertIn(name, names)
			self.assertLess(names.index("adapter-clippy"), names.index(name))
		self.assertLess(names.index("format"), names.index("clippy"))
		self.assertLess(names.index("polars-inventory-preflight"), names.index("clippy"))
		self.assertLess(names.index("adapter-clippy"), names.index("polars-inventories"))
		self.assertEqual(names.index("polars-inventories") + 1, names.index("polars-suite"))
		self.assertLess(names.index("production-cli-build"), names.index("production-cli-controls"))
		build = dict(steps)["production-cli-build"]
		self.assertIn("--locked", build)
		self.assertIn("--release", build)
		self.assertNotIn("test-support", " ".join(build))


if __name__ == "__main__":
	unittest.main()
