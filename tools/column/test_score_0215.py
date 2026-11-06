import copy
import unittest
from score_0215 import evaluate, POLICY, KINDS, LAYOUTS


def fixture(wrappers=False):
	kinds = {"add-right", "add-left", "sub-right", "sub-left", "float-generic", "float-sub", "float-mul"} if wrappers else KINDS
	layouts = {"benign", "adversarial"} if wrappers else LAYOUTS
	builds = {'builds': [dict(role=r, source_commit=p) for r, p in dict(POLICY['baselines'], candidate=POLICY['candidate']).items()]}
	rows = []
	for kind in kinds:
		for layout in layouts:
			for n in (4096, 65536):
				for role in ('baseline', 'original', 'candidate'):
					entry = POLICY['entries'].get('/'.join((role, kind, layout)))
					vector = [-v for v in entry['vector']] if entry else [0] * len(POLICY['functions'])
					account = dict(rnx_Ir=10000 + sum(vector), rnx_functions=dict(zip(POLICY['functions'], vector)), other_functions={})
					rows.append(dict(kind=kind, layout=layout, rows=n, role=role, fixture_sha256='same', costs=[{'accounting': copy.deepcopy(account)} for _ in range(2)]))
	return rows, builds


class ConstantTests(unittest.TestCase):
	def test_controls(self):
		rows, builds = fixture()
		# Baseline-pair caps also apply to original where the same constants
		# exist. Original may have additional work elsewhere in real profiles.
		self.assertEqual(evaluate(rows, builds)['status'], 'PASS')
		for mutation in ('cost', 'vector', 'unknown', 'slope'):
			x = copy.deepcopy(rows)
			r = next(r for r in x if r['kind'] == 'boolnot' and r['layout'] == 'flat' and r['rows'] == 65536 and r['role'] == 'candidate')
			a = r['costs'][1]['accounting']
			if mutation in ('cost', 'slope'):
				a['rnx_Ir'] += 1 if mutation == 'cost' else 3 * 64
			elif mutation == 'vector':
				a['rnx_functions'][POLICY['functions'][0]] += 1
			else:
				a['other_functions']['unknown'] = 1
			self.assertEqual(evaluate(x, builds)['status'], 'FAIL', mutation)

	def test_pair_and_completeness(self):
		rows, builds = fixture()
		with self.assertRaises(ValueError): evaluate(rows[:-1], builds)
		builds['builds'][0]['source_commit'] = '0' * 40
		with self.assertRaises(ValueError): evaluate(rows, builds)

	def test_wrapper_cap_plus_one(self):
		rows, builds = fixture(wrappers=True)
		self.assertEqual(evaluate(rows, builds, wrappers=True)['status'], 'PASS')
		r = next(x for x in rows if x['role'] == 'candidate' and x['kind'] == 'float-sub')
		r['costs'][0]['accounting']['rnx_Ir'] += 1
		self.assertEqual(evaluate(rows, builds, wrappers=True)['status'], 'FAIL')
