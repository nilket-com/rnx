import unittest
from partitions import balanced, validate


class Partitions(unittest.TestCase):
	def test_union_includes_ignored(self):
		self.assertEqual(validate(['run', 'ignored'], [['run'], ['ignored']]), [['run'], ['ignored']])

	def test_bad_union(self):
		for shards in ([['a']], [['a'], ['a', 'b']], [['a', 'b', 'foreign']], [[], ['a', 'b']]):
			with self.subTest(shards=shards), self.assertRaises(ValueError):
				validate(['a', 'b'], shards)

	def test_pack_from_measured_durations(self):
		durations = {'a': 60, 'b': 50, 'c': 10, 'ignored': 0}
		shards = balanced(list(durations), durations, 2)
		self.assertEqual([sum(durations[n] for n in s) for s in shards], [60, 60])

	def test_missing_timings_rejected(self):
		with self.assertRaises(ValueError):
			balanced(['a', 'b'], {'a': 1}, 2)


if __name__ == '__main__':
	unittest.main()
