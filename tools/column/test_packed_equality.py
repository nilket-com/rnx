"""Negative controls for ADR 0226 exact pins and raw ceilings."""
import copy
import unittest
import packed_equality as p


class Controls(unittest.TestCase):
	def test_pins_and_one_instruction(self):
		for kind, rules in p.POLICY['rules'].items():
			a = {n: (100, r['calls'][0]) for n, r in rules.items()}
			b = {n: (100 + r['constant_Ir'], r['calls'][1]) for n, r in rules.items()}
			self.assertEqual(p.check_vectors(a, b, kind), {})
			for n in rules:
				bad = dict(b); bad[n] = (b[n][0] + 1, b[n][1])
				self.assertIn(n, p.check_vectors(a, bad, kind))
				bad = dict(b); bad[n] = (b[n][0], b[n][1] + 1)
				self.assertIn(n, p.check_vectors(a, bad, kind))
			b['unexpected'] = (1, 1)
			self.assertIn('unexpected', p.check_vectors(a, b, kind))

	def test_pair(self):
		p.verify_pair(p.POLICY['builds'])
		bad = copy.deepcopy(p.POLICY['builds']); bad[1]['sha256'] = '0' * 64
		with self.assertRaises(ValueError): p.verify_pair(bad)
		bad = copy.deepcopy(p.POLICY['builds']); bad[1]['source_commit'] = '0' * 40
		with self.assertRaises(ValueError): p.verify_pair(bad)

	def test_raw_ceilings_and_ratio(self):
		for kind in p.CHANGED:
			for layout in p.LAYOUTS:
				cap = (2.5 if layout == 'flat' else 5) if kind == 'boolchain' else (2 if layout == 'flat' else 4)
				limit = int(cap * 10000000)
				self.assertEqual(p.changed_rules(kind, layout, limit * 10, limit, 10000000), [])
				self.assertIn('Boolean whole-map ceiling', p.changed_rules(kind, layout, limit * 20, limit + 1, 10000000))
				if kind in p.NEW:
					self.assertEqual(p.changed_rules(kind, layout, 10000, 1000, 10000000), [])
					self.assertEqual(p.changed_rules(kind, layout, 10000, 1001, 10000000), ['Boolean baseline ratio'])


if __name__ == '__main__':
	unittest.main()
