import fcntl
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import child_result as child
from final_evidence import Owners, compact, recover, release, revalidate, tree


class EvidenceControls(unittest.TestCase):
	def setUp(self):
		self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
		self.root=Path(self.tmp.name);self.out=self.root/'final';self.registry=self.root/'owners'
		self.owner=Owners(self.registry,self.out);self.addCleanup(self.owner.close)

	def test_identity_and_interior_link(self):
		p=self.owner.assign(self.root,'bulk');(p/'data').write_text('abc')
		outside=self.root/'outside';outside.write_text('keep');(p/'link').symlink_to(outside)
		proof=tree(p);self.assertEqual(proof['entries'],2)
		r=self.owner.release();self.assertEqual(r['status'],'PASS');self.assertFalse(p.exists());self.assertEqual(outside.read_text(),'keep')
		self.assertEqual(release(self.owner.path)['status'],'PASS')

	def test_refusals(self):
		with self.assertRaises(ValueError):self.owner.assign(self.root,'x',name='../escape')
		(self.root/'exists').mkdir()
		with self.assertRaises(ValueError):self.owner.assign(self.root,'x',name='exists')
		p=self.owner.assign(self.root,'bulk');p.rename(self.root/'saved');p.symlink_to(self.root/'saved',target_is_directory=True)
		with self.assertRaises(ValueError):self.owner.release()
		p.unlink();p.mkdir()
		with self.assertRaises(ValueError):self.owner.release()

	def test_ancestor_and_incomplete(self):
		parent=self.root/'parent';parent.mkdir();p=self.owner.assign(parent,'bulk')
		parent.rename(self.root/'renamed');parent.symlink_to(self.root/'renamed',target_is_directory=True)
		with self.assertRaises(ValueError):self.owner.release()
		parent.unlink();(self.root/'renamed').rename(parent)
		(p/'nonempty').write_text('keep')
		row=json.loads(self.owner.path.read_text());row['roots'][0]['state']='creating';child.atomic_json(self.owner.path,row)
		with self.assertRaises(ValueError):self.owner.release()

	def test_release_failure_and_retry_after_removal(self):
		p=self.owner.assign(self.root,'bulk')
		with patch('final_evidence.shutil.rmtree',side_effect=OSError('removal failed')):
			with self.assertRaises(OSError):self.owner.release()
		self.assertTrue(p.exists())
		p.rmdir() # crash after removal but before released journal
		self.assertEqual(self.owner.release()['status'],'PASS')

	def test_live_owner_and_killed_owner(self):
		with self.assertRaisesRegex(ValueError,'live final'):recover(self.registry)
		p=self.owner.assign(self.root,'bulk');self.owner.close()
		recover(self.registry);self.assertFalse(p.exists());self.assertTrue(list(self.registry.glob('*-RECOVERY.json')));self.assertFalse(list(self.registry.glob('*.owner.json')));self.assertFalse(list(self.registry.glob('*.lock')))

	def test_compact_and_revalidate(self):
		p=self.owner.assign(self.root,'bulk');(p/'data').write_text('secret payload')
		stage=self.out/'stage.json';child.atomic_json(stage,dict(status='PASS',process_exit=0,seconds=1,cleanup_seconds=0,
			context={'environment':child.environment({'RNX_SECRET':'secret env'})},artifacts={str(p/'data'):child.digest(p/'data')}))
		s={'record':{'candidate':'c'},'source':'s','policy':'p','stages':[['x',['declared-command']]]}
		m=compact(s,{'x':{'path':str(stage),'sha256':child.digest(stage)}},self.owner,{'compiler':'version'})
		self.assertNotIn('secret',json.dumps(m));self.assertIn('RNX_SECRET',json.dumps(m))
		child.atomic_json(self.out/'MANIFEST.json',m);child.atomic_json(self.out/'ACCEPTANCE.json',{'status':'PASS','candidate':'c'})
		r=self.owner.release();r.update(acceptance_sha256=child.digest(self.out/'ACCEPTANCE.json'),manifest_sha256=child.digest(self.out/'MANIFEST.json'))
		child.atomic_json(self.out/'RELEASE.json',r)
		child.atomic_json(self.out/'RESULT.json',dict(status='PASS',candidate='c',acceptance_scope='complete-final',complete_final=True,records={n:child.digest(self.out/(n+'.json')) for n in ('ACCEPTANCE','MANIFEST','RELEASE')}))
		self.assertEqual(revalidate(self.out)['bulk'],'released; not rehashed')
		(self.out/'RELEASE.json').unlink()
		with self.assertRaises(FileNotFoundError):revalidate(self.out)

	def test_manifest_cap_preserves_bulk(self):
		p=self.owner.assign(self.root,'bulk')
		s={'record':{'candidate':'c'},'source':'s','policy':'p','stages':[]}
		with self.assertRaises(ValueError):compact(s,{},self.owner,{'too_big':'x'*300000})
		self.assertTrue(p.exists())

	def test_missing_bound_file_still_fails(self):
		artifact=self.out/'raw';artifact.write_text('ok');row=self.out/'stage.json'
		child.atomic_json(row,dict(status='PASS',process_exit=0,context={'source':'s','kind':'command'},artifacts={str(artifact):child.digest(artifact)}))
		ref={'path':str(row),'sha256':child.digest(row)};artifact.unlink()
		with self.assertRaises(FileNotFoundError):child.evidence(ref,'s')

	def test_killed_driver_drains_registered_producer(self):
		# Readiness pipe, never a status polling loop.
		script=self.out/'driver.py'
		script.write_text('import sys,os\nfrom owned_build import run_process\nrun_process([sys.executable,"-c","import time;print(\\\"ready\\\",flush=True);time.sleep(120)"],env=dict(os.environ,RNX_FINAL_GROUPS='+repr(str(self.owner.groups))+'))\n')
		p=subprocess.Popen([sys.executable,str(script)],env=dict(os.environ,PYTHONPATH=str(Path(__file__).parent)),stdout=subprocess.PIPE,text=True)
		try:
			self.assertEqual(p.stdout.readline().strip(),'ready');p.kill();p.wait(timeout=10)
			self.owner.assign(self.root,'bulk');self.owner.close();shutil.rmtree(self.out);recover(self.registry)
			rows=json.loads(next(self.registry.glob('*-RECOVERY.json')).read_text())['producer_groups']
			self.assertTrue(rows and all(x['drained'] for x in rows))
		finally:
			if p.poll() is None:p.kill();p.wait()
			p.stdout.close()



class FinalOrderControls(unittest.TestCase):
	def exercise(self, fail_stage=False, fail_release=False, interrupted=False, terminal_signals=False, names=None):
		import final
		from types import SimpleNamespace
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);out=root/'final';events=[]
			s={'record':{'candidate':'c'},'source':'s','policy':'p','stages':[['stage',['command']]]}
			def execute(command,**kw):
				p=Path(kw['env']['RNX_FINAL_ROUTING_ROOT']);(p/'payload').write_text('data')
				if interrupted:raise KeyboardInterrupt('interrupted')
				d=kw['directory'];d.mkdir();r=dict(status='FAIL' if fail_stage else 'PASS',process_exit=int(fail_stage),seconds=0,cleanup_seconds=0,context={'environment':{'keys':[],'sha256':'x'}},artifacts={str(p/'payload'):child.digest(p/'payload')})
				child.atomic_json(d/'RESULT.json',r)
				return SimpleNamespace(returncode=int(fail_stage),validation={'path':str(d/'RESULT.json'),'sha256':child.digest(d/'RESULT.json')})
			def finalize(*args):
				self.assertTrue(list((root/'target/routing-cli').glob('*/payload')));events.append('validated');return {'status':'PASS'}
			actual=Owners.release
			def release_checked(owner):
				self.assertTrue((out/'ACCEPTANCE.json').exists());self.assertTrue((out/'MANIFEST.json').exists())
				events.append('release')
				if terminal_signals:
					os.kill(os.getpid(),signal.SIGTERM);os.kill(os.getpid(),signal.SIGINT)
				if fail_release:raise OSError('release failed')
				return actual(owner)
			with patch.object(final,'disk_check'),patch.object(final.child,'head',return_value='c'),patch.object(final.gate,'select',return_value=s),patch.object(final.gate,'check_selection'),patch.object(final.child,'run',side_effect=execute),patch.object(final.gate,'finalize',side_effect=finalize),patch.object(Owners,'release',release_checked):
				r=final.run(root,out,names=names)
			self.assertEqual(r['complete_final'],names is None and not (fail_stage or fail_release or interrupted))
			self.assertEqual(r['checks'],[{'name':'stage','exit':int(fail_stage)}] if not interrupted else [])
			self.assertEqual(r['status'],'FAIL' if fail_stage or fail_release or interrupted else 'PASS')
			if not fail_release:self.assertFalse(list((root/'target/routing-cli').iterdir()))
			if fail_stage:self.assertIn('stage failed',r['error'])
			if interrupted:self.assertIn('KeyboardInterrupt',r['error'])
			if not (fail_stage or interrupted):self.assertEqual(events,['validated','release'])
			if fail_stage or interrupted:
				from final_evidence import abandon_diagnostics
				self.assertLessEqual((out/'DIAGNOSTICS.json').stat().st_size,4*1024*1024)
				abandon_diagnostics(out);self.assertFalse((out/'DIAGNOSTICS.json').exists())
			if r['status']=='PASS':self.assertEqual(revalidate(out)['status'],'PASS')

	def test_pass_order(self):self.exercise()
	def test_stage_failure_first(self):self.exercise(fail_stage=True,fail_release=True)
	def test_failure_releases(self):self.exercise(fail_stage=True)
	def test_release_failure_fails_pass(self):self.exercise(fail_release=True)
	def test_interruption_releases_not_complete(self):self.exercise(interrupted=True)
	def test_second_signals_ignored_during_terminal(self):self.exercise(terminal_signals=True)
	def test_selected_scope_is_not_complete_final(self):self.exercise(names=['stage'])



class ReviewControls(unittest.TestCase):
	def test_creating_absent_and_empty(self):
		for absent in (False,True):
			with tempfile.TemporaryDirectory() as tmp:
				root=Path(tmp);owner=Owners(root/'registry',root/'out')
				try:
					p=owner.assign(root,'bulk')
					if absent:p.rmdir()
					row=json.loads(owner.path.read_text());row['roots'][0]['state']='creating'
					row['roots'][0].pop('inode');row['roots'][0].pop('device')
					child.atomic_json(owner.path,row)
					self.assertEqual(owner.release()['status'],'PASS');self.assertFalse(p.exists())
					child.atomic_json(root/'release.json',{'status':'PASS'})
					owner.retire();self.assertFalse(list((root/'registry').iterdir()))
				finally:owner.close()

	def test_retirement_interrupted_is_recoverable(self):
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);owner=Owners(root/'registry',root/'out')
			owner.assign(root,'bulk');owner.release()
			row=json.loads(owner.path.read_text());row['status']='retiring';child.atomic_json(owner.path,row)
			shutil.rmtree(owner.groups);owner.close();owner.path.with_suffix('.lock').unlink()
			recover(root/'registry');self.assertFalse(list((root/'registry').iterdir()))

	def test_failure_snapshot_errors_do_not_skip_release(self):
		import final
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);owner=Owners(root/'registry',root/'out');p=owner.assign(root,'stage')
			try:
				a={'status':'FAIL','candidate':'c','error':'original failure'}
				with patch.object(final,'compact',side_effect=KeyError('seconds')),patch.object(final,'diagnostics',side_effect=OSError('snapshot')):
					r=final.terminal(owner.out,owner,{'selected':True},{},{},a,{})
				self.assertEqual(r['status'],'PASS');self.assertFalse(p.exists())
				stored=json.loads((owner.out/'ACCEPTANCE.json').read_text())
				self.assertEqual(stored['error'],'original failure')
				self.assertIn('manifest_error',stored);self.assertIn('diagnostics_error',stored)
			finally:owner.close()

	def test_passing_manifest_failure_keeps_bulk(self):
		import final
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);owner=Owners(root/'registry',root/'out');p=owner.assign(root,'stage')
			try:
				with patch.object(final,'compact',side_effect=ValueError('cap')):
					r=final.terminal(owner.out,owner,{'selected':True},{},{},{'status':'PASS','candidate':'c'},{})
				self.assertEqual(r['status'],'FAIL');self.assertTrue(p.exists())
			finally:owner.close()

	def test_diagnostics_redact_values_and_share_stage_budget(self):
		import final
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);entries=[]
			secret='VERY_PRIVATE_ENV_SENTINEL_0260'
			for name in ('one','two'):
				p=root/name;p.mkdir();entries.append({'path':str(p),'stage':'same-stage'})
				for i in range(20):(p/f'{i}.stderr').write_text((secret+' x'*1000+'\n')*20)
			final.diagnostics(root,entries,secret,{'RNX_SECRET':secret})
			text=(root/'DIAGNOSTICS.json').read_text();self.assertNotIn(secret,text)
			self.assertLessEqual(sum(len(json.dumps(x).encode()) for x in json.loads(text)['files']),256*1024)

	def test_default_selects_all_and_scoped_flag_is_explicit(self):
		import final
		for flags,expected in (([],None),(['--stages','format,clippy'],['format','clippy'])):
			with patch('sys.argv',['final','--out','ignored',*flags]),patch.object(final,'run',return_value={'status':'PASS'}) as run,patch.object(final.signal,'signal'):
				self.assertEqual(final.main(),0);self.assertEqual(run.call_args.args[2],expected)

	def test_fresh_and_override_admission(self):
		import final
		with tempfile.TemporaryDirectory() as tmp,patch.dict(os.environ,{},clear=True):
			root=Path(tmp);(root/'target').mkdir()
			self.assertIsNone(final.fresh_violation(root,False))
			self.assertIn('preexisting',final.fresh_violation(root,True))
			os.environ['CARGO_TARGET_DIR']='override'
			self.assertIn('CARGO_TARGET_DIR',final.fresh_violation(root,False))

	def test_disk_check_and_post_stage_clean_check(self):
		import final
		from types import SimpleNamespace
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);out=root/'out';out.mkdir()
			with patch.object(final.subprocess,'run') as call:
				final.disk_check(root,out)
				self.assertIn(str(root/'ci/disk_space.py'),call.call_args.args[0]);self.assertTrue(call.call_args.kwargs['check'])
			owner=Owners(root/'registry',out);rows=[]
			try:
				with patch.object(final.gate,'check_selection',side_effect=[None,ValueError('dirty after stage')]),patch.object(final.child,'run',return_value=SimpleNamespace(returncode=0,validation={},declared_output={'archive':'a'})):
					with self.assertRaisesRegex(ValueError,'dirty after'):
						final.run_stages(root,out,{'stages':[['stage',['command']]]},owner,{}, {},rows)
				self.assertEqual(rows[0]['declared_output'],{'archive':'a'})
			finally:owner.close()

if __name__=='__main__':unittest.main()
