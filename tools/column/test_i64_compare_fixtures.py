import unittest
from i64_compare_fixtures import specs


class Fixtures(unittest.TestCase):
	def test_complete_and_direction_sensitive(self):
		rows = specs()
		self.assertEqual(len(rows), 18)
		self.assertEqual(rows['i64lt_right'][3], 'false')
		self.assertEqual(rows['i64lt_left'][3], 'true')
		self.assertEqual(rows['i64ge_right'][3], 'true')
		self.assertEqual(rows['i64ge_left'][3], 'false')
		for name, row in rows.items():
			self.assertEqual(len(row[2]), len(row[4]))
			self.assertEqual(len(row[2]), 2 if name.endswith('_pair') else 1)


if __name__ == '__main__':
	unittest.main()
