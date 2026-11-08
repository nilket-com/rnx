import unittest
from portable_vectors import check, DRIVER, OLD, NEW


class PortableVectors(unittest.TestCase):
	def test_exact_redistribution_and_negative_controls(self):
		for rows in (4096, 65536):
			blocks = rows // 1024
			for temperature, calls in (('first', 1), ('warm', 2)):
				a = {DRIVER: (100, calls), OLD: (30 * blocks, calls * blocks)}
				b = {DRIVER: (106 + 2 * blocks, calls), NEW: (28 * blocks, calls * blocks)}
				self.assertFalse(check(a, b, rows, temperature))
				for name in b:
					for ir, call in ((1, 0), (blocks, 0), (0, 1)):
						bad = dict(b); x = bad[name]; bad[name] = (x[0] + ir, x[1] + call)
						self.assertTrue(check(a, bad, rows, temperature))
				bad = dict(b); bad['unreviewed'] = (1, 0)
				self.assertTrue(check(a, bad, rows, temperature))
				# Redistributing an extra instruction into the driver and removing it
				# from the helper cannot hide behind the unchanged +6 total.
				bad = dict(b)
				bad[DRIVER] = (b[DRIVER][0] + 1, calls)
				bad[NEW] = (b[NEW][0] - 1, calls * blocks)
				self.assertTrue(check(a, bad, rows, temperature))


if __name__ == '__main__':
	unittest.main()
