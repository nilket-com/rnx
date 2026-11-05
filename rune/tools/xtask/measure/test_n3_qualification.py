"""The N3 record is exact, pinned and cannot hide another cell/call-table change."""
import copy
import json
from pathlib import Path
import unittest

import n3_qualification as n3

HOT = 'rune::experimental_core::region::cache::Ready::enter'
OTHER = 'rune::experimental_core::region::cache::checkout'


def profiles(delta):
	tip = dict(total_Ir=100_000_000,functions={HOT:dict(self_Ir=100_000,calls=1000),
		OTHER:dict(self_Ir=100_000,calls=1000),'outside':dict(self_Ir=99_800_000,calls=1)})
	candidate=copy.deepcopy(tip)
	candidate['total_Ir']+=delta
	candidate['functions'][HOT]['self_Ir']+=delta
	return tip,candidate,dict(tip=n3.facts(tip),candidate=n3.facts(candidate),reference=n3.facts(tip))


class N3Qualification(unittest.TestCase):
	def test_policy_and_attached_proof_are_pinned(self):
		root=Path(__file__).parent/'qualifications'
		self.assertEqual(json.loads((root/'n3.json').read_text()),n3.POLICY)
		self.assertEqual(n3.pin(root/'n3-proof.json')['sha256'],n3.POLICY['proof_sha256'])
		self.assertEqual({cell:delta for entry in n3.POLICY['entries']for cell,delta in entry['cells'].items()},n3.EXPECTED)

	def test_exact_three_cells_retain_raw_profiles_and_accounting(self):
		for cell,delta in n3.EXPECTED.items():
			with self.subTest(cell=cell):
				tip,candidate,proof=profiles(delta);saved=copy.deepcopy(candidate)
				adjusted,accounting=n3.qualify(tip,candidate,cell,proof)
				self.assertEqual(candidate,saved)
				self.assertEqual(adjusted,tip)
				self.assertEqual(accounting['qualified_net_core_Ir'],delta)
				self.assertEqual(accounting['raw_candidate'],n3.facts(candidate))

	def test_one_instruction_either_way_or_changed_calls_is_rejected(self):
		for cell,delta in n3.EXPECTED.items():
			for mutation in (-1,1,'calls'):
				with self.subTest(cell=cell,mutation=mutation):
					tip,candidate,proof=profiles(delta)
					if mutation=='calls':candidate['functions'][HOT]['calls']+=1
					else:
						candidate['functions'][HOT]['self_Ir']+=mutation
						candidate['total_Ir']+=mutation
					with self.assertRaises(ValueError):n3.qualify(tip,candidate,cell,proof)

	def test_same_net_delta_cannot_hide_other_growth(self):
		tip,candidate,proof=profiles(2626)
		candidate['functions'][HOT]['self_Ir']-=1
		candidate['functions'][OTHER]['self_Ir']+=1
		with self.assertRaisesRegex(ValueError,'call table changed'):
			n3.qualify(tip,candidate,'on/while',proof)

	def test_native_off_and_unqualified_cells_get_no_positive_allowance(self):
		for cell in ('off/while','off/numeric','off/calls','off/fib','off/shared_rows','on/calls','on/fib'):
			with self.subTest(cell=cell):
				tip,candidate,proof=profiles(1)
				with self.assertRaises(ValueError):n3.qualify(tip,candidate,cell,proof)
				tip,candidate,proof=profiles(0)
				adjusted,accounting=n3.qualify(tip,candidate,cell,proof)
				self.assertEqual(adjusted,candidate)
				self.assertEqual(accounting['qualified_net_core_Ir'],0)

	def test_even_reviewed_cell_requires_exact_delta_and_raw_whole_bound(self):
		tip,candidate,proof=profiles(2627)
		with self.assertRaisesRegex(ValueError,'qualified cell delta'):
			n3.qualify(tip,candidate,'on/while',proof)
		tip,candidate,proof=profiles(2626)
		candidate['total_Ir']+=200_000;proof['candidate']=n3.facts(candidate)
		with self.assertRaisesRegex(ValueError,'raw whole-process bound'):
			n3.qualify(tip,candidate,'on/while',proof)


if __name__=='__main__':
	unittest.main()
