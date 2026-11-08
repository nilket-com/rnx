import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import child_result as c


class Results(unittest.TestCase):
	def setUp(self):
		self.tmp = tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
		self.root = Path(self.tmp.name)
		subprocess.run(['git', 'init', '-q', str(self.root)], check=True)
		(self.root / '.gitignore').write_text('runs/\n')
		subprocess.run(['git', '-C', str(self.root), 'add', '.gitignore'], check=True)
		self.out = self.root / 'runs'; self.out.mkdir()
		self.script = self.root / 'producer.py'
		self.script.write_text('import sys\nsys.path.insert(0, '+repr(str(c.ROOT / 'tools/column'))+')\nimport child_result as c\nfrom pathlib import Path\nc.publish(Path(sys.argv[sys.argv.index("--out")+1])/"RESULT.json", {"status":"PASS"})\n')

	def invoke(self, code=None):
		if code is not None: self.script.write_text(code)
		result_dir = self.out / 'producer'; result_dir.mkdir()
		cmd = [sys.executable, str(self.script), '--out', str(result_dir)]
		with patch.object(c, 'PRODUCERS', {'producer.py'}), (self.out / 'stdout').open('w+') as log:
			return c.run(cmd, cwd=self.root, root=self.root, stdout=log, stderr=subprocess.STDOUT, directory=self.out/'attempt')

	def test_real_controller_and_raw_leaf(self):
		p = self.invoke(); self.assertEqual(p.returncode, 0)
		row = c.evidence(p.validation, c.source(self.root))
		self.assertEqual(row['context']['kind'], 'producer')
		with (self.out/'leaf.log').open('w+') as log:
			p = c.run([sys.executable, '--version'], cwd=self.root, root=self.root, stdout=log, stderr=subprocess.STDOUT, directory=self.out/'leaf')
		self.assertEqual(p.returncode, 0)

	def test_zero_exit_missing_receipt_is_failure(self):
		p = self.invoke('pass\n'); self.assertNotEqual(p.returncode, 0)
		row = json.loads(Path(p.validation['path']).read_text())
		self.assertEqual(row['process_exit'], 0); self.assertEqual(row['status'], 'FAIL')

	def test_fail_receipt_zero_exit_and_pass_nonzero_exit(self):
		original = self.script.read_text()
		for fail in (True, False):
			with self.subTest(fail=fail):
				code = original.replace('"PASS"', '"FAIL"') if fail else original+'raise SystemExit(7)\n'
				# Distinct attempts, no retries of failed evidence.
				self.out = self.root / 'runs' / str(fail); self.out.mkdir()
				p = self.invoke(code); self.assertNotEqual(p.returncode, 0)

	def test_wrong_context_source_and_malformed_receipts(self):
		p = self.invoke(); directory = Path(p.validation['path']).parent
		ctx = json.loads((directory/'CONTEXT.json').read_text())
		original = json.loads((directory/'CHILD.json').read_text())
		for key, value in [('invocation','stale'),('source','wrong'),('context','wrong'),('status','FAIL'),('schema',99)]:
			with self.subTest(key=key):
				row = dict(original); row[key] = value
				(directory/'CHILD.json').write_text(json.dumps(row))
				with self.assertRaises(ValueError): c.validate(directory, ctx, 0)
		(directory/'CHILD.json').write_text('{')
		with self.assertRaises(ValueError): c.validate(directory, ctx, 0)
		(directory/'CHILD.json').write_text(json.dumps(original))
		with self.assertRaises(ValueError): c.validate(directory, ctx, 1)
		Path(original['receipt']).write_text('{"status":"FAIL"}')
		with self.assertRaises(ValueError): c.validate(directory, ctx, 0)

	def test_preexisting_attempt_and_unregistered_child_fail_closed(self):
		p = self.invoke()
		with self.assertRaises(FileExistsError):
			c.run(['cargo','--version'],cwd=self.root,root=self.root,directory=self.out/'attempt')
		with self.assertRaises(ValueError): c.contract([sys.executable, str(self.script)],self.root,self.root)

	def test_source_change_and_spawn_failure_preserve_fail(self):
		code = self.script.read_text().replace('c.publish(', 'Path("changed").write_text("changed")\nc.publish(')
		p = self.invoke(code); self.assertNotEqual(p.returncode,0)
		with (self.out/'spawn.log').open('w+') as log:
			with patch.object(c.subprocess,'run',side_effect=OSError('spawn failed')):
				p = c.run(['cargo','--version'],cwd=self.root,root=self.root,stdout=log,directory=self.out/'spawn')
		self.assertEqual(json.loads(Path(p.validation['path']).read_text())['status'],'FAIL')

	def test_resume_archives_prior_and_rechecks_artifacts(self):
		first = self.invoke(); self.assertEqual(first.returncode, 0)
		old = (self.out/'producer/RESULT.json').read_bytes()
		cmd = [sys.executable, str(self.script), '--out', str(self.out/'producer'), '--resume']
		with patch.object(c,'PRODUCERS',{'producer.py'}), (self.out/'second.log').open('w+') as log:
			second=c.run(cmd,cwd=self.root,root=self.root,stdout=log,stderr=subprocess.STDOUT,directory=self.out/'second')
		self.assertEqual(second.returncode,0)
		self.assertEqual(next((self.out/'second').glob('PRIOR-*.json')).read_bytes(),old)
		c.evidence(second.validation,c.source(self.root))
		(self.out/'second.log').write_text('corrupt')
		with self.assertRaises(ValueError):c.evidence(second.validation,c.source(self.root))

	def test_completed_failure_cannot_be_resumed(self):
		(self.out/'producer').mkdir();(self.out/'producer/RESULT.json').write_text('{"status":"FAIL"}')
		cmd=[sys.executable,str(self.script),'--out',str(self.out/'producer'),'--resume']
		with patch.object(c,'PRODUCERS',{'producer.py'}), (self.out/'failed.log').open('w+') as log:
			p=c.run(cmd,cwd=self.root,root=self.root,stdout=log,stderr=subprocess.STDOUT,directory=self.out/'failed')
		self.assertNotEqual(p.returncode,0)
		self.assertEqual(json.loads((self.out/'producer/RESULT.json').read_text())['status'],'FAIL')

	def test_recipe_changes_and_unregistered_module_fail(self):
		p=self.invoke(); directory=Path(p.validation['path']).parent
		ctx=json.loads((directory/'CONTEXT.json').read_text()); changed=dict(ctx,environment='wrong')
		with self.assertRaises(ValueError):c.validate(directory,changed,0)
		with self.assertRaises(ValueError):c.contract([sys.executable,'-m','tools.column.verify'],self.root,self.root)

	def test_remote_invocation_recipe_and_finalization(self):
		ctx={'invocation':'unique','candidate':'commit','source':'tree','bootstrap_sha256':'recipe'}
		r={'status':'PASS','candidate':'commit','remote_context':ctx,'finalization':{'status':'PASS','candidate':'commit','source':'tree'}}
		c.validate_remote(0,r,ctx)
		with self.assertRaises(ValueError):c.validate_remote(1,r,ctx)
		for key in ctx:
			with self.subTest(key=key),self.assertRaises(ValueError):c.validate_remote(0,r,dict(ctx,**{key:'wrong'}))
		with self.assertRaises(ValueError):c.validate_remote(0,dict(r,finalization={}),ctx)

	def test_declared_environment_only_and_no_raw_values_in_receipts(self):
		base={'RUSTFLAGS':'private-recipe-value','CARGO_BUILD_JOBS':'4','RNX_MODE':'private-mode'}
		bound=c.environment(base)
		self.assertEqual(bound['keys'],sorted(base))
		self.assertEqual(bound,c.environment(dict(base,TERM='other',PWD='/elsewhere',SSH_AUTH_SOCK='socket',UNDECLARED_SECRET='secret')))
		self.assertNotEqual(bound,c.environment(dict(base,RUSTFLAGS='changed')))
		with patch.dict(os.environ,dict(base,UNDECLARED_SECRET='unique-secret-marker')):
			p=self.invoke()
		self.assertEqual(p.returncode,0)
		for path in self.out.rglob('*.json'):
			text=path.read_text()
			for forbidden in ('private-recipe-value','private-mode','unique-secret-marker','UNDECLARED_SECRET'):
				self.assertNotIn(forbidden,text)


if __name__ == '__main__': unittest.main()
