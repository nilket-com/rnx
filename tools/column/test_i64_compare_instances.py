import unittest
from collections import Counter
from i64_compare_instances import ADAPTERS, PREFIX, PREPARE, evaluate


class Instances(unittest.TestCase):
	def test_adapters_only_once_and_not_generic(self):
		base = Counter({PREFIX + 'run::<T>': 1, PREPARE: 1})
		for name in ADAPTERS:
			self.assertEqual(evaluate(base, base + Counter({name: 1}))['status'], 'PASS')
			for extra in (Counter({name: 2}), Counter({name + '::<T>': 1})):
				self.assertEqual(evaluate(base, base + extra)['status'], 'FAIL')

	def test_duplicate_driver_new_tuple_and_other_helper_fail(self):
		base = Counter({PREFIX + 'run::<T>': 1, PREPARE: 1})
		for name in (PREFIX + 'run::<T>', PREFIX + 'run::<U>', PREFIX + 'new_helper'):
			self.assertEqual(evaluate(base, base + Counter({name: 1}))['status'], 'FAIL')

	def test_preparation_is_exactly_one_non_generic_frame(self):
		base = Counter({PREFIX + 'run::<T>': 1})
		self.assertEqual(evaluate(base, base + Counter({PREPARE: 1}))['status'], 'PASS')
		for extra in (Counter(), Counter({PREPARE: 2}), Counter({PREPARE + '::<T>': 1})):
			self.assertEqual(evaluate(base, base + extra)['status'], 'FAIL')


if __name__ == '__main__':
	unittest.main()
