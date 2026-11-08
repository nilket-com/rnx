import copy
import itertools
import unittest
from masked_reuse_acceptance import KINDS, LAYOUTS, check


class Acceptance(unittest.TestCase):
	def fixtures(self):
		faults = dict(policy_pass=False, calls=[dict(kind=k, layout=l, call=c, baseline_minor=1000,
			candidate_minor=500 if l == 'sliced-chunked-null' else 0)
			for k,l,c in itertools.product(KINDS,LAYOUTS,range(3))])
		phases = {}
		for kind in KINDS:
			events = []
			for call in range(3):
				for label in ('clock-start','validity-start','validity-end','run-start','output-first-block','run-end','transfer-start','transfer-end','clock-end'):
					events.append(dict(label=label, call=call, minor=0 if label=='clock-start' else 500,
						output_pointer=1000, output_capacity=100000000, memory_kib=dict(AnonHugePages=8000)))
			phases[kind] = events
		timing = dict(status='PASS',comparisons=[dict(kind=k,layout=l,mode=m,ratio=0.7)
			for k,l,m in itertools.product(KINDS,LAYOUTS,('first','warm'))])
		return faults,phases,timing

	def test_pass_and_negative_controls(self):
		base = self.fixtures()
		self.assertEqual(check(*base)['status'],'PASS')
		for mutation in ('fault','pointer','capacity','missing','timing','other-layout'):
			faults,phases,timing = copy.deepcopy(base)
			if mutation == 'fault': next(x for x in phases['f64const'] if x['call']==1 and x['label']=='run-end')['minor'] += 1
			elif mutation == 'pointer': next(x for x in phases['f64const'] if x['call']==1 and x['label']=='output-first-block')['output_pointer'] += 1
			elif mutation == 'capacity': next(x for x in phases['f64const'] if x['call']==1 and x['label']=='output-first-block')['output_capacity'] += 1
			elif mutation == 'missing': phases['f64const'].pop()
			elif mutation == 'timing': next(x for x in timing['comparisons'] if x['mode']=='warm')['ratio'] = 1
			else: next(x for x in faults['calls'] if x['call']==1 and x['layout']=='alternating')['candidate_minor'] = 11
			with self.subTest(mutation=mutation), self.assertRaises((AssertionError,KeyError)):
				check(faults,phases,timing)


if __name__ == '__main__':
	unittest.main()
