import itertools
import unittest
from unittest.mock import patch
from predicate_pins import POLICY
from predicate_vectors import PROTECTED, SPECS, LAYOUTS, evaluate


def records():
	out = []
	for kind, layout, rows, role in itertools.product((*PROTECTED, *SPECS), LAYOUTS, (4096, 65536), ('baseline', 'candidate')):
		functions = {'unchanged': (100, 1)}
		if kind in PROTECTED:
			functions.update({name: (100 + (pin['constant_Ir'] if role == 'candidate' else 0), 1) for name, pin in POLICY['rules'][kind].items()})
		if kind in SPECS and role == 'candidate':
			functions = {'rune::experimental_core::column::run::predicate::run::test': (50, 1)}
		cost = dict(functions=functions, Ir_per_row=1, accounting=dict(other_Ir=0, rnx_Ir=100, dynamic_Ir=0))
		out.append(dict(kind=kind, layout=layout, rows=rows, role=role, fixture_sha256='fixture', costs=[cost, cost.copy()]))
	return out


class VectorControls(unittest.TestCase):
	@patch('predicate_vectors.vector', side_effect=lambda cost: cost['functions'])
	def test_exact_then_one_instruction_growth(self, _):
		r = records()
		self.assertEqual(evaluate(r, POLICY['builds'])['status'], 'PASS')
		r[1]['costs'][0]['functions'] = {'unchanged': (101, 1)}
		v = evaluate(r, POLICY['builds'])
		self.assertEqual(v['status'], 'FAIL')
		self.assertEqual(v['failures'][0]['rule'], 'protected exact vector')

	@patch('predicate_vectors.vector', side_effect=lambda cost: cost['functions'])
	def test_missing_driver(self, _):
		r = records()
		for row in r:
			if row['kind'] in SPECS:
				for cost in row['costs']: cost['functions'] = {'generic': (1, 1)}
		v = evaluate(r, POLICY['builds'])
		self.assertEqual(v['status'], 'FAIL')
		self.assertEqual(v['failures'][0]['rule'], 'live predicate driver witness')

	def test_missing_case(self):
		with self.assertRaises(ValueError): evaluate(records()[:-1], POLICY['builds'])
