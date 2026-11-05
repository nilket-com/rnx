import copy
from fractions import Fraction
import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parent))
import n5_qualification as n5

PROOF=dict(measured_commit=n5.CANDIDATE,tip_commit=n5.TIP,profile='cgu1',production_identical=True,tooling_only_delta=[])
VM='<rune::experimental_core::execute::Executor>::invoke_args::<true, false>::{closure#0}'
NEW_VM=VM.replace('<true, false>','<true, false, false>')

def pair(n, host_cost=8):
	functions={n5.INVOKE:dict(self_Ir=166*n,calls=n),n5.CHECKOUT:dict(self_Ir=99*n+368,calls=n),
		VM:dict(self_Ir=402*n,calls=n),'outside':dict(self_Ir=100000*n,calls=n)}
	tip=dict(functions=functions,total_Ir=sum(r['self_Ir'] for r in functions.values()))
	candidate=copy.deepcopy(tip);candidate['functions'][n5.INVOKE]['self_Ir']+=host_cost*n
	candidate['functions'][NEW_VM]=candidate['functions'].pop(VM);candidate['total_Ir']+=host_cost*n
	return dict(tip=tip,candidate=candidate)

class N5Tests(unittest.TestCase):
	def test_checked_in_policy_and_proof(self):
		self.assertEqual(n5.checked_policy(),n5.POLICY)

	def test_all_nine_exact_slopes_preserve_raw_profiles(self):
		for cell in n5.DIAGNOSTICS:
			profiles={n:pair(n) for n in (1000,2000)};saved=copy.deepcopy(profiles)
			result=n5.qualify_short(profiles,cell,PROOF)
			self.assertEqual(result['raw_steady_Ir'],8000);self.assertEqual(result['cold_setup_delta'],0)
			self.assertEqual(saved,profiles)

	def test_exact_allowance_rejects_one_more_or_less(self):
		for d in (-1,1):
			profiles={n:pair(n) for n in (1000,2000)}
			profiles[2000]['candidate']['functions'][n5.INVOKE]['self_Ir']+=d
			with self.assertRaisesRegex(ValueError,'exact host-entry'):
				n5.qualify_short(profiles,'range',PROOF)

	def test_vm_checkout_and_other_steady_growth_rejected(self):
		for name in (NEW_VM,n5.CHECKOUT,'experimental_core::unexpected'):
			profiles={n:pair(n) for n in (1000,2000)}
			r=profiles[2000]['candidate']['functions'].setdefault(name,dict(self_Ir=0,calls=1));r['self_Ir']+=1
			with self.assertRaises(ValueError):n5.qualify_short(profiles,'range',PROOF)

	def test_wrong_cell_profile_or_source_cannot_inherit(self):
		for key in ('measured_commit','tip_commit','profile','production_identical'):
			proof=dict(PROOF);proof[key]='other'
			with self.assertRaisesRegex(ValueError,'binding'):n5.qualify_short({n:pair(n) for n in (1000,2000)},'range',proof)
		with self.assertRaises(ValueError):n5.qualify_short({n:pair(n) for n in (1000,2000)},'other',PROOF)
		for cell in ('on/fib','on/unknown'):
			with self.assertRaises(ValueError):n5.qualify(**pair(1000),cell=cell,proof=PROOF)

	def test_standard_adjusts_only_invoke_and_keeps_whole_raw(self):
		for cell in n5.CELLS:
			expected=n5.POLICY['entries'][0]['cells'][cell]
			p=pair(1000,expected//1000);saved=copy.deepcopy(p['candidate'])
			adjusted,accounting=n5.qualify(**p,cell=cell,proof=PROOF)
			self.assertEqual(adjusted['total_Ir'],saved['total_Ir'])
			self.assertEqual(adjusted['functions'][n5.INVOKE],p['tip']['functions'][n5.INVOKE])
			self.assertEqual(p['candidate'],saved);self.assertEqual(accounting['qualified_entry_Ir'],expected)
		p=pair(1000)
		with self.assertRaises(ValueError):n5.qualify(**p,cell='off/while',proof=PROOF)
		adjusted,_=n5.qualify(p['tip'],p['tip'],'off/while',PROOF);self.assertEqual(adjusted,p['tip'])

	def test_changed_call_counts_or_vm_cost_rejected(self):
		for symbol,field in ((n5.INVOKE,'calls'),(NEW_VM,'self_Ir')):
			p=pair(1000);p['candidate']['functions'][symbol][field]+=1
			with self.assertRaises(ValueError):n5.qualify(**p,cell='on/while',proof=PROOF)

	def test_native_return_validator_uses_recorded_completion_bound(self):
		import callgrind_rules as rules
		counts=dict(entries=1000,completions=999,replays=1,slot_limit=64)
		tip=dict(total_Ir=2000000,functions={'experimental_core::old':dict(self_Ir=2000000,calls=1000)})
		candidate=dict(total_Ir=1090909,functions={'experimental_core::old':dict(self_Ir=1000000,calls=1000),rules.NATIVE_RETURN_SHAPE:dict(self_Ir=90909,calls=6993)})
		kwargs=dict(invocations=1000,iterations=None,controls={k:dict(core=0,whole=0)for k in ('is','branch')},native_returns=counts,cell='on/fib')
		result=rules.assess(tip,candidate,tip,**kwargs)
		self.assertLess(Fraction(result['core_change']),0)
		self.assertEqual(candidate['functions'][rules.NATIVE_RETURN_SHAPE]['self_Ir'],90909)
		old=copy.deepcopy(candidate);old['functions'][rules.NATIVE_RETURN_SHAPE]['calls']=1
		with self.assertRaisesRegex(ValueError,'native return'):
			rules.assess(tip,old,tip,**dict(kwargs,cell='on/while'))
		bad=copy.deepcopy(candidate);bad['functions'][rules.NATIVE_RETURN_SHAPE]['self_Ir']=3000000;bad['total_Ir']=4000000
		with self.assertRaises(ValueError):rules.assess(tip,bad,tip,**kwargs)

	def test_native_return_validator_rejects_foreign_cell_symbol_and_bound(self):
		import callgrind_rules as rules
		row=dict(function=rules.NATIVE_RETURN_SHAPE,calls=6993,candidate_calls=6993,tip_calls=None)
		counts=dict(entries=1000,completions=999,replays=1,slot_limit=64)
		self.assertTrue(rules._native_return_slot(row,'on/fib',1000,counts))
		self.assertFalse(rules._native_return_slot(row,'on/fib',1000,counts,iterations=1))
		for cell in ('on/while','on/calls','off/fib','on/shared_rows'):
			self.assertFalse(rules._native_return_slot(row,cell,1000,counts))
		for changed in (dict(row,function='experimental_core::other'),dict(row,calls=999*64+1,candidate_calls=999*64+1)):
			self.assertFalse(rules._native_return_slot(changed,'on/fib',1000,counts))
		for changed in (dict(counts,slot_limit=65),dict(counts,completions=0),dict(counts,entries=999),dict(counts,replays=0)):
			self.assertFalse(rules._native_return_slot(row,'on/fib',1000,changed))

if __name__=='__main__':unittest.main()
