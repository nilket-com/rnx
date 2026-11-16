import json
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch
import suite_stage as suite


class DeclaredOutput(unittest.TestCase):
	def test_api_twins_are_required_and_cannot_be_vacuous(self):
		docs = [c for c in suite.recipes('rune-suite') if '--doc' in c]
		self.assertEqual({c[-1] for c in docs}, {'host_function_owned_output', 'host_function_string_input', 'ExecutionRuntime', 'function::host::HostOutput', 'opaque_factory', 'opaque_method'})
		self.assertTrue(all('execution' in c[c.index('--features') + 1].split(',') for c in docs))
		for command in docs:
			expected = {'host_function_owned_output': 4, 'host_function_string_input': 8, 'ExecutionRuntime': 2, 'function::host::HostOutput': 4, 'opaque_factory': 2, 'opaque_method': 5}[command[-1]]
			for count in (0, 1, expected - 1, expected, expected + 1):
				with self.subTest(selector=command[-1], count=count), tempfile.TemporaryDirectory() as tmp:
					listing = ''.join(f'twin{i}: test\n' for i in range(count))
					with patch.object(suite, 'recipes', return_value=[command]), patch.object(suite, 'invoke', side_effect=[listing, f'test result: ok. {count} passed;']):
						if count == expected:
							self.assertEqual(suite.run('rune-suite', Path(tmp))[0]['passed'], expected)
						else:
							with self.assertRaises(ValueError): suite.run('rune-suite', Path(tmp))

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


class PackageChecks(unittest.TestCase):
	def test_probe_build_and_lint_use_configured_headers_and_refuse_missing_headers(self):
		manifest = 'rune/tools/xtask/column/probe/Cargo.toml'
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp)
			header = root/'include/valgrind/callgrind.h'
			header.parent.mkdir(parents=True); header.touch()
			with patch('fixed_route_gate.valgrind_root', return_value=root), patch.object(suite.subprocess, 'run') as run:
				for index, verb in enumerate(('check', 'clippy')):
					suite.invoke(['cargo', '+1.98.1', verb, '--manifest-path', manifest], root, root/f'{index}.log')
					self.assertEqual(run.call_args.kwargs['env']['VALGRIND_INCLUDE'], str(root/'include'))
				header.unlink(); run.reset_mock()
				with self.assertRaisesRegex(ValueError, 'requires Valgrind include'):
					suite.invoke(['cargo', '+1.98.1', 'check', '--manifest-path', manifest], root, root/'missing.log')
				run.assert_not_called()
				suite.invoke(['cargo', '+1.98.1', 'fmt', '--manifest-path', manifest], root, root/'format.log')
				self.assertIsNone(run.call_args.kwargs['env'])

	def test_every_registered_recipe_propagates_package_build_failure(self):
		for manifest, (name, commands) in suite.PACKAGE_CHECKS.items():
			with self.subTest(manifest=manifest), tempfile.TemporaryDirectory() as tmp:
				root = Path(tmp); header = root/'include/valgrind/callgrind.h'
				header.parent.mkdir(parents=True); header.touch()
				self.assertTrue(all(manifest in c for c in suite.recipes(name)))
				with patch('fixed_route_gate.valgrind_root', return_value=root), patch.object(suite.subprocess, 'run', side_effect=subprocess.CalledProcessError(101, commands[0])) as run:
					with self.assertRaises(subprocess.CalledProcessError): suite.run(name, Path(tmp))
					self.assertEqual(run.call_count, 1)
					self.assertTrue(run.call_args.kwargs['check'])
