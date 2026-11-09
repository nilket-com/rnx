import copy
import json
import unittest
from fixed_route_exception import evaluate, POLICY, vector_pin

class ExactException(unittest.TestCase):
	def setUp(self):
		self.policy=json.loads(POLICY.read_text())
		self.records=[]
		for c in self.policy['cases']:
			for role in ('baseline','candidate'):
				a={}
				for part in ('rnx_functions','other_functions'):
					a[part]={'unchanged':100, 'changed':100+(c['n'] if role=='candidate' else 0)}
				a.update(rnx_Ir=sum(a['rnx_functions'].values()),other_Ir=sum(a['other_functions'].values()),dynamic_Ir=100000000+(c['dynamic_delta'] if role=='candidate' else 0))
				a['total_Ir']=a['rnx_Ir']+a['other_Ir']+a['dynamic_Ir']
				self.records.append(dict(kind=c['kind'],policy=c['policy'],n=c['n'],role=role,status='PASS',accounting=a))
		for c in self.policy['cases']:
			for part in ('rnx_functions','other_functions'):
				c[part]=vector_pin(['unchanged','changed'],{'changed':c['n']})
	def check(self):return evaluate(self.records,self.policy['binaries'],self.policy)
	def bump(self,r,n):
		a=r['accounting'];name=next(iter(a['rnx_functions']))
		a['rnx_functions'][name]+=n;a['rnx_Ir']+=n;a['total_Ir']+=n
	def test_exact(self):self.assertEqual(self.check()['status'],'PASS')
	def test_one_extra_instruction_each_case(self):
		original=copy.deepcopy(self.records)
		for i,r in enumerate(original):
			if r['role']=='candidate':
				self.records=copy.deepcopy(original);self.bump(self.records[i],1)
				with self.assertRaises(ValueError):self.check()
	def test_extra_per_n_step(self):
		for r in self.records:
			if r['role']=='candidate':self.bump(r,r['n'])
		with self.assertRaises(ValueError):self.check()
	def test_unlisted_function(self):
		a=self.records[1]['accounting'];a['rnx_functions']['unlisted']=1;a['rnx_Ir']+=1;a['total_Ir']+=1
		with self.assertRaises(ValueError):self.check()
	def test_zero_delta_unlisted_function(self):
		for r in self.records[:2]:r['accounting']['rnx_functions']['unlisted']=0
		with self.assertRaises(ValueError):self.check()
	def test_redistributed_delta(self):
		a=self.records[1]['accounting']['rnx_functions']
		a['changed']+=1;a['unchanged']-=1
		with self.assertRaises(ValueError):self.check()
	def test_canonical_order(self):
		self.assertEqual(vector_pin(['a','b'],{'a':1,'b':2}),vector_pin(['b','a'],{'b':2,'a':1}))
	def test_unlisted_workload(self):
		self.records[0]['kind']='unlisted'
		with self.assertRaises(ValueError):self.check()
	def test_wrong_pair(self):
		bad=dict(self.policy['binaries']);bad['candidate']='0'*64
		with self.assertRaises(ValueError):evaluate(self.records,bad,self.policy)
	def test_missing_case(self):
		self.records.pop()
		with self.assertRaises(ValueError):self.check()

if __name__=='__main__':unittest.main()
