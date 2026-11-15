"""Keep the cost fixture compatible with rnx run's one-argument entry ABI."""
from pathlib import Path
import re
import unittest

from json_parse_cost import program


class EntryAbi(unittest.TestCase):
	def test_generated_file_main_accepts_the_argument_vector(self):
		source = program(Path('fixture.json'))
		declarations = re.findall(r'\bpub\s+fn\s+main\s*\(([^()]*)\)', source)
		self.assertEqual(len(declarations), 1)
		self.assertEqual(len([p for p in declarations[0].split(',') if p.strip()]), 1)
		self.assertIsNone(re.search(r'\bmain\s*\(\s*\)', source))


if __name__ == '__main__':
	unittest.main()
