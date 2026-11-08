import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest
from unittest.mock import patch

import gate_policy as g


class Gates(unittest.TestCase):
	def setUp(self):
		self.tmp = tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
		self.root = Path(self.tmp.name)
		self.git('init','-q'); self.git('config','user.name','Test'); self.git('config','user.email','test@example.invalid')
		self.write('.gitignore','runs/\n'); self.write('src/a.rs','old'); self.commit('baseline')
		self.baseline=self.git('rev-parse','HEAD')
		self.write('plans/adr/9999.md','plan'); self.commit('plan: adr 9999 controls')

	def git(self,*args):
		return subprocess.check_output(['git','-C',str(self.root),*args],stderr=subprocess.DEVNULL).decode().strip()

	def write(self,name,data):
		p=self.root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(data)

	def commit(self,subject):
		self.git('add','-A');self.git('-c','commit.gpgSign=false','commit','-qm',subject)

	def test_conservative_paths_and_both_rename_sides(self):
		for name in ['src/new.rs','tools/project/src/lib.rs','tools/project/Cargo.toml','tools/project/Cargo.lock','rune/crates/new.rs','Cargo.toml','Cargo.lock','rune/Cargo.toml','rune/Cargo.lock','tools/verify.py','tools/verify/fixture.json']:
			self.assertEqual(g.required([name]),['cross'])
		self.assertEqual(g.required(['tools/column/foo.py']),[])
		(self.root/'src/a.rs').rename(self.root/'renamed.rs');self.commit('impl: adr 9999 controls')
		r=g.record(self.root);self.assertEqual(r['baseline'],self.baseline)
		self.assertIn('src/a.rs',r['paths']);self.assertIn('renamed.rs',r['paths']);self.assertEqual(r['required'],['cross'])

	def test_auto_add_cross_and_reject_omission_or_spoof(self):
		self.write('src/a.rs','changed');self.commit('impl: adr 9999 controls')
		m=g.select(self.root,self.root/'runs',['format'])
		self.assertEqual({n for n,_ in m['stages']},{'format','cross'})
		cross=dict(m['stages'])['cross']
		self.assertTrue(Path(cross[cross.index('--out')+1]).is_relative_to(self.root/'target/rnx-verify'))
		g.check_selection(m,self.root,final=True)
		for mutation in ('omit','command','baseline','policy','source'):
			bad=copy.deepcopy(m)
			if mutation=='omit':bad['stages']=[r for r in bad['stages'] if r[0]!='cross']
			elif mutation=='command':bad['stages'][-1][1]=['true']
			elif mutation=='baseline':bad['record']['baseline']=bad['record']['candidate']
			else:bad[mutation]='bad'
			with self.subTest(mutation=mutation),self.assertRaises(ValueError):g.check_selection(bad,self.root,final=True)

	def test_development_not_final_and_source_changes_fail(self):
		m=g.select(self.root,self.root/'runs',['format'])
		with self.assertRaises(ValueError):g.check_selection(m,self.root,final=True)
		self.write('tools/a.py','a');self.commit('impl: adr 9999 controls')
		m=g.select(self.root,self.root/'runs',['format'])
		self.write('src/a.rs','dirty')
		with self.assertRaises(ValueError):g.check_selection(m,self.root,final=True)

	def test_missing_wrong_command_and_nonproducer_cross_rejected(self):
		self.write('src/a.rs','changed');self.commit('impl: adr 9999 controls')
		m=g.select(self.root,self.root/'runs',['cross'])
		with self.assertRaises(ValueError):g.finalize(m,{},self.root)
		command=m['stages'][0][1]
		good={'command':command,'context':{'command':command,'kind':'producer','candidate':m['record']['candidate']}}
		with patch.object(g.child,'evidence',return_value=good):
			self.assertEqual(g.finalize(m,{'cross':{}},self.root)['status'],'PASS')
		for row in [{'command':['true'],'context':{'command':['true'],'kind':'producer'}},
			{'command':command,'context':{'command':command,'kind':'leaf'}}]:
			row['context']['candidate']=m['record']['candidate']
			with patch.object(g.child,'evidence',return_value=row),self.assertRaises(ValueError):g.finalize(m,{'cross':{}},self.root)


if __name__ == '__main__':unittest.main()
