import itertools
import unittest
from unittest.mock import patch
from predicate_limits import CEILINGS, limits


class TargetControls(unittest.TestCase):
	def matrix(self):
		out = {}
		for kind, rows, role in itertools.product((*CEILINGS, 'f64chain'), (10000000,100000000), ('baseline','candidate')):
			ir = int(CEILINGS.get(kind,1)*rows)*(4 if role=='baseline' else 1)
			cost = dict(Ir=ir, Ir_per_row=ir/rows, accounting=dict(dynamic_Ir=0, rnx_Ir=100, other_Ir=0))
			out[kind,'flat',rows,role] = dict(costs=[dict(cost),dict(cost)])
		return out

	@patch('predicate_limits.prior_limits', side_effect=lambda *args: [])
	def test_each_ceiling_and_ratio(self, _):
		self.assertFalse(limits(self.matrix(),(),('flat',),(10000000,100000000)))
		for kind in CEILINGS:
			r = self.matrix(); c = r[kind,'flat',100000000,'candidate']['costs'][1]
			c['Ir'] += 1; c['Ir_per_row'] = c['Ir']/100000000
			self.assertEqual({f['rule'] for f in limits(r,(),('flat',),(10000000,100000000))}, {'predicate ceiling','predicate baseline ratio'})
			r = self.matrix(); c = r[kind,'flat',100000000,'baseline']['costs'][0]
			c['Ir'] -= 1; c['Ir_per_row'] = c['Ir']/100000000
			self.assertEqual({f['rule'] for f in limits(r,(),('flat',),(10000000,100000000))}, {'predicate baseline ratio'})

	@patch('predicate_limits.prior_limits', side_effect=lambda *args: [])
	def test_f64_program_and_system(self, _):
		for field, amount, expected in [('rnx_Ir',1,'protected F64 program nonpositive / unknown exact'), ('other_Ir',1,'protected F64 program nonpositive / unknown exact'), ('dynamic_Ir',200000,'unchanged system band')]:
			r = self.matrix()
			r['f64chain','flat',100000000,'candidate']['costs'][0]['accounting'] = dict(rnx_Ir=100,other_Ir=0,dynamic_Ir=0)
			r['f64chain','flat',100000000,'candidate']['costs'][0]['accounting'][field] += amount
			self.assertIn(expected,{f['rule'] for f in limits(r,(),('flat',),(10000000,100000000))})

	@patch('predicate_limits.prior_limits', side_effect=lambda *args: [dict(rule='F64 whole-map nonpositive',kind='f64chain'),dict(rule='whole-map ceiling',kind='f64chain')])
	def test_scope_keeps_ceiling(self, _):
		self.assertEqual([f['rule'] for f in limits(self.matrix(),(),('flat',),(10000000,100000000))],['whole-map ceiling'])
