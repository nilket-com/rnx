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

	def test_required_complete_explicit_scopes(self):
		self.write('adapters/polars/src/example.rs','change');self.commit('impl: adr 9999 controls')
		required=g.select(self.root,self.root/'runs',scope='required')
		names=[n for n,_ in required['stages']]
		self.assertIn('polars-inventories',names)
		explicit=g.select(self.root,self.root/'runs',names,scope='explicit')
		self.assertEqual(explicit['stages'],required['stages'])
		self.assertFalse(explicit['publication_eligible']);self.assertTrue(required['publication_eligible'])
		with patch('fixed_route_gate.valgrind_root'):
			complete=g.select(self.root,self.root/'runs',scope='complete')
		import accept
		self.assertEqual(complete['stages'],[list(x) for x in accept.commands(self.root/'runs',root=self.root)])
		with patch('fixed_route_gate.valgrind_root'):
			for value in (required,explicit,complete):g.check_selection(value,self.root,final=True)
		bad=copy.deepcopy(required);bad['scope']='explicit'
		with self.assertRaises(ValueError):g.check_selection(bad,self.root,final=True)
		bad=copy.deepcopy(required);bad['stages'].append(['extra',['extra']])
		with self.assertRaises(ValueError):g.check_selection(bad,self.root,final=True)
		with self.assertRaises(ValueError):g.select(self.root,self.root/'runs',names,scope='required')

	def test_bulk_assignments_cover_producer_calls(self):
		import ast
		import accept
		import suite_stage
		root=Path(accept.ROOT)
		names=dict(accept.commands(self.root/'runs',root=root))
		self.assertFalse(set(accept.BULK_ROOTS)-names.keys())
		for name,command in names.items():
			producer=g.child.script(command,root)
			files=[producer] if producer is not None else []
			for source in files:
				tree=ast.parse(source.read_text())
				variables={n.args[0].value for n in ast.walk(tree) if isinstance(n,ast.Call) and isinstance(n.func,ast.Name) and n.func.id=='assigned_root'}
				if source.name=='suite_stage.py':
					# This producer's baseline reader is conditional on its actual recipes.
					if name=='rune-clippy' or not any('clippy' in c for c in suite_stage.recipes(name)):variables=set()
				for variable in variables:
					kind={'RNX_FINAL_BASELINE_ROOT':'baseline','RNX_FINAL_FIXED_ROOT':'fixed'}[variable]
					self.assertIn(kind,accept.BULK_ROOTS.get(name,()),name)
		self.assertIn('project',accept.BULK_ROOTS['generated-projects'])
		self.assertIn('cross',accept.BULK_ROOTS['cross'])

	def test_failed_producer_never_runs_consumer(self):
		import final
		from types import SimpleNamespace
		out=self.root/'runs/failure'
		steps=[('producer',['producer']),('consumer',['consumer'])];calls=[]
		selection={'record':{'candidate':'candidate'},'source':'s','policy':'p','stages':steps}
		def fail(command, **kwargs):
			calls.append(command)
			path=kwargs['directory'];path.mkdir()
			row=dict(status='FAIL',process_exit=1,seconds=0,cleanup_seconds=0,context={'environment':{'keys':[],'sha256':'x'}},artifacts={})
			g.child.atomic_json(path/'RESULT.json',row)
			return SimpleNamespace(returncode=1,validation={'path':str(path/'RESULT.json'),'sha256':g.child.digest(path/'RESULT.json')})
		with patch.object(final,'disk_check'), patch.object(final.child,'head',return_value='candidate'), patch.object(final.gate,'select',return_value=selection), \
			patch.object(final.gate,'check_selection'), patch.object(final.child,'run',side_effect=fail):
			self.assertEqual(final.run(self.root,out)['status'],'FAIL')
		self.assertEqual(calls,[['producer']])
		self.assertEqual(json.loads((out/'RELEASE.json').read_text())['status'],'PASS')
		self.assertIn('producer',json.loads((out/'RESULT.json').read_text())['error'])


if __name__ == '__main__':unittest.main()
