import copy
import unittest
import score_0214 as rules


def records():
	return [dict(kind=k, rows=n, layout=l, role=r, fixture_sha256=k+l,
		costs=[{'Ir': (40 if r == 'baseline' else 32)*n} for _ in range(2)])
		for k in rules.KINDS for n in rules.SIZES for l in rules.LAYOUTS for r in ['baseline', 'candidate']]


class ConstantTargets(unittest.TestCase):
	def test_exact_boundaries_pass(self):
		self.assertEqual(rules.constant_targets(records())['status'], 'PASS')

	def test_one_instruction_above_ceiling_fails_specific_rule(self):
		r = records()
		for x in r:
			if x['role'] == 'baseline':
				for c in x['costs']: c['Ir'] = 50*x['rows']
		x = next(x for x in r if x['role'] == 'candidate'); x['costs'][0]['Ir'] += 1
		self.assertEqual({f['rule'] for f in rules.constant_targets(r)['failures']}, {'constant whole-map <=32 Ir/row'})

	def test_one_instruction_above_ratio_fails_specific_rule(self):
		r = records()
		for x in r:
			for c in x['costs']: c['Ir'] = (35 if x['role'] == 'baseline' else 28)*x['rows']
		x = next(x for x in r if x['role'] == 'candidate'); x['costs'][0]['Ir'] += 1
		self.assertEqual({f['rule'] for f in rules.constant_targets(r)['failures']}, {'constant whole-map <=80% baseline'})

	def test_incomplete_duplicate_mismatched_fixture_and_zero_fail_closed(self):
		original = records()
		variants = [original[:-1], original + [original[0]], copy.deepcopy(original), copy.deepcopy(original)]
		next(x for x in variants[2] if x['role'] == 'candidate')['fixture_sha256'] = 'wrong'
		variants[3][0]['costs'][0]['Ir'] = 0
		for r in variants:
			with self.assertRaises(ValueError): rules.constant_targets(r)



class ProtectedTargets(unittest.TestCase):
	def rows(self):
		return [dict(kind=k, rows=n, layout=l, role=r, fixture_sha256=k+l,
			costs=[{'accounting': {'total_Ir': 20*n, 'rnx_Ir': 18*n, 'other_Ir': n, 'dynamic_Ir': n}} for _ in range(2)])
			for k in ['i64add','axpy','multiply3','f64const','f64add','boolnot']
			for n in rules.SIZES for l in ['flat','sliced-chunked-null','alternating','deterministic-random']
			for r in ['baseline','candidate']]

	def score(self, rows, failures=None):
		return rules.protected_targets(rows, {'rows': sorted(rules.SIZES)},
			lambda *_: {'status': 'FAIL' if failures else 'PASS', 'failures': failures or []})

	def test_equal_passes_and_program_one_fails(self):
		r = self.rows(); self.assertEqual(self.score(r)['status'], 'PASS')
		a = next(x for x in r if x['role']=='candidate')['costs'][0]['accounting']
		a['rnx_Ir'] += 1; a['total_Ir'] += 1
		self.assertEqual({f['rule'] for f in self.score(r)['failures']}, {'existing flat program zero growth'})

	def test_system_band_and_ceilings_remain(self):
		for kind in ['constant', 'slope']:
			r = self.rows()
			for x in r:
				if x['role']=='candidate' and (kind=='constant' or x['rows']==100000000):
					delta = 4097 if kind=='constant' else 1000000
					a=x['costs'][0]['accounting']; a['dynamic_Ir']+=delta; a['total_Ir']+=delta
			self.assertEqual({f['rule'] for f in self.score(r)['failures']}, {'system library growth'})
		self.assertEqual(self.score(self.rows(), [{'rule':'strict nullable improvement'}])['status'], 'PASS')
		self.assertEqual(self.score(self.rows(), [{'rule':'numeric ceiling'}])['status'], 'FAIL')



class BuildPins(unittest.TestCase):
	def test_wrong_pair_rejected_before_scoring(self):
		for a, b in [('wrong', rules.CANDIDATE), (rules.BASELINE, 'wrong'), (rules.BASELINE, rules.CANDIDATE[:8])]:
			with self.assertRaisesRegex(ValueError, 'unreviewed build pair'):
				rules.evaluate({}, {}, {'baseline': a, 'candidate': b}, {}, None)

	def test_role_pin_mismatch_rejected(self):
		builds = {'baseline': rules.BASELINE, 'candidate': rules.CANDIDATE,
			'builds': [{'role': 'baseline', 'source_commit': rules.BASELINE}, {'role': 'candidate', 'source_commit': 'wrong'}]}
		with self.assertRaisesRegex(ValueError, 'build role source mismatch'):
			rules.evaluate({}, {}, builds, {}, None)


if __name__ == '__main__':
	unittest.main()
