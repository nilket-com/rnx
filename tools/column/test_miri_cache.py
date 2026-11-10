import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from miri_cache import Cache, description


class MiriCacheControls(unittest.TestCase):
	def setUp(self):
		self.tmp=tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
		self.root=Path(self.tmp.name)
		self.recipe=description(self.root, {'crate/src/lib.rs':'a'}, 'nightly', '-Zmiri-strict-provenance', 'b,a', {'miri':'sha'}, {'sha256':'env'})
		self.calls=0

	def action(self, path):
		self.calls+=1
		log=path/'output';log.write_text('test exact ... ok\ntest result: ok. 1 passed;')
		return {'names':['exact'], 'seconds':7}, [log]

	def cache(self, recipe=None):return Cache(self.root/'cache', recipe or self.recipe)

	def test_pass_and_prepared_target_reused_after_restart(self):
		cache=self.cache(); first=cache.stage('test','exact',self.action)
		self.assertFalse(first['reused'])
		(self.root/'unrelated-plan.md').write_text('amended')
		second=self.cache().stage('test','exact',self.action)
		self.assertTrue(second['reused']);self.assertEqual(self.calls,1)
		def prepare(path):
			self.calls+=1
			target=path/'target';target.mkdir();artifact=target/'prepared';artifact.write_text('build')
			return {'target':str(target)}, [artifact]
		a=cache.stage('prepare','',prepare);b=self.cache().stage('prepare','',prepare)
		self.assertEqual(a['target'],b['target']);self.assertTrue(b['reused']);self.assertEqual(self.calls,2)

	def test_every_recipe_change_and_test_name_invalidates(self):
		original=self.cache().stage('test','exact',self.action)['cache_key']
		for field in ('inputs','toolchain','miriflags','features','tools','environment'):
			mutant=copy.deepcopy(self.recipe);mutant[field]='different'
			row=self.cache(mutant).stage('test','exact',self.action)
			self.assertNotEqual(row['cache_key'],original);self.assertFalse(row['reused'])
		self.assertFalse(self.cache().stage('test','other',self.action)['reused'])

	def test_timeout_and_interruption_resume_but_semantic_failure_does_not(self):
		cache=self.cache()
		with self.assertRaises(subprocess.TimeoutExpired):
			cache.stage('test','timeout',lambda _: (_ for _ in ()).throw(subprocess.TimeoutExpired('miri',1)))
		self.assertFalse(cache.stage('test','timeout',self.action)['reused'])
		with self.assertRaises(KeyboardInterrupt):
			cache.stage('test','killed',lambda _: (_ for _ in ()).throw(KeyboardInterrupt()))
		self.assertFalse(cache.stage('test','killed',self.action)['reused'])
		with self.assertRaises(ValueError):cache.stage('test','bad',lambda _: (_ for _ in ()).throw(ValueError('UB')))
		with self.assertRaisesRegex(ValueError,'semantic failure'):cache.stage('test','bad',self.action)

	def test_corrupt_artifact_fails_closed(self):
		self.cache().stage('test','exact',self.action)
		log=next((self.root/'cache').glob('*/job/attempt-*/output'));log.write_text('tampered')
		with self.assertRaisesRegex(ValueError,'corrupt'):self.cache().stage('test','exact',self.action)

	def test_longest_first_uses_history_without_reusing_changed_source(self):
		cache=self.cache();cache.stage('test','exact',self.action)
		jobs=[{'name':'a','names':['unknown']},{'name':'z','names':['exact']}]
		self.assertEqual(cache.order(jobs)[0]['names'],['exact'])
		new=copy.deepcopy(self.recipe);new['inputs']={'crate/src/lib.rs':'b'}
		self.assertEqual(self.cache(new).order(jobs)[0]['names'],['exact'])
		self.assertFalse(self.cache(new).stage('test','exact',self.action)['reused'])


if __name__=='__main__': unittest.main()

class ProcessRestart(unittest.TestCase):
	def test_actual_kill_preserves_passed_sibling(self):
		import os, signal, sys
		with tempfile.TemporaryDirectory() as temp:
			root=Path(temp); cache=Cache(root/'cache',{'key':1})
			def passed(path):
				p=path/'evidence';p.write_text('PASS');return {'names':['done']},[p]
			first=cache.stage('test','done',passed)
			read,write=os.pipe()
			code='''import os,sys,signal
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from miri_cache import Cache
c=Cache(Path(sys.argv[2]),{'key':1})
def action(path):
 os.write(int(sys.argv[3]),b'ready')
 signal.pause()
c.stage('test','interrupted',action)
'''
			child=subprocess.Popen([sys.executable,'-B','-c',code,str(Path(__file__).parent.resolve()),str(root/'cache'),str(write)],pass_fds=(write,))
			os.close(write)
			try:
				self.assertEqual(os.read(read,5),b'ready');child.kill();child.wait(timeout=10)
			finally:
				os.close(read)
				if child.returncode is None:child.kill();child.wait()
			def forbidden(path):raise AssertionError('passed test repeated')
			self.assertTrue(Cache(root/'cache',{'key':1}).stage('test','done',forbidden)['reused'])
			self.assertFalse(cache.stage('test','interrupted',passed)['reused'])
			key=first['cache_key'];artifact=next((root/'cache'/key).glob('job/attempt-*/evidence'));artifact.write_text('bad')
			with self.assertRaises(ValueError):cache.stage('test','done',forbidden)
			archive=cache.invalidate(key,'controlled corruption test')
			self.assertTrue((archive/'INVALIDATED.json').is_file())
			self.assertFalse(cache.stage('test','done',passed)['reused'])
