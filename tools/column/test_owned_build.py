import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from owned_build import OwnedBuild
from child_result import digest
from miri_cache import Cache


class OwnershipControls(unittest.TestCase):
	def setUp(self):
		self.tmp = tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
		self.root = Path(self.tmp.name)

	def test_pass_and_failure_keep_evidence(self):
		for fail in (False, True):
			parent = self.root/str(fail); parent.mkdir()
			log = parent/'log'; log.write_text('diagnostic'); before = digest(log)
			try:
				with OwnedBuild(parent, 'target') as owned:
					self.assertTrue(owned.record.exists())
					owned.path.mkdir(); (owned.path/'object').write_bytes(b'x'*100)
					if fail: raise ValueError('original')
			except ValueError as error: self.assertEqual(str(error), 'original')
			self.assertFalse(owned.path.exists()); self.assertEqual(digest(log), before)
			self.assertEqual(owned.result['status'], 'PASS')
			self.assertGreaterEqual(owned.result['seconds'], 0)

	def test_cleanup_failure_preserves_original_and_fails_pass(self):
		for original in (False, True):
			parent = self.root/str(original); parent.mkdir()
			with patch('owned_build.shutil.rmtree', side_effect=OSError('cannot remove')):
				with self.assertRaisesRegex(ValueError if original else RuntimeError, 'original' if original else 'removal failed'):
					with OwnedBuild(parent, 'target') as owned:
						owned.path.mkdir()
						if original: raise ValueError('original')
			self.assertEqual(owned.result['status'], 'FAIL')
			self.assertIn('cannot remove', owned.result['error'])

	def test_outside_shared_symlink_and_retained_refused(self):
		with self.assertRaises(ValueError): OwnedBuild(self.root, '../outside')
		(self.root/'shared').mkdir()
		with self.assertRaises(ValueError): OwnedBuild(self.root, 'shared')
		with self.assertRaises(RuntimeError):
			with OwnedBuild(self.root, 'link') as owned: owned.path.symlink_to(self.root/'shared', target_is_directory=True)
		self.assertTrue((self.root/'shared').exists())
		with self.assertRaises(RuntimeError):
			with OwnedBuild(self.root, 'bound') as owned:
				owned.path.mkdir(); artifact=owned.path/'artifact'; artifact.write_text('keep'); owned.keep([artifact])
		self.assertTrue(artifact.exists())

	def test_miri_preparation_and_semantic_receipts_revalidate(self):
		cache = Cache(self.root/'cache', {'test':'cleanup'})
		def prepare(path):
			target=path/'target'; target.mkdir(); p=target/'harness'; p.write_text('prepared')
			return {'target':str(target)}, [p]
		prepared=cache.stage('prepare','',prepare)
		def action(path):
			with OwnedBuild(path,'target') as owned:
				owned.path.mkdir(); (owned.path/'harness').write_text('copy')
				log=path/'test.log'; log.write_text('PASS'); owned.keep([log])
			return {'names':['exact']}, [p for p in path.iterdir() if p.is_file()]
		cache.stage('test','exact',action)
		self.assertTrue(cache.stage('test','exact',action)['reused'])
		self.assertTrue(cache.stage('prepare','',prepare)['reused'])
		self.assertTrue(Path(prepared['target']).is_dir())

	def test_binary_copy_survives_and_host_check_runs_before_release(self):
		from paired_build import verify_copy
		from child_result import atomic_json
		with OwnedBuild(self.root,'cache-00') as owned:
			owned.path.mkdir(); (owned.path/'original').write_text('binary')
			binary=self.root/'rnx'; binary.write_text('binary'); sha=digest(binary)
			receipt=self.root/'BUILD.json'; atomic_json(receipt,dict(status='PASS',binary=str(binary),sha256=sha))
			verify_copy(owned.path,binary,sha,receipt)
			owned.keep([binary,receipt])
		self.assertEqual(digest(binary),sha)
		# The actual launcher ordering is exercised by the real cross gate.
		text=(Path(__file__).parents[1]/'verify.py').read_text()
		self.assertLess(text.index('--host-check'),text.index("rename(output/'CONTROLLER-RESULT.json')"))
		self.assertLess(text.index("rename(output/'CONTROLLER-RESULT.json')"),text.index("result['cleanup']"))

	def test_sigterm_launcher_reaches_child_and_grandchild(self):
		import os, signal, subprocess, sys
		read, write = os.pipe()
		self.addCleanup(os.close, read)
		# Readiness is a blocking pipe handshake, not a progress/status poll.
		grandchild = "import os,signal;os.write("+str(write)+",(str(os.getpid())+'\\n').encode());signal.pause()"
		child = "import os,signal,subprocess,sys;os.write("+str(write)+",(str(os.getpid())+'\\n').encode());subprocess.Popen([sys.executable,'-c',"+repr(grandchild)+"],pass_fds=("+str(write)+",));signal.pause()"
		launcher = "import sys;sys.path.insert(0,"+repr(str(Path(__file__).parent))+ ");from owned_build import run_process;run_process([sys.executable,'-c',"+repr(child)+"],pass_fds=("+str(write)+",))"
		with (self.root/'signals.log').open('w') as log:
			process = subprocess.Popen([sys.executable,'-c',launcher],pass_fds=(write,),stdout=log,stderr=log)
			os.close(write)
			try:
				with os.fdopen(os.dup(read),'r') as stream: pids=[int(stream.readline()),int(stream.readline())]
				process.send_signal(signal.SIGTERM)
				self.assertNotEqual(process.wait(timeout=10),0)
				for pid in pids:
					stat=Path('/proc')/str(pid)/'stat'
					# An orphan zombie is dead and cannot write; init reaps it.
					if stat.exists(): self.assertEqual(stat.read_text().rsplit(')',1)[1].split()[0],'Z')
			finally:
				if process.returncode is None: process.kill(); process.wait()


if __name__ == '__main__': unittest.main()

class WaitControls(unittest.TestCase):
	def test_handlers_restored_and_ignoring_child_killed(self):
		import os, signal, subprocess, sys
		from owned_build import run_process
		previous = {sig:signal.getsignal(sig) for sig in (signal.SIGTERM,signal.SIGINT)}
		for code, timeout, expected in [('pass',5,None),('raise SystemExit(4)',5,subprocess.CalledProcessError),
			("import signal;signal.signal(signal.SIGTERM,signal.SIG_IGN);signal.pause()",0.2,subprocess.TimeoutExpired)]:
			if expected:
				with self.assertRaises(expected): run_process([sys.executable,'-c',code],timeout=timeout,check=True,termination_grace=0.1)
			else: run_process([sys.executable,'-c',code],timeout=timeout,check=True)
			self.assertEqual(previous,{sig:signal.getsignal(sig) for sig in previous})

	def test_off_main_thread_refused(self):
		from concurrent.futures import ThreadPoolExecutor
		from owned_build import run_process
		with ThreadPoolExecutor(max_workers=1) as pool:
			with self.assertRaisesRegex(RuntimeError,'main thread'): pool.submit(run_process,['true']).result()
