import copy
import unittest
from i64_compare_pins import POLICY, verify_pair, check_vectors, unknown_signature, evaluate


class Pins(unittest.TestCase):
	def test_pair_requires_full_source_and_binary_identity(self):
		verify_pair(POLICY['builds'])
		for field in ('source_commit', 'sha256'):
			bad = copy.deepcopy(POLICY['builds'])
			bad[1][field] = '0' * len(bad[1][field])
			with self.assertRaises(ValueError):
				verify_pair(bad)

	def test_every_pin_rejects_extra_instruction_call_or_block(self):
		for key, rules in POLICY['rules'].items():
			kind, layout, call = key.split('/')
			for rows in (4096, 65536, 10000000):
				blocks = (rows + 1023) // 1024
				a = {n: (100000, p['calls'][0]) for n, p in rules.items()}
				b = {n: (100000+p['constant_Ir']+p['per_block_Ir']*blocks, p['calls'][1]) for n, p in rules.items()}
				self.assertFalse(check_vectors(a, b, kind, layout, int(call), rows))
				for name in rules:
					for ir, calls in ((1, 0), (blocks, 0), (0, 1)):
						bad = dict(b); old = bad[name]; bad[name] = (old[0]+ir, old[1]+calls)
						self.assertIn(name, check_vectors(a, bad, kind, layout, int(call), rows))
				bad = dict(b); bad['unreviewed'] = (1, 0)
				self.assertIn('unreviewed', check_vectors(a, bad, kind, layout, int(call), rows))

	def test_incomplete_matrix_is_rejected(self):
		with self.assertRaisesRegex(ValueError, "missing, duplicate or unexpected"):
			evaluate([dict(kind="f64gt", layout="flat", rows=10000000, role="baseline")], POLICY["builds"])

	def test_unknown_translation_only(self):
		a = {'???::0x1000': 4, '???::0x1010': 2}
		b = {'???::0x2000': 4, '???::0x2010': 2}
		self.assertEqual(unknown_signature(a), unknown_signature(b))
		for changed in ({'???::0x2000': 5, '???::0x2010': 2},
			{'???::0x2000': 4, '???::0x2011': 2}):
			self.assertNotEqual(unknown_signature(a), unknown_signature(changed))


if __name__ == '__main__':
	unittest.main()
