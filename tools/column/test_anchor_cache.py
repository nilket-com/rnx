import copy
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import child_result as child
from anchor_cache import Cache
import drift_band as band
from final_evidence import assigned_root


class Anchors(unittest.TestCase):
	def setUp(self):
		self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup)
		self.root=Path(self.tmp.name);self.source=self.root/'source';self.source.mkdir()
		self.policy=json.loads(band.POLICY.read_text())
		binary=self.source/'rnx';binary.write_bytes(b'binary');self.binary=binary
		rows=[];artifacts={str(binary):child.digest(binary)}
		for k in self.policy['kernels']:
			for m in self.policy['modes']:
				for n in self.policy['sizes']:
					p=self.source/f'{k}-{m}-{n}';p.mkdir()
					for name in ('profile','program.rn','program.stdout'):(p/name).write_text(name)
					artifacts.update({str(f):child.digest(f) for f in p.iterdir()})
					rows.append(dict(status='PASS',kind=k,policy=m,n=n,profile=str(p/'profile'),profile_sha256=child.digest(p/'profile'),fixture_sha256=child.digest(p/'program.rn'),stdout_sha256=child.digest(p/'program.stdout'),accounting=dict(rnx_Ir=n*100000,other_Ir=0,dynamic_Ir=10,total_Ir=n*100000+10)))
		self.receipt=dict(status='PASS',revision=self.policy['anchor'],recipe={},policy_sha256=child.identity(self.policy),records=rows,artifacts=artifacts,binary_sha256=child.digest(binary),capture_key='key1')

	def test_independent_copies_and_hit_miss_scored_rows_equal(self):
		with Cache(self.root/'cache') as c:
			self.assertEqual(c.lookup('key1'),(None,'absent'))
			fresh=c.publish(self.receipt,self.binary,{'receipt_sha256':'provenance'})
			base=copy.deepcopy(self.receipt);base['revision']='baseline'
			candidate=copy.deepcopy(base);candidate['revision']='candidate'
			miss=band.score(base,candidate,fresh,self.policy)
			shutil.rmtree(self.source)
		with Cache(self.root/'cache') as c:
			hit,why=c.lookup('key1');self.assertEqual(why,'hit')
			self.assertEqual(band.score(base,candidate,hit,self.policy),miss)
			self.assertNotIn(str(self.source),json.dumps(hit))

	def test_missing_modified_and_metadata_corruption_rebuild(self):
		for mutation in ('missing','changed','metadata','whole-root'):
			with self.subTest(mutation=mutation),Cache(self.root/('cache-'+mutation)) as c:
				r=c.publish(self.receipt,self.binary,{})
				p=Path(r['records'][0]['profile'])
				if mutation=='missing':p.unlink()
				elif mutation=='changed':p.write_text('changed')
				elif mutation=='metadata':
					q=Path(c.state['entries'][0]['path'])/'ANCHOR.json';v=json.loads(q.read_text());v['records'][0]['accounting']['rnx_Ir']+=1;child.atomic_json(q,v)
				else:shutil.rmtree(c.state['entries'][0]['path'])
				self.assertEqual(c.lookup('key1'),(None,'verification failed'))
				c.publish(self.receipt,self.binary,{})
				self.assertEqual(c.lookup('key1')[1],'hit')

	def test_one_entry_bound_and_failed_replacement_keeps_previous(self):
		with Cache(self.root/'cache') as c:
			c.publish(self.receipt,self.binary,{})
			second=dict(self.receipt,capture_key='key2')
			self.assertEqual(c.lookup('key2'),(None,'key differs'))
			with patch('anchor_cache.shutil.copyfile',side_effect=OSError('disk')):
				with self.assertRaises(OSError):c.publish(second,self.binary,{})
			self.assertEqual(c.lookup('key1')[1],'hit')
			c.publish(second,self.binary,{})
			self.assertEqual(len(c.state['entries']),1)
			self.assertEqual(len(list(c.root.glob('entry-*'))),1)
			self.assertEqual(c.lookup('key1'),(None,'key differs'))

	def test_root_replacement_and_symlinks_refused(self):
		with Cache(self.root/'cache') as c:
			c.publish(self.receipt,self.binary,{})
			row=c.state['entries'][0];p=Path(row['path']);old=p.with_name('saved');p.rename(old);p.mkdir()
			with self.assertRaises(ValueError):c.lookup('key1')
			p.rmdir();p.symlink_to(old,target_is_directory=True)
			with self.assertRaises(ValueError):c.lookup('key1')

	def test_interrupted_publish_recovered_without_final_directory(self):
		with Cache(self.root/'cache') as c:
			c.publish(self.receipt,self.binary,{})
			original=c.remove
			with patch('anchor_cache.os.rename',side_effect=OSError('interrupted')),patch.object(c,'remove',side_effect=OSError('killed')):
				with self.assertRaises(OSError):c.publish(dict(self.receipt,capture_key='key2'),self.binary,{})
			self.assertEqual(len(c.state['entries']),2)
		shutil.rmtree(self.source)
		with Cache(self.root/'cache') as c:
			self.assertEqual(len(c.state['entries']),1);self.assertEqual(c.lookup('key1')[1],'hit')

	def test_lock_excludes_other_consumers_through_scoring(self):
		command=[sys.executable,'-c',"import fcntl,sys; f=open(sys.argv[1],'a+'); fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)",str(self.root/'cache/LOCK')]
		with Cache(self.root/'cache') as c:
			c.publish(self.receipt,self.binary,{})
			anchor,_=c.lookup('key1')
			band.score(self.receipt,self.receipt,anchor,self.policy)
			self.assertNotEqual(subprocess.run(command,capture_output=True).returncode,0)
		self.assertEqual(subprocess.run(command,capture_output=True).returncode,0)

	def test_assignment_and_early_baseline_deletion(self):
		with patch.dict(os.environ,{},clear=True):self.assertEqual(assigned_root('ASSIGNED',self.source),self.source)
		with patch.dict(os.environ,{'RNX_FINAL':'1'},clear=True):
			with self.assertRaises(ValueError):assigned_root('ASSIGNED',self.source)
			os.environ['ASSIGNED']=str(self.source)
			self.assertEqual(assigned_root('ASSIGNED',self.root),self.source)
			from suite_stage import invoke
			manifest=self.source/'Cargo.toml';manifest.write_text('baseline source')
			command=[sys.executable,'-c','from pathlib import Path; print(Path("Cargo.toml").read_text())']
			self.assertIn('baseline source',invoke(command,self.source,self.root/'before.log'))
			shutil.rmtree(self.source)
			with self.assertRaises(FileNotFoundError):invoke(command,self.source,self.root/'after.log')

if __name__=='__main__':unittest.main()
