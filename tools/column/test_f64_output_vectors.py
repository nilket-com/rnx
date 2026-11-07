"""Fail-closed controls for the initial ADR0222 diagnostic."""
import unittest
from unittest.mock import patch
import f64_output_vectors as scorer
from test_f64_plan_vectors import fixture, ENTRY, LANE

class Controls(unittest.TestCase):
	def setUp(self):
		self.rows = fixture()
		for row in self.rows:
			if row['kind'] == 'f64chain':
				for cost in row['costs']:
					cost['functions'].update({ENTRY: (10, 1), LANE: (20, 1)})
		self.builds = [dict(role=r, source_commit='a'*40, sha256='b'*64) for r in ('baseline','candidate')]
		p = patch.object(scorer, 'vector', side_effect=lambda c:c['functions'])
		p.start(); self.addCleanup(p.stop)
		for name, replacement in [('f64_output_pins.verify_pair', lambda b:None), ('f64_output_pins.check_vectors', lambda a,b,*args:(scorer.compare(a,b),{})), ('f64_output_edges.check', lambda r:[])]:
			p=patch(name, side_effect=replacement);p.start();self.addCleanup(p.stop)
	def verdict(self):
		return scorer.evaluate(self.rows,self.builds)
	def test_positive(self):
		self.assertEqual(self.verdict()['status'],'PASS')
	def test_f64_growth(self):
		x=next(r for r in self.rows if r['kind']=='f64chain' and r['role']=='candidate')
		x['costs'][0]['Ir_per_row'] += 0.000001
		self.assertIn('F64 whole-map nonpositive', [f['rule'] for f in self.verdict()['failures']])
	def test_protected_growth(self):
		self.rows[1]['costs'][0]['functions']['protected']=(101,1)
		self.assertEqual(self.verdict()['status'],'FAIL')
	def test_missing_case(self):
		self.rows.pop()
		with self.assertRaises(AssertionError): self.verdict()
