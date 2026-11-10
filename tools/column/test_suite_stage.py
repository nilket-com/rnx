import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import suite_stage as suite


class DeclaredOutput(unittest.TestCase):
	def test_polars_clippy_keeps_existing_diagnostic_as_warning(self):
		from clippy import POLARS_LINT_ARGS, diagnostics
		command = ['cargo', 'clippy']
		self.assertEqual(suite.clippy_command(command, 'clippy-adapter-polars'),
			command + ['--message-format=json'] + list(POLARS_LINT_ARGS))
		self.assertEqual(suite.clippy_command(command, 'clippy-adapter-postgres'),
			command + ['--message-format=json'])
		self.assertTrue(all(c[-2:] == ['-D', 'warnings'] for c in suite.recipes('rune-clippy')))
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); log = root/'diagnostics.jsonl'
			row = {'reason': 'compiler-message', 'target': {'name': 'preview'},
				'message': {'level': 'warning', 'code': {'code': 'clippy::erasing_op'},
					'message': 'operation always zero', 'spans': []}}
			log.write_text(json.dumps(row)+'\n'); baseline = diagnostics(log, root)
			log.write_text((json.dumps(row)+'\n')*2)
			self.assertTrue(diagnostics(log, root)-baseline)

	def exercise(self, test_failure=False, cleanup_failure=False):
		with tempfile.TemporaryDirectory() as tmp:
			out=Path(tmp)/'out';events=[]
			def run(*args):
				events.append('test')
				if test_failure:raise RuntimeError('test failed')
				return []
			def finish(*args):
				events.append('restore')
				if cleanup_failure:raise ValueError('unexpected mutation')
				return None
			def publish(path,result):
				events.append('publish');self.assertEqual(events,['test','restore','publish'])
				self.assertEqual(result['status'],'FAIL' if test_failure or cleanup_failure else 'PASS')
			with patch('sys.argv',['suite','--stage','suite-adapter-polars','--out',str(out)]),patch.object(suite,'run',side_effect=run),patch('stage_outputs.finish_stage',side_effect=finish),patch.object(suite.child,'publish',side_effect=publish):
				self.assertEqual(suite.main(),bool(test_failure or cleanup_failure))

	def test_cleanup_before_receipt(self):self.exercise()
	def test_failure_remains_failure(self):self.exercise(test_failure=True)
	def test_cleanup_rejection_fails(self):self.exercise(cleanup_failure=True)
