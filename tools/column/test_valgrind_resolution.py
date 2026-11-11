import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from fixed_route_gate import valgrind_root


class Resolution(unittest.TestCase):
	def test_explicit_configured_and_path_roots(self):
		with tempfile.TemporaryDirectory() as temp:
			root=Path(temp);(root/'bin').mkdir();binary=root/'bin/valgrind'
			binary.write_text('#!/bin/sh\n');binary.chmod(0o755)
			with patch.dict(os.environ, {'RNX_VALGRIND_ROOT':temp}):
				self.assertEqual(valgrind_root(),root)
				with self.assertRaises(ValueError):valgrind_root(root/'missing')
			with patch.dict(os.environ, {}, clear=True), patch('shutil.which',return_value=str(binary)):
				self.assertEqual(valgrind_root(),root)
			with patch.dict(os.environ, {}, clear=True), patch('shutil.which',return_value=None):
				with self.assertRaises(ValueError):valgrind_root()


if __name__=='__main__':unittest.main()
