import unittest
import preparation_rule as prep
import score_0213 as score


class Scoring(unittest.TestCase):
	def data(self):
		def matrix(sizes):
			rows = []
			for n in sizes:
				for k, vector in prep.VECTORS.items():
					for layout in prep.LAYOUTS:
						for role in ('baseline', 'candidate'):
							candidate = role == 'candidate'
							rate = (24 if candidate else 45) if k == 'multiply3' else 3
							cost = rate * n + (sum(vector) if candidate else 0)
							funcs = {f: 2000 + (v if candidate else 0) for f, v in zip(prep.FUNCTIONS, vector, strict=True)}
							calls = [dict(Ir=cost, accounting=dict(total_Ir=cost, rnx_Ir=cost, dynamic_Ir=0, other_Ir=0, rnx_functions=dict(funcs))) for _ in range(2)]
							rows.append(dict(kind=k, layout=layout, rows=n, role=role, costs=calls))
			return dict(records=rows)
		adv = dict(records=[dict(rows=n, layout=l, role=r, costs=[dict(Ir=n * (2 if r == 'baseline' else 1)) for _ in range(2)]) for n in (4096, 65536) for l in ('benign', 'adversarial') for r in ('baseline', 'candidate')])
		return dict(preflight=matrix([4096]), large=matrix([10000000, 100000000]), adversarial=adv, builds=dict(baseline=prep.BASE, candidate=prep.CANDIDATE), policy={}, raw_evaluator=lambda *_: dict(failures=[]))

	def test_good_and_target(self):
		d = self.data()
		self.assertEqual(score.evaluate(**d)['status'], 'PASS')
		r = next(r for r in d['large']['records'] if r['kind'] == 'multiply3' and r['layout'] == 'alternating' and r['role'] == 'candidate')
		c = r['costs'][0]; c['Ir'] = 25 * r['rows'] + 1
		c['accounting']['total_Ir'] = c['accounting']['rnx_Ir'] = c['Ir']
		self.assertEqual(score.evaluate(**d)['status'], 'FAIL')

	def test_unknown_and_missing(self):
		d = self.data(); d['large']['records'].pop()
		with self.assertRaisesRegex(ValueError, 'coverage'):
			score.evaluate(**d)
		d = self.data(); c = d['large']['records'][1]['costs'][0]
		c['accounting']['other_Ir'] += 1; c['accounting']['rnx_Ir'] -= 1
		with self.assertRaisesRegex(ValueError, 'unknown'):
			score.evaluate(**d)

	def test_keep_ceiling_and_changed_improvement(self):
		for kind, rule in [('i64add', 'nullable ceiling'), ('multiply3', 'strict nullable improvement')]:
			d = self.data(); d['raw_evaluator'] = lambda *_: dict(failures=[dict(kind=kind, rule=rule)])
			self.assertEqual(score.evaluate(**d)['status'], 'FAIL')
		d = self.data(); d['raw_evaluator'] = lambda *_: dict(failures=[dict(kind='i64add', rule='strict nullable improvement')])
		self.assertEqual(score.evaluate(**d)['status'], 'PASS')

	def test_adversarial_regression(self):
		d = self.data()
		r = next(r for r in d['adversarial']['records'] if r['layout'] == 'adversarial' and r['role'] == 'candidate')
		r['costs'][0]['Ir'] = 2 * r['rows'] + 1
		self.assertEqual(score.evaluate(**d)['status'], 'FAIL')
