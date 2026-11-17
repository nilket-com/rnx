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

