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
		for name in ['src/new.rs','tests/a.rs']:
			self.assertTrue({'root-tests','inventory','cross'} <= set(g.required([name])))
		self.assertEqual(set(g.required(['tools/column/foo.py'])),{'commit-discipline','controller-controls'})
		with self.assertRaises(ValueError):g.required(['Cargo.toml'])
		(self.root/'src/a.rs').rename(self.root/'renamed.rs');self.commit('impl: adr 9999 controls')
		r=g.record(self.root);self.assertEqual(r['baseline'],self.baseline)
		self.assertIn('src/a.rs',r['paths']);self.assertIn('renamed.rs',r['paths']);self.assertIn('root-tests',r['required'])

	def test_auto_add_cross_and_reject_omission_or_spoof(self):
		self.write('src/a.rs','changed');self.commit('impl: adr 9999 controls')
		m=g.select(self.root,self.root/'runs',['format'])
		self.assertEqual({n for n,_ in m['stages']},{'format','cross','root-tests','inventory','commit-discipline','controller-controls'})
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
		evidence={n:{} for n,_ in m['stages']}
		def good(_, source):
			return rows.pop(0)
		rows=[{'command':c,'context':{'command':c,'kind':'producer','candidate':m['record']['candidate']}} for n,c in m['stages']]
		with patch.object(g.child,'evidence',side_effect=good):
			self.assertEqual(g.finalize(m,evidence,self.root)['status'],'PASS')
		rows=[{'command':c,'context':{'command':c,'kind':'leaf','candidate':m['record']['candidate']}} for n,c in m['stages']]
		with patch.object(g.child,'evidence',side_effect=good),self.assertRaises(ValueError):g.finalize(m,evidence,self.root)

	def test_consumer_selection_closes_over_producers(self):
		self.write('tools/a.py', 'a'); self.commit('impl: adr 9999 controls')
		m = g.select(self.root, self.root/'runs', ['suite-adapter-polars'])
		names = [n for n, _ in m['stages']]
		self.assertLess(names.index('polars-inventories'), names.index('suite-adapter-polars'))
		self.assertEqual(g.close_prerequisites([('a', []), ('b', []), ('c', [])], {'c'}, {'c': ('b',), 'b': ('a',)}), {'a', 'b', 'c'})

	def test_missing_or_late_producer_fails_closed(self):
		for commands in [[('consumer', [])], [('consumer', []), ('producer', [])]]:
			with self.subTest(commands=commands), self.assertRaisesRegex(ValueError, 'consumer.*producer'):
				g.close_prerequisites(commands, {'consumer'}, {'consumer': ('producer',)})

	def test_failed_producer_never_runs_consumer(self):
		import accept
		from types import SimpleNamespace
		out = self.root/'runs/failure'
		steps = [('polars-inventories', ['producer']), ('suite-adapter-polars', ['consumer'])]
		calls = []
		def fail(command, **kwargs):
			calls.append(command)
			return SimpleNamespace(returncode=1, validation={}, declared_output=None)
		with patch.object(accept, 'ROOT', self.root), patch.object(accept, 'MANIFESTS', []), \
			patch.object(accept, 'commands', return_value=steps), \
			patch.object(accept.gate_policy, 'select', return_value={}), \
			patch.object(accept.subprocess, 'run'), \
			patch.object(accept.subprocess, 'check_output', side_effect=lambda c, **kw: 'candidate\n' if 'rev-parse' in c else ''), \
			patch.object(accept.child_result, 'run', side_effect=fail), \
			patch('sys.argv', ['accept', '--candidate', 'candidate', '--out', str(out)]):
			self.assertEqual(accept.main(), 1)
		self.assertEqual(calls, [['producer']])
		self.assertEqual(json.loads((out/'RESULT.json').read_text())['status'], 'FAIL')


if __name__ == '__main__':unittest.main()
