"""ADR 0227 fail-closed controls for every pin and absolute call count."""
import copy
import unittest
import huge_page_vectors as p


class Controls(unittest.TestCase):
	def test_each_pin(self):
		for key in p.POLICY['rules']:
			kind, layout = key.split('/')
			for rows, call in __import__('itertools').product((4096, 65536, 10000000, 100000000), range(2)):
				rules = p.rules_for(kind, layout, rows, call)
				a, b = {}, {}
				for name, pin in rules.items():
					delta, calls = p.expected(pin, rows, layout)
					a[name] = (1000000000, calls[0])
					b[name] = (1000000000 + delta, calls[1])
				self.assertEqual(p.check_vectors(a, b, kind, layout, rows, call), {})
				for name in rules:
					bad = dict(b); bad[name] = (b[name][0] + 1, b[name][1])
					self.assertIn(name, p.check_vectors(a, bad, kind, layout, rows, call))
					bad = dict(b); bad[name] = (b[name][0], b[name][1] + 1)
					self.assertIn(name, p.check_vectors(a, bad, kind, layout, rows, call))
				b['unreviewed'] = (1, 1)
				self.assertIn('unreviewed', p.check_vectors(a, b, kind, layout, rows, call))

	def test_pair(self):
		p.verify_pair(p.POLICY['builds'])
		for field, length in (('sha256', 64), ('source_commit', 40)):
			bad = copy.deepcopy(p.POLICY['builds']); bad[1][field] = '0' * length
			with self.assertRaises(ValueError): p.verify_pair(bad)

	def test_small_observed_calls_and_partial_blocks(self):
		self.assertEqual(p.execute_counts(4096, 'flat'), (4, 0))
		self.assertEqual(p.execute_counts(4096, 'sliced-chunked-null'), (32, 0))
		self.assertEqual(p.execute_counts(4096, 'alternating'), (0, 68))
		self.assertEqual(p.execute_counts(4096, 'deterministic-random'), (1, 66))
		self.assertEqual(p.execute_counts(1025, 'flat'), (2, 0))
		self.assertEqual(p.expected({'formula': 'pair'}, 1025, 'flat'), (-8, [2, 2]))


if __name__ == '__main__':
	unittest.main()
