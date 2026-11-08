import json
import unittest
import tempfile
import hashlib
from pathlib import Path
from copy import deepcopy
from masked_float_vectors import POLICY, check_fixed, seal, changed, rep_exception, check_faults


class Pins(unittest.TestCase):
	def test_every_fixed_pin_rejects_extra_call_or_block_cost(self):
		policy = json.loads(POLICY.read_text())
		for rules in policy['fixed'].values():
			a = {n:(1000,p['calls'][0]) for n,p in rules.items()}
			b = {n:(1000+p['delta'],p['calls'][1]) for n,p in rules.items()}
			self.assertFalse(check_fixed(a,b,rules))
			for name in rules:
				for extra in (1,4,64,9766):
					bad = dict(b); bad[name] = (b[name][0]+extra,b[name][1])
					self.assertTrue(check_fixed(a,bad,rules))
				bad = dict(b); bad[name] = (b[name][0],b[name][1]+1)
				self.assertTrue(check_fixed(a,bad,rules))
			self.assertTrue(check_fixed(a,dict(b,unlisted=(1,1)),rules))

	def test_rep_exception_rejects_extra_instruction_and_profile_change(self):
		with tempfile.TemporaryDirectory() as directory:
			path = Path(directory) / 'profile'
			path.write_bytes(b'exact measured edge and byte counts')
			cost = dict(profile=str(path), accounting=dict(total_Ir=100, dynamic_Ir=80, rnx_Ir=20))
			pin = {side + suffix: value for side in ('baseline','candidate') for suffix,value in
				(('_profile',hashlib.sha256(path.read_bytes()).hexdigest()),('_accounting',seal(cost['accounting'],{})))}
			self.assertTrue(rep_exception(cost,cost,pin))
			for key in cost['accounting']:
				bad = deepcopy(cost); bad['accounting'][key] += 1
				self.assertFalse(rep_exception(cost,bad,pin))
			path.write_bytes(b'changed call or byte count')
			self.assertFalse(rep_exception(cost,cost,pin))

	def test_fault_observations_reject_plus_one(self):
		policy = json.loads(POLICY.read_text())
		self.assertTrue(check_faults(policy['fault_deltas'],policy))
		for i in range(len(policy['fault_deltas'])):
			bad = deepcopy(policy['fault_deltas']); bad[i]['candidate_minor'] += 1
			self.assertFalse(check_faults(bad,policy))

	def test_changed_scope_and_capture_seal(self):
		self.assertTrue(changed('f64add','alternating'))
		self.assertFalse(changed('f64add','flat'))
		self.assertFalse(changed('f64chain','alternating'))
		self.assertNotEqual(seal({'a':(1,1)},{'a':(2,1)}),seal({'a':(1,1)},{'a':(3,1)}))


if __name__ == '__main__':
	unittest.main()
