"""Negative controls for ADR0221 exact vectors and executed loop shapes."""
import copy
import itertools
import unittest
from unittest.mock import patch
import f64_plan_vectors as scorer

ENTRY = 'rune::experimental_core::column::run::f64_plan::run::<closure>'
LANE = 'rune::experimental_core::column::run::f64_plan::lanes::<0, A, B>'
COLD = 'rune::experimental_core::column::run::f64_plan::lanes::<1, C, D>'


def fixture():
	rows = []
	for kind, layout, n, role in itertools.product(scorer.KINDS, scorer.LAYOUTS, (4096, 65536), ('baseline', 'candidate')):
		functions = {'protected': (100, 1)}
		if kind == 'f64chain' and role == 'candidate':
			functions.update({ENTRY: (10, 1), LANE: (20, 1)})
		cost = dict(functions=functions, Ir_per_row=1.0,
			accounting=dict(other_Ir=2, rnx_Ir=100, dynamic_Ir=5))
		rows.append(dict(kind=kind, layout=layout, rows=n, role=role,
			fixture_sha256='same', costs=[copy.deepcopy(cost), copy.deepcopy(cost)]))
	return rows


class Controls(unittest.TestCase):
	def setUp(self):
		self.mock = patch.object(scorer, 'vector', side_effect=lambda c: c['functions'])
		self.mock.start(); self.addCleanup(self.mock.stop)
		for name, replacement in [('verify_pair', lambda builds: None), ('check_vectors', lambda a,b,*args: (scorer.compare(a,b), {}))]:
			mock = patch('f64_plan_pins.' + name, side_effect=replacement)
			mock.start(); self.addCleanup(mock.stop)
		self.rows = fixture()
		self.builds = [dict(role=r) for r in ('baseline', 'candidate')]

	def target(self, kind='booland'):
		return next(r for r in self.rows if r['role'] == 'candidate' and r['kind'] == kind)

	def verdict(self):
		return scorer.evaluate(self.rows, self.builds)

	def test_positive(self):
		self.assertEqual(self.verdict()['status'], 'PASS')

	def test_protected_ir(self):
		self.target()['costs'][0]['functions']['protected'] = (101, 1)
		self.assertEqual(self.verdict()['failures'][0]['rule'], 'protected exact vector')

	def test_protected_call(self):
		self.target()['costs'][0]['functions']['protected'] = (100, 2)
		self.assertEqual(self.verdict()['failures'][0]['rule'], 'protected exact vector')

	def test_protected_driver(self):
		self.target()['costs'][0]['functions'][ENTRY] = (1, 1)
		self.assertEqual(self.verdict()['failures'][0]['rule'], 'protected exact vector')

	def test_missing_witness(self):
		del self.target('f64chain')['costs'][0]['functions'][ENTRY]
		self.assertEqual(self.verdict()['failures'][0]['rule'], 'live F64 driver witness')

	def test_unknown_ir(self):
		self.target()['costs'][0]['accounting']['other_Ir'] += 1
		self.assertEqual(self.verdict()['failures'][0]['rule'], 'protected exact vector')

	def test_fixture_mismatch(self):
		self.target()['fixture_sha256'] = 'different'
		with self.assertRaises(AssertionError): self.verdict()

	def test_missing_case(self):
		self.rows.pop()
		with self.assertRaises(AssertionError): self.verdict()

	def test_executed_shape(self):
		shape = dict(functions=[dict(name=LANE, packed_arithmetic=2, assembly='10: movupd (%rdx,%rax,1),%xmm0\n14: addpd %xmm1,%xmm0\n18: addpd %xmm1,%xmm0\n1c: addpd %xmm1,%xmm0\n20: addpd %xmm1,%xmm0\n24: add $0x40,%rax\n28: cmp %rax,%rcx\n2c: jne 10 <loop>'),
			dict(name=COLD, packed_arithmetic=0, assembly='subsd')])
		r = scorer.executed_shapes(self.rows, shape)
		self.assertEqual(r['status'], 'PASS')
		self.assertEqual(r['unexecuted'], [COLD])
		shape['functions'][0]['packed_arithmetic'] = 0
		shape['functions'][1]['packed_arithmetic'] = 2
		r = scorer.executed_shapes(self.rows, shape)
		self.assertEqual(r['status'], 'FAIL')
		self.assertEqual(r['failures'][0]['rule'], 'executed scalar lane specialization')

	def test_missing_shape(self):
		r = scorer.executed_shapes(self.rows, dict(functions=[]))
		self.assertEqual(r['status'], 'FAIL')
		self.assertEqual(r['failures'][0]['rule'], 'missing or ambiguous lane disassembly')


if __name__ == '__main__':
	unittest.main()
