import unittest
from boolean_limits import limits
from boolean_vectors import EXACT, POSITIVE, LAYOUTS

SIZES = (10000000, 100000000)


def fixture():
	out = {}
	for k in EXACT + POSITIVE:
		for l in LAYOUTS:
			for n in SIZES:
				for role in ('baseline', 'candidate'):
					cost = 1 if k in POSITIVE else 2
					out[k, l, n, role] = {'costs': [dict(Ir=n * cost, Ir_per_row=cost, accounting={'dynamic_Ir': 100}) for _ in range(2)]}
	return out


class BooleanLimits(unittest.TestCase):
	def test_baseline(self):
		self.assertFalse(limits(fixture(), EXACT, POSITIVE, LAYOUTS, SIZES))

	def test_pair_and_chain_ceiling_plus_one(self):
		for k in POSITIVE:
			for l in LAYOUTS:
				x = fixture(); n = SIZES[0]
				cap = (2.5 if l == 'flat' else 5) if k == 'boolchain' else (2 if l == 'flat' else 4)
				cost = x[k, l, n, 'candidate']['costs'][0]
				cost.update(Ir=int(n * cap + 1), Ir_per_row=(n * cap + 1) / n)
				self.assertTrue(any(f['rule'] == 'whole-map ceiling' for f in limits(x, EXACT, POSITIVE, LAYOUTS, SIZES)))

	def test_system_slope_and_constant(self):
		for deltas in ((0, 100000), (4097, 4097)):
			data = fixture()
			for n, delta in zip(SIZES, deltas):
				data['i64add', 'flat', n, 'candidate']['costs'][0]['accounting']['dynamic_Ir'] += delta
			self.assertTrue(any(f['rule'] == 'unchanged system band' for f in limits(data, EXACT, POSITIVE, LAYOUTS, SIZES)))

	def test_protected_numeric_ceiling(self):
		data = fixture(); n = SIZES[0]
		cost = data['f64add', 'flat', n, 'candidate']['costs'][0]
		cost.update(Ir=n * 5, Ir_per_row=5)
		self.assertTrue(any(f['rule'] == 'whole-map ceiling' and f['kind'] == 'f64add' for f in limits(data, EXACT, POSITIVE, LAYOUTS, SIZES)))


if __name__ == '__main__':
	unittest.main()
