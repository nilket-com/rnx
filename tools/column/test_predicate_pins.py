import copy
import unittest
from predicate_pins import POLICY, verify_pair, check_vectors


class PinControls(unittest.TestCase):
	def test_pair(self):
		verify_pair(POLICY['builds'])
		for field in ('source_commit', 'sha256'):
			bad = copy.deepcopy(POLICY['builds']); bad[1][field] = '0'*len(bad[1][field])
			with self.assertRaises(ValueError): verify_pair(bad)

	def test_every_family_ir_calls_and_slope(self):
		for kind, pins in POLICY['rules'].items():
			a = {n:(100,1) for n in pins}
			b = {n:(100+p['constant_Ir'],1) for n,p in pins.items()}
			self.assertFalse(check_vectors(a,b,kind))
			for name in pins:
				for extra_ir, extra_call in [(1,0),(0,1),(64,0),(97657,0)]:
					bad = dict(b); ir,calls = bad[name]; bad[name] = (ir+extra_ir,calls+extra_call)
					self.assertIn(name,check_vectors(a,bad,kind))
