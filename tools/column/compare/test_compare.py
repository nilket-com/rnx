import json
from pathlib import Path
import tempfile
import unittest
from durable import atomic,digest,reuse
from fixtures import SPECS,LAYOUTS,manifest,arrow_inputs,validate,NATIVE
from engines import prepare


class Controls(unittest.TestCase):
	def test_fixture_engines(self):
		for kind in SPECS:
			for layout in LAYOUTS:
				m=manifest(kind,1031,layout);table=arrow_inputs(m)
				for engine in ('polars','arrow','duckdb'):
					for native in ((False,True) if engine!='arrow' and kind in NATIVE else (False,)):
						with self.subTest(kind=kind,layout=layout,engine=engine,native=native):
							call,export,_=prepare(engine,kind,table,native,'single')
							validate(m,export(call()),native)

	def test_reusable_semantics(self):
		import itertools
		import math
		import pyarrow as pa
		from semantics import oracle,equal
		rows=[]
		for a,b,c in itertools.product((None,False,True),repeat=3):
			rows.append(dict(x=2.0,y=1.0,a=a,b=b,c=c,i=2))
		for x,y in itertools.product((None,float('nan'),float('inf'),-float('inf'),-0.0,0.0,0.1,2.0),repeat=2):
			rows.append(dict(x=x,y=y,a=True,b=False,c=True,i=-2**63))
		table=pa.table({n:pa.array([r[n] for r in rows],type=pa.float64() if n in ('x','y') else pa.int64() if n=='i' else pa.bool_()) for n in rows[0]})
		for kind in SPECS:
			for engine in ('polars','arrow','duckdb'):
				call,export,_=prepare(engine,kind,table,False,'single')
				actual=export(call()).to_pylist()
				self.assertEqual(len(actual),len(rows))
				self.assertTrue(all(equal(v,oracle(kind,r)) for v,r in zip(actual,rows)),(kind,engine))

	def test_corrupt_output_rejected(self):
		import pyarrow as pa
		m=manifest('f64const',1031,'flat')
		with self.assertRaises(AssertionError): validate(m,pa.array([0.0]*1031))

	def test_trimmed_thread_axis(self):
		from collections import Counter
		from matrix import cells
		jobs=[c for n in (10000000,100000000) for t in ('single','default') for c in cells(n,t,list(SPECS),list(LAYOUTS))]
		self.assertEqual(Counter(c['panel'] for c in jobs),{'equivalent':528,'native':224})
		self.assertTrue(all(c['threads']=='single' for c in jobs if c['engine'] in ('rnx','arrow')))

	def test_secondary_policy_and_validation_reuse(self):
		from collections import Counter
		from matrix import cells,selected,validation_identity_matches
		jobs=[c for n in (10000000,100000000) for t in ('single','default') for r in range(5) for c in cells(n,t,list(SPECS),list(LAYOUTS)) if selected(c,r)]
		self.assertEqual(Counter(c['panel'] for c in jobs),{'equivalent':2640,'native':336})
		old={'sources':{'matrix.py':'a','worker.py':'same'},'rnx_sha256':'binary'}
		new={'sources':{'matrix.py':'b','worker.py':'same'},'rnx_sha256':'binary','candidate_commit':'signed','policy':{}}
		self.assertTrue(validation_identity_matches(old,new))
		new['sources']['worker.py']='changed'
		self.assertFalse(validation_identity_matches(old,new))
		new['sources']['worker.py']='same';new['rnx_sha256']='other'
		self.assertFalse(validation_identity_matches(old,new))

	def test_resume(self):
		with tempfile.TemporaryDirectory() as d:
			d=Path(d);identity={'a':1}
			self.assertIsNone(reuse(d,identity))
			atomic(d/'raw.json',{'answer':42})
			atomic(d/'RESULT.json',dict(status='PASS',identity=identity,artifacts={'raw.json':digest(d/'raw.json')}))
			self.assertEqual(reuse(d,identity)['status'],'PASS')
			with self.assertRaises(RuntimeError):reuse(d,{'a':2})
			(d/'raw.json').write_text('changed')
			with self.assertRaises(RuntimeError):reuse(d,identity)
			atomic(d/'RESULT.json',dict(status='FAIL',identity=identity,artifacts={}))
			with self.assertRaises(RuntimeError):reuse(d,identity)


if __name__=='__main__':unittest.main()
