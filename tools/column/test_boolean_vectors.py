import unittest
from boolean_vectors import compare


class BooleanVectors(unittest.TestCase):
	def test_identical_passes(self):
		self.assertFalse(compare({'execute': (100, 4)}, {'execute': (100, 4)}))

	def test_one_instruction_fails(self):
		self.assertTrue(compare({'execute': (100, 4)}, {'execute': (101, 4)}))

	def test_one_call_fails(self):
		self.assertTrue(compare({'execute': (100, 4)}, {'execute': (100, 5)}))

	def test_redistribution_fails(self):
		self.assertTrue(compare({'execute': (100, 4)}, {'execute': (90, 4), 'helper': (10, 4)}))

	def test_preparation_not_exempt(self):
		self.assertTrue(compare({'invoke': (100, 1)}, {'invoke': (101, 1)}))

class ReviewedPins(unittest.TestCase):
	def vectors(self, pins, rows):
		a, b = {}, {}
		for name, pin in pins.items():
			delta = pin.get('delta_Ir', 2 - (rows + 1023) // 1024)
			base = max(0, -delta) + 1000
			a[name] = (base, pin['calls'][0])
			b[name] = (base + delta, pin['calls'][1])
		return a, b

	def test_every_function_plus_one_fails(self):
		from boolean_pins import POLICY, check_vectors
		for key, pins in POLICY['vectors'].items():
			kind, layout = key.split('/')
			for rows in (4096, 65536, 10000000, 100000000):
				a, b = self.vectors(pins, rows)
				self.assertFalse(check_vectors(a, b, kind, layout, rows)[0])
				for name in pins:
					changed = dict(b)
					changed[name] = (b[name][0] + 1, b[name][1])
					self.assertIn(name, check_vectors(a, changed, kind, layout, rows)[0])

	def test_call_change_and_unpinned_work_fail(self):
		from boolean_pins import POLICY, check_vectors
		key = 'multiply3/flat'; pins = POLICY['vectors'][key]
		a, b = self.vectors(pins, 65536)
		name = next(iter(pins)); b[name] = (b[name][0], b[name][1] + 1)
		self.assertIn(name, check_vectors(a, b, 'multiply3', 'flat', 65536)[0])
		b['unexpected'] = (1, 1)
		self.assertIn('unexpected', check_vectors(a, b, 'multiply3', 'flat', 65536)[0])

	def test_wrong_pair_rejected(self):
		from boolean_pins import POLICY, verify_pair
		builds = [dict(role=r, **v) for r, v in POLICY['binaries'].items()]
		verify_pair(builds)
		builds[0]['sha256'] = '0' * 64
		with self.assertRaises(ValueError): verify_pair(builds)


if __name__ == '__main__':
	unittest.main()
