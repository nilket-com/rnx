import copy
import json
import unittest
from pathlib import Path
from drift_band import POLICY, compare, score, validate_policy
from child_result import identity


class Bands(unittest.TestCase):
	def setUp(self):
		self.policy = json.loads(POLICY.read_text())

	def rows(self, low=100000, high=500000, dynamic=0):
		return [dict(status='PASS', kind=k, policy=m, n=n, stdout_sha256='output', fixture_sha256='fixture',
			accounting=dict(rnx_Ir=value, other_Ir=0, dynamic_Ir=dynamic, total_Ir=value+dynamic))
			for k in self.policy['kernels'] for m in self.policy['modes'] for n, value in ((1, low), (5, high))]

	def passes(self, low, high):
		return all(r['pass_all'] for r in compare(self.rows(), self.rows(low, high), self.policy, 5))

	def test_exact_boundaries_and_decreases(self):
		self.assertTrue(self.passes(100500,502500))
		self.assertFalse(self.passes(100500,502501))
		self.assertTrue(self.passes(100000,502000))
		self.assertFalse(self.passes(100000,502001))
		self.assertTrue(self.passes(90000,450000))

	def test_raw_total_catches_allocation_growth(self):
		a=self.rows();b=self.rows(dynamic=3000)
		r=compare(a,b,self.policy,5)
		self.assertTrue(all(x['pass_all'] for x in r if x['metric']=='W'))
		self.assertFalse(all(x['pass_all'] for x in r if x['metric']=='T'))

	def test_missing_duplicate_bad_output_fixture_denominator(self):
		for mutation in ('missing','duplicate','output','fixture','count'):
			b=self.rows()
			if mutation=='missing': b.pop()
			elif mutation=='duplicate': b.append(b[0])
			elif mutation=='output': b[0]['stdout_sha256']='different'
			elif mutation=='fixture': b[0]['fixture_sha256']='different'
			else: b[0]['accounting']['total_Ir']+=1
			with self.subTest(mutation=mutation),self.assertRaises(ValueError):compare(self.rows(),b,self.policy,5)
		with self.assertRaises(ValueError):compare(self.rows(5,5),self.rows(),self.policy,5)

	def receipt(self, rows, revision='candidate'):
		return dict(status='PASS',revision=revision,recipe={'CPU':'a','toolchain':'b','environment':'c'},policy_sha256=identity(self.policy),records=rows)

	def test_cap_and_stale_anchor_even_with_identity(self):
		anchor=self.receipt(self.rows(),self.policy['anchor']);base=self.receipt(self.rows(100800,504000));candidate=self.receipt(self.rows(101200,506000))
		result=score(base,candidate,anchor,self.policy)
		self.assertTrue(all(r['pass_all'] for r in result['per_record']));self.assertEqual(result['status'],'FAIL')
		for mutation in ('missing','policy','CPU','toolchain','environment','anchor'):
			a=copy.deepcopy(anchor)
			if mutation=='missing':a=None
			elif mutation=='policy':a['policy_sha256']='stale'
			elif mutation=='anchor':a['revision']='wrong'
			else:a['recipe'][mutation]='changed'
			with self.subTest(mutation=mutation),self.assertRaises(ValueError):score(base,candidate,a,self.policy)

	def test_anchor_rebuild_proof_cannot_hide_execution_changes(self):
		r=dict(cpu=['cpu'],environment={},run_environment={},valgrind='v',fixtures={},policy={},tooling={},build_environment='old')
		anchor=self.receipt(self.rows(),self.policy['anchor']);anchor['recipe']=r
		candidate=self.receipt(self.rows());candidate['recipe']=dict(r,build_environment='new')
		base=copy.deepcopy(candidate)
		anchor.update(binary_sha256='a'*64,reuse_proof=dict(current_recipe=candidate['recipe'],binary_sha256='a'*64,current_build={'sha256':'a'*64}))
		self.assertEqual(score(base,candidate,anchor,self.policy)['status'],'PASS')
		bad=copy.deepcopy(anchor);bad['recipe']['cpu']=['other']
		with self.assertRaises(ValueError):score(base,candidate,bad,self.policy)
		bad=copy.deepcopy(anchor);bad['reuse_proof']['current_build']['sha256']='b'*64
		with self.assertRaises(ValueError):score(base,candidate,bad,self.policy)

	def test_policy_duplicate_or_limits(self):
		for field,value in [('inventory',self.policy['inventory']*2),('per_record_per_mille',6)]:
			p=copy.deepcopy(self.policy);p[field]=value
			with self.assertRaises(ValueError):validate_policy(p)




class AnchorLineage(unittest.TestCase):
	def test_successive_links_and_skipping(self):
		from drift_band import INITIAL_ANCHOR, anchor_transition
		a=dict(anchor=INITIAL_ANCHOR,anchor_transition=None)
		b=dict(anchor='b'*40,anchor_transition=dict(old=a['anchor'],new='b'*40,plan='1'*40,path='plans/adr/9998.md'))
		c=dict(anchor='c'*40,anchor_transition=dict(old=b['anchor'],new='c'*40,plan='2'*40,path='plans/adr/9999.md'))
		self.assertIsNone(anchor_transition(a,None))
		self.assertEqual(anchor_transition(b,a),b['anchor_transition'])
		self.assertEqual(anchor_transition(c,b),c['anchor_transition'])
		self.assertIsNone(anchor_transition(c,c))
		with self.assertRaises(ValueError):anchor_transition(c,a)
		bad=copy.deepcopy(c);bad['anchor_transition']['old']=a['anchor']
		with self.assertRaises(ValueError):anchor_transition(bad,b)
		bad['anchor_transition']='approved'
		with self.assertRaises(ValueError):anchor_transition(bad,b)

class MeasurementRecipe(unittest.TestCase):
	def test_build_environment_ignores_unrelated_and_rejects_unknown_overrides(self):
		from paired_build import build_environment
		a={'PATH':'/tools','HOME':'/tmp','RUSTFLAGS':'-Copt-level=3'}
		self.assertEqual(build_environment(a),build_environment(dict(a,SSH_AUTH_SOCK='different',TERM='different',PWD='different')))
		self.assertNotEqual(build_environment(a),build_environment(dict(a,RUSTFLAGS='-Copt-level=2')))
		with self.assertRaises(ValueError):build_environment(dict(a,RUST_SOMETHING='unknown'))

	def test_capture_key_uses_bytes_and_execution_inputs(self):
		from fixed_route_gate import capture_key
		r = dict(cpu=['cpu'], environment={'digest':'x'}, run_environment={'PATH':'/bin'}, valgrind='v', fixtures={'f':'a'},
			policy={'matrix':'x'}, tooling={'tools/column/paired_build.py':'old','capture.py':'same'}, build_environment='old', toolchain='old')
		changed=copy.deepcopy(r);changed.update(build_environment='new',toolchain='new');changed['tooling']['tools/column/paired_build.py']='new'
		self.assertEqual(capture_key('a'*64,r),capture_key('a'*64,changed))
		self.assertNotEqual(capture_key('a'*64,r),capture_key('b'*64,r))
		for key in ('cpu','environment','run_environment','valgrind','fixtures','policy'):
			changed=copy.deepcopy(r);changed[key]='changed'
			self.assertNotEqual(capture_key('a'*64,r),capture_key('a'*64,changed),key)
		changed=copy.deepcopy(r);changed['tooling']['capture.py']='new'
		self.assertNotEqual(capture_key('a'*64,r),capture_key('a'*64,changed))

	def test_stable_path_and_locales(self):
		from paired_build import build_environment
		self.assertEqual(build_environment({'PATH':'/one','LANG':'x','LC_CTYPE':'a'}),build_environment({'PATH':'/two','LANG':'y','LC_ALL':'b'}))

class AnchorReuse(unittest.TestCase):
	def test_verified_import_and_invalidation(self):
		import tempfile
		import child_result as child
		from fixed_route_gate import reuse_anchor, verified
		from ir_pool import capture_environment
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp); profile=root/'case/profile'; profile.parent.mkdir();profile.write_text('counts')
			binary=root/'binary';binary.write_bytes(b'code')
			build=dict(binary=str(binary),sha256=child.digest(binary))
			env=capture_environment()
			r=dict(cpu=['cpu'],environment=child.environment(env),valgrind='v',fixtures={},policy={},tooling={},build_environment='old')
			manifest=root/'MANIFEST.json';child.atomic_json(manifest,dict(environment=env,binaries={'baseline':build}))
			receipt=root/'original.json';child.atomic_json(receipt,dict(status='PASS',revision='anchor',recipe=r,
				records=[dict(profile=str(profile),role='baseline')],artifacts={str(binary):child.digest(binary),str(profile):child.digest(profile)}))
			current=dict(r,build_environment='new',run_environment=env)
			reused=reuse_anchor(receipt,build,current,{'anchor':'anchor'})
			self.assertEqual(reused['binary_sha256'],build['sha256']);verified(reused)
			self.assertEqual(json.loads(receipt.read_text())['recipe'],r)
			for key in ('cpu','valgrind','run_environment'):
				changed=dict(current);changed[key]='changed'
				with self.assertRaisesRegex(ValueError,'new capture required'):reuse_anchor(receipt,build,changed,{'anchor':'anchor'})
			other=root/'other';other.write_bytes(b'codf')
			with self.assertRaisesRegex(ValueError,'new capture required'):reuse_anchor(receipt,dict(binary=str(other),sha256=child.digest(other)),current,{'anchor':'anchor'})
			manifest.write_text('tampered')
			with self.assertRaisesRegex(ValueError,'artifact changed'):verified(reused)

if __name__=='__main__':unittest.main()
