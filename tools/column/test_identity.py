"""Production identity needs matching nonempty sections and the paired recipe."""
import copy
import unittest
from identity import validate_pair_metadata


class Identity(unittest.TestCase):
	def pair(self):
		base = {'role': 'baseline', 'source_commit': 'old', 'command': ['cargo', '+1.98.1', 'build', '--locked', '--release', '--manifest-path', 'adapters/polars/Cargo.toml', '--bin', 'rnx-polars'], 'sha256': 'file-a', 'sections': {'.text': {'size': 10, 'sha256': 'text'}, '.rodata': {'size': 20, 'sha256': 'data'}}}
		return {'status': 'PASS', 'builds': [base, dict(copy.deepcopy(base), role='candidate', source_commit='new', sha256='file-b')]}

	def test_section_identity_does_not_require_whole_file_identity(self):
		validate_pair_metadata(self.pair(), 'old')
		for section in ('.text', '.rodata'):
			bad = self.pair()
			bad['builds'][1]['sections'][section]['sha256'] = 'changed'
			with self.assertRaises(AssertionError):
				validate_pair_metadata(bad, 'old')
		bad = self.pair()
		bad['builds'][1]['command'].append('--features=other')
		with self.assertRaises(AssertionError):
			validate_pair_metadata(bad, 'old')
		with self.assertRaises(AssertionError):
			validate_pair_metadata(self.pair(), 'wrong-baseline')


if __name__ == '__main__':
	unittest.main()
