import unittest
from admission import admit, GIB


class Admission(unittest.TestCase):
	def test_measured_peak_with_margin(self):
		r = admit(32, 50 * GIB, 50 * GIB, 3288178688)
		self.assertEqual(r['per_worker_reservation_bytes'], 5 * GIB)
		self.assertEqual(r['workers'], 9)

	def test_ancestor_limit(self):
		self.assertEqual(admit(32, 50 * GIB, 13 * GIB, 3288178688)['workers'], 2)

	def test_cpu_and_override_only_reduce(self):
		self.assertEqual(admit(2, 50 * GIB, 50 * GIB, 3288178688, 20)['workers'], 2)
		self.assertEqual(admit(32, 50 * GIB, 50 * GIB, 3288178688, 1)['workers'], 1)

	def test_no_admission_when_one_cannot_fit(self):
		with self.assertRaises(ValueError):
			admit(32, 6 * GIB, 50 * GIB, 3288178688)


if __name__ == '__main__':
	unittest.main()
