import unittest
from collections import Counter
from masked_float_instances import SIBLING, evaluate


class Instances(unittest.TestCase):
	def test_exact_sibling_only(self):
		base = Counter({'rune::experimental_core::column::run::f64_plan::run::<T>': 1})
		candidate = base + Counter({SIBLING: 1})
		r = evaluate(base, candidate)
		self.assertEqual(r['status'], 'PASS')
		self.assertEqual(r['raw']['status'], 'FAIL')
		for extra in (SIBLING, SIBLING.replace('&mut ', ''), SIBLING.replace('never_canceled', 'closure'),
			'rune::experimental_core::column::run::operands::execute::<T>', next(iter(base))):
			self.assertEqual(evaluate(base, candidate + Counter({extra: 1}))['status'], 'FAIL')
		self.assertEqual(evaluate(base, base)['status'], 'FAIL')
		self.assertEqual(evaluate(candidate, candidate)['status'], 'FAIL')


if __name__ == '__main__':
	unittest.main()
