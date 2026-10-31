"""N1: bounded cold analysis iterators keep all ordinary cost checks."""
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import callgrind_rules

COLD = ('<core::ops::range::Range<usize> as core::iter::traits::iterator::Iterator>'
	'::try_fold::<(), core::iter::traits::iterator::Iterator::find::check<usize, '
	'&mut <rune::experimental_core::region::FunctionContext>::analyze_header::{closure#8}'
	'::{closure#0}, core::ops::control_flow::ControlFlow<usize>>')
BY_VALUE = COLD.replace('find::check', 'any::check').replace('&mut ', '').replace('ControlFlow<usize>', 'ControlFlow<()>')
HOT = '<rune::experimental_core::heap::Store>::range_next'
BASE = 'rune::experimental_core::execute::Executor::invoke'


def profiles(name=COLD, calls=6, delta=111, base=1_000_000, extra_whole=0):
	tip = {'total_Ir': base, 'functions': {BASE: {'self_Ir': base, 'calls': 1000}}}
	candidate = {'total_Ir': base + delta + extra_whole,
		'functions': {BASE: {'self_Ir': base, 'calls': 1000}, name: {'self_Ir': delta, 'calls': calls}}}
	if extra_whole:
		candidate['functions']['other::host'] = {'self_Ir': extra_whole, 'calls': 1}
	return tip, candidate


def assess(**kwargs):
	tip, candidate = profiles(**kwargs)
	return callgrind_rules.assess(tip, candidate, tip, invocations=1000, iterations=100000,
		controls={'is': {'core': 0, 'whole': 0}, 'branch': {'core': 0, 'whole': 0}})


class ColdAnalysisIterator(unittest.TestCase):
	def test_cold_cost_is_counted_and_uses_normal_bounds(self):
		for calls in (1, 3, 6, 9, 1002):
			with self.subTest(calls=calls):
				self.assertEqual(assess(calls=calls)['net_Ir'], 111)

	def test_unknown_zero_or_iteration_scale_is_still_hot(self):
		for calls in (None, 0, 1003, 100000):
			with self.subTest(calls=calls):
				with self.assertRaisesRegex(ValueError, 'hot range cost grows'):
					assess(calls=calls)

	def test_real_vm_range_never_gets_cold_treatment(self):
		with self.assertRaisesRegex(ValueError, 'hot range cost grows'):
			assess(name=HOT, calls=1, delta=1)

	def test_another_owner_cannot_use_the_cold_name_shape(self):
		with self.assertRaisesRegex(ValueError, 'hot range cost grows'):
			assess(name=COLD.replace('FunctionContext', 'OtherContext'))

	def test_group_limit_is_unchanged(self):
		with self.assertRaisesRegex(ValueError, 'per-invocation group exceeds'):
			assess(delta=72001, base=100_000_000)

	def test_core_limit_is_unchanged(self):
		with self.assertRaisesRegex(ValueError, 'per-invocation group exceeds'):
			assess(delta=1001, base=1_000_000)

	def test_whole_limit_is_unchanged(self):
		with self.assertRaises(ValueError):
			assess(extra_whole=20000)


class BothAnalysisSpellings(unittest.TestCase):
	def test_positive_and_negative_call_counts_for_both_spellings(self):
		for name in (COLD, BY_VALUE):
			for calls in (1, 1002):
				with self.subTest(name=name, calls=calls):
					self.assertEqual(assess(name=name, calls=calls, delta=38)['net_Ir'], 38)
			for calls in (None, 0, -1, 1003, 100000):
				with self.subTest(name=name, calls=calls):
					with self.assertRaisesRegex(ValueError, 'hot range cost grows'):
						assess(name=name, calls=calls)

	def test_other_owners_and_real_vm_iterators_remain_hot(self):
		for name in (COLD, BY_VALUE):
			for bad in (name.replace('FunctionContext', 'OtherContext'), name.replace('analyze_header', 'execute_loop'), HOT):
				with self.subTest(name=bad):
					with self.assertRaisesRegex(ValueError, 'hot range cost grows'):
						assess(name=bad, calls=1, delta=1)

	def test_ordinary_bounds_apply_to_both_spellings(self):
		for name in (COLD, BY_VALUE):
			for kwargs in (dict(delta=72001, base=100_000_000), dict(delta=1001), dict(extra_whole=20000)):
				with self.subTest(name=name, kwargs=kwargs):
					with self.assertRaises(ValueError):
						assess(name=name, **kwargs)


if __name__ == '__main__':
	unittest.main()
