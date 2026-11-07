import unittest
from f64_plan_limits import limits


class Limits(unittest.TestCase):
	def data(self,kind='f64chain'):
		return {(kind,l,n,r):{'costs':[{'Ir':n*(100 if r=='baseline' else 10),'Ir_per_row':100 if r=='baseline' else 10,'accounting':{'dynamic_Ir':0}} for _ in range(2)]}
			for l in ('flat','sliced-chunked-null','alternating','deterministic-random') for n in (10000000,100000000) for r in ('baseline','candidate')}

	def test_targets_and_ratio(self):
		x=self.data();layouts=tuple(dict.fromkeys(k[1] for k in x));args=(('f64chain',),layouts,(10000000,100000000))
		self.assertFalse(limits(x,*args))
		for l in layouts:
			bad=self.data();c=bad['f64chain',l,10000000,'candidate']['costs'][0]
			c['Ir']=10000000*(22 if l in ('flat','sliced-chunked-null') else 30)+1;c['Ir_per_row']=c['Ir']/10000000
			self.assertTrue(any(f['rule']=='whole-map ceiling' for f in limits(bad,*args)))
		for l in layouts:
			c=x['f64chain',l,10000000,'baseline']['costs'][0];c['Ir']=200000000;c['Ir_per_row']=20
		self.assertTrue(any(f['rule']=='F64 baseline ratio' for f in limits(x,*args)))

	def test_boolean_and_band(self):
		x=self.data('booland');layouts=tuple(dict.fromkeys(k[1] for k in x))
		for (k,l,n,r),v in x.items():
			for c in v['costs']:c['Ir']=n;c['Ir_per_row']=1
		args=(('booland',),layouts,(10000000,100000000));self.assertFalse(limits(x,*args))
		for n in (10000000,100000000):
			x['booland','flat',n,'candidate']['costs'][0]['accounting']['dynamic_Ir']=5000
		self.assertTrue(any(f['rule']=='unchanged system band' for f in limits(x,*args)))
