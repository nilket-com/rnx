import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import child_result as child
import failure_detail as detail
from cross_program_gate import stage


class FailureControls(unittest.TestCase):
	def setUp(self):
		self.temp = tempfile.TemporaryDirectory(); self.addCleanup(self.temp.cleanup)
		self.root = Path(self.temp.name)

	def test_large_log_and_planted_environment_and_argv_secrets(self):
		source = self.root/'source'; source.mkdir(); out = self.root/'retained'; out.mkdir()
		secret = 'PLANTED_PRIVATE_VALUE_0271_DO_NOT_KEEP'
		argv_secret = 'PLANTED_ARGV_VALUE_0271_DO_NOT_KEEP'
		receipt = source/'RESULT.json'
		child.atomic_json(receipt, dict(status='FAIL',error='real failure '+secret, command=['bad',argv_secret],
			environment={'token':secret}, accounting={'big':'x'*100000}))
		log = source/'child.log'
		log.write_text(('argv: ["child", "--token='+argv_secret+'"]\n'+repr(['child',argv_secret])+'\n'+secret+'\n')*50000+'actual error\n')
		reference = detail.retain(out,'capture',receipt=receipt,logs=[log],roots=[source],environment={'RNX_SECRET':secret})
		index = json.loads(Path(reference['path']).read_text())
		text = ''.join(p.read_text() for p in out.rglob('*') if p.is_file())
		self.assertNotIn(secret,text); self.assertNotIn(argv_secret,text)
		self.assertNotIn('accounting',text); self.assertNotIn('"command"',text)
		self.assertIn('real failure',text); self.assertIn('actual error',text)
		self.assertTrue(any(f['truncated'] for f in index['files']))
		self.assertLessEqual(sum(p.stat().st_size for p in out.rglob('*') if p.is_file()),detail.BUNDLE_CAP)
		self.assertTrue(all(f['bytes']<=detail.FILE_CAP for f in index['files']))

	def test_sanitizer_keeps_numbers_paths_versions_and_option_prose(self):
		import final
		home = str(Path.home())
		env = dict(SHLVL='1', X='0', HOME=home, USER='me', TOKEN='s3cr3t-abcdef',
			PATH='/usr/bin:/opt/bin', TOOL_PATH='/opt/bin', COUNT='123456789', LC_TEST='long-locale')
		inputs = ['InsufficientMemory: host_available_bytes=41000000000, workers=0, reserve_bytes=2147483648',
			f'File "{home}/work/rnx/tools/a.py", line 1, in x',
			'exit 1 with rust 1.98.1; run cargo test --locked again; s3cr3t-abcdef']
		self.assertIs(final.redact, detail.sanitize)
		for text in inputs:
			self.assertEqual(detail.sanitize(text, env), text.replace(env['TOKEN'], '<redacted>'))
		self.assertEqual(detail.sanitize('/opt/bin 123456789 long-locale', env), '/opt/bin 123456789 long-locale')
		for flag in ('--token', '--api-key', '--password', '--client-secret', '--auth'):
			for separator in ('=', ' '):
				text = f'failed with {flag}{separator}"unknown-secret-value"; run --locked again'
				self.assertEqual(detail.sanitize(text, {}), f'failed with {flag}{separator}<redacted>; run --locked again')

	def test_secret_split_at_each_excerpt_boundary_leaves_no_fragment(self):
		import final
		source = self.root/'source'; source.mkdir(); out = self.root/'retained'; out.mkdir()
		secret = 'BOUNDARY_PRIVATE_SENTINEL_0271_NEVER_RETAIN'
		data = bytearray(b'a\n'*(3*detail.FILE_CAP//2))
		for offset in (detail.FILE_CAP//2-10, len(data)-detail.FILE_CAP//2-10):
			data[offset:offset+len(secret)] = secret.encode()
		log = source/'child.log'; log.write_bytes(data)
		detail.retain(out, 'capture', logs=[log], roots=[source], environment={'TOKEN': secret})
		text = ''.join(p.read_text() for p in out.rglob('*') if p.is_file())
		self.assertNotIn(secret[:10], text); self.assertNotIn(secret[10:], text)
		self.assertIn('<truncated>', text)
		# The final's older general diagnostic stream uses the same safe excerpts.
		data = bytearray(b'a\n'*(3*32768//2))
		for offset in (32768//2-10, len(data)-32768//2-10): data[offset:offset+len(secret)] = secret.encode()
		log.write_bytes(data)
		final.diagnostics(out,[dict(stage='capture',path=str(source))],'workers=0',{'TOKEN':secret,'X':'0'})
		text = (out/'DIAGNOSTICS.json').read_text()
		self.assertNotIn(secret[:10],text); self.assertNotIn(secret[10:],text)
		self.assertIn('workers=0',text)

	def test_named_admission_numbers_survive_real_payload_release(self):
		from final_evidence import Owners
		from ir_resources import fixed_budget, memory
		observation = dict(cpu_ids=[0], mem_available_bytes=41000000000, ancestor_headroom_bytes=1073741824,
			binding_cgroup='/fixture/capture', binding_limit='memory.high')
		with self.assertRaises(memory.InsufficientMemory) as caught: fixed_budget(observation)
		error = caught.exception
		out = self.root/'final'; owner = Owners(self.root/'owners', out); self.addCleanup(owner.close)
		bulk = owner.assign(self.root, 'capture')
		child.atomic_json(bulk/'RESULT.json', dict(status='FAIL',error=str(error),admission=error.numbers))
		(bulk/'child.log').write_text(str(error)+'\n')
		detail.retain(out, 'capture', receipt=bulk/'RESULT.json', roots=[bulk],
			environment=dict(SHLVL='1', X='0', HOME=str(Path.home()), TOKEN='s3cr3t-abcdef'))
		self.assertEqual(owner.release()['status'], 'PASS'); self.assertFalse(bulk.exists())
		index = json.loads((out/'FAILURE.json').read_text())
		for key, number in error.numbers.items(): self.assertIn(f'{key}={number}', index['error'])
		projection = json.loads((out/index['files'][0]['path']).read_text())
		self.assertEqual(projection['admission'], error.numbers)

	def test_nested_leaf_prioritized_and_projection_is_not_acceptance(self):
		source = self.root/'source'; source.mkdir(); case = source/'case'; case.mkdir()
		child.atomic_json(case/'RESULT.json',dict(status='FAIL',kind='fib',error='leaf broke',configuration='private'))
		(case/'child.log').write_text('leaf log\n')
		child.atomic_json(source/'RESULT.json',dict(status='FAIL',error='capture failed',records=[str(case/'RESULT.json')]))
		out = self.root/'out'; out.mkdir()
		reference = detail.retain(out,'capture',receipt=source/'RESULT.json',roots=[source],environment={})
		row = json.loads(Path(reference['path']).read_text())
		self.assertEqual(row['error'],'leaf broke'); self.assertTrue(row['diagnostic_only'])
		self.assertIn('leaf broke',(out/row['files'][0]['path']).read_text())
		copy = self.root/'copy'; copy.mkdir()
		copied = detail.copy_bundle(copy,reference,roots=[out],environment={})
		self.assertEqual(copied['error'],'leaf broke')
		(out/row['files'][0]['path']).write_text('tampered')
		(self.root/'bad').mkdir()
		with self.assertRaisesRegex(ValueError,'changed'):
			detail.copy_bundle(self.root/'bad',reference,roots=[out],environment={})

	def test_links_outside_paths_and_missing_receipt_keep_available_log(self):
		source = self.root/'source'; source.mkdir(); (source/'child.log').write_text('original cause\n')
		out = self.root/'out'; out.mkdir()
		reference = detail.retain(out,'capture',receipt=source/'missing',logs=[source/'child.log'],roots=[source],error='missing receipt',environment={})
		self.assertIn('original cause',reference['error'])
		self.assertTrue(json.loads(Path(reference['path']).read_text())['omitted'])
		outside = self.root/'outside'; outside.write_text('PRIVATE')
		(source/'linked').symlink_to(outside)
		for path in (outside, source/'linked'):
			with self.assertRaises(ValueError): detail.checked(path,[source])

	def test_missing_child_receipt_and_timeout_have_wrapper_receipts(self):
		for mode in ('missing','timeout'):
			out = self.root/mode; out.mkdir()
			if mode == 'timeout':
				context = patch('cross_program_gate.subprocess.run',side_effect=subprocess.TimeoutExpired(['child','--token=PRIVATE'],1))
			else: context = patch('cross_program_gate.subprocess.run',return_value=SimpleNamespace(returncode=1))
			with context, self.assertRaises(detail.StageFailure): stage(out,'capture',['child'],out/'RESULT.json')
			self.assertEqual(json.loads((out/'capture-stage.json').read_text())['status'],'FAIL')
			row = json.loads((out/'FAILURE.json').read_text())
			self.assertTrue(row['files']); self.assertNotIn('PRIVATE',json.dumps(row))

	def test_receipt_and_exit_both_required_and_error_truncation_marked(self):
		for code, status in ((0,'FAIL'),(1,'PASS')):
			out = self.root/f'mismatch-{code}'; out.mkdir()
			child.atomic_json(out/'RESULT.json',dict(status=status,error='x'*20000))
			with patch('cross_program_gate.subprocess.run',return_value=SimpleNamespace(returncode=code)),self.assertRaises(detail.StageFailure):
				stage(out,'capture',['child'],out/'RESULT.json')
			index = json.loads((out/'FAILURE.json').read_text())
			self.assertTrue(index['error_truncated']); self.assertEqual(len(index['error'].encode()),detail.ERROR_CAP)

	def test_rule_failure_identity_without_measurement_payload(self):
		row = dict(status='FAIL',per_record=[dict(kind='fib',mode='off',metric='W',level_pass=True,growth_pass=False,pass_all=False,B1=999)])
		text = json.dumps(detail.projection(row,{}))
		self.assertIn('growth_pass=False',text); self.assertNotIn('B1',text)

	def exercise_release(self, nested, *, drop_retention=False):
		import final
		from final_evidence import Owners
		root = self.root/('nested' if nested else 'direct'); root.mkdir()
		out = root/'final'
		selection = dict(scope='explicit',record={'candidate':'c'},source='s',policy='p',stages=[['broken',['command']]])
		def execute(command, **kw):
			bulk = Path(kw['env']['RNX_FINAL_ROUTING_ROOT'])
			case = bulk/'case'; case.mkdir()
			child.atomic_json(case/'RESULT.json',dict(status='FAIL',error='THE ACTUAL CHILD CAUSE'))
			(case/'child.log').write_text('THE FAILING CHILD LOG\n')
			semantic = bulk/'RESULT.json'
			if nested:
				child.atomic_json(semantic,dict(status='FAIL',error='wrapper failed',records=[str(case/'RESULT.json')]))
				reference = detail.retain(bulk,'capture',receipt=semantic,roots=[bulk],environment={})
				child.atomic_json(semantic,dict(status='FAIL',failure=reference,error='wrapper failed'))
			else: child.atomic_json(semantic,dict(status='FAIL',error='THE ACTUAL CHILD CAUSE'))
			kw['stdout'].write('THE FAILING CHILD LOG\n'); kw['stdout'].flush()
			d = kw['directory']; d.mkdir()
			child.atomic_json(d/'CHILD.json',dict(receipt=str(semantic)))
			child.atomic_json(d/'RESULT.json',dict(status='FAIL',error='child exit is nonzero'))
			return SimpleNamespace(returncode=1,validation={'path':str(d/'RESULT.json'),'sha256':child.digest(d/'RESULT.json')})
		with patch.object(final,'disk_check'),patch.object(final,'tools_identity',return_value={}),patch.object(final.child,'head',return_value='c'),patch.object(final.gate,'select',return_value=selection),patch.object(final.gate,'check_selection'),patch.object(final.child,'run',side_effect=execute),patch.object(final,'compact',return_value={}):
			if drop_retention:
				with patch.object(final,'copy_bundle',return_value=None),patch.object(final,'retain',return_value=None):
					result = final.run(root,out,names=['broken'])
			else: result = final.run(root,out,names=['broken'])
		self.assertEqual(result['status'],'FAIL'); self.assertEqual(result['failed_stage'],'broken')
		self.assertFalse(result['publication_eligible']); self.assertFalse(result['complete_final'])
		self.assertFalse(list((root/'target/routing-cli').iterdir()))
		self.assertTrue((out/'FAILURE.json').is_file(),'failure evidence was released')
		self.assertIn('THE ACTUAL CHILD CAUSE',result['error'])
		acceptance = json.loads((out/'ACCEPTANCE.json').read_text()); self.assertEqual(acceptance['failed_stage'],'broken')
		text = ''.join(p.read_text() for p in (out/'failure').iterdir())
		self.assertIn('THE FAILING CHILD LOG',text)
		self.assertEqual(json.loads((out/'RELEASE.json').read_text())['status'],'PASS')

	def test_direct_failure_survives_release(self): self.exercise_release(False)
	def test_nested_failure_survives_release(self): self.exercise_release(True)
	def test_dropped_retention_mutation_is_detected(self):
		with self.assertRaisesRegex(AssertionError,'failure evidence was released'): self.exercise_release(True,drop_retention=True)


if __name__ == '__main__': unittest.main()
