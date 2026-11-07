import copy
import unittest
import reuse_vectors as p


class Pins(unittest.TestCase):
	def test_each_exact_pin_and_growth(self):
		for key, rules in p.POLICY['rules'].items():
			kind, layout, call, size = key.split('/')
			for n in ((4096, 65536) if size == 'small' else (10000000, 100000000)):
				blocks = (n + 1023) // 1024
				a = {name: (100000000000, pin['calls'][0]) for name, pin in rules.items()}
				b = {name: (a[name][0] + pin['fixed'] + pin['per_block'] * blocks, pin['calls'][1]) for name, pin in rules.items()}
				self.assertFalse(p.check(a, b, kind, layout, n, int(call)))
				for name in rules:
					for extra in (1, blocks):
						bad = dict(b); bad[name] = (b[name][0] + extra, b[name][1])
						self.assertIn(name, p.check(a, bad, kind, layout, n, int(call)))
					bad = dict(b); bad[name] = (b[name][0], b[name][1] + 1)
					self.assertIn(name, p.check(a, bad, kind, layout, n, int(call)))
				bad = dict(b, unreviewed=(1, 1))
				self.assertIn('unreviewed', p.check(a, bad, kind, layout, n, int(call)))

	def test_pair(self):
		p.verify_pair(p.POLICY['builds'])
		for field, width in [('source_commit', 40), ('sha256', 64)]:
			bad = copy.deepcopy(p.POLICY['builds']); bad[1][field] = '0' * width
			with self.assertRaises(ValueError): p.verify_pair(bad)


if __name__ == '__main__':
	unittest.main()
