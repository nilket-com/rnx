"""Test report handling cannot hide a source edit or weaken an oracle result."""
import copy
import json
from pathlib import Path
import subprocess
import tempfile
import unittest

from stage_outputs import ORACLE, finish_stage, validate_report


class StageOutputs(unittest.TestCase):
	def report(self):
		return {'cases': 1, 'verified': 1, 'tally': {'match': 1}, 'results': [
			{'id': 'unique', 'path': 'unique', 'policy': 'unordered', 'unordered': True, 'status': 'match', 'detail': '1,2'}]}

	def test_unapproved_missing_or_reclassified_cases_fail(self):
		original = json.dumps(self.report()).encode()
		for key, value in [('status', 'mismatch'), ('id', 'different'), ('policy', 'changed')]:
			bad = self.report()
			bad['results'][0][key] = value
			with self.subTest(key=key), self.assertRaises(AssertionError):
				validate_report(original, json.dumps(bad).encode())
		bad = self.report()
		bad['verified'] = 0
		with self.assertRaises(AssertionError):
			validate_report(original, json.dumps(bad).encode())

	def test_only_declared_report_is_restored_and_evidence_is_retained(self):
		with tempfile.TemporaryDirectory() as directory:
			root = Path(directory)
			def git(*args):
				return subprocess.check_output(['git', *args], cwd=root, stderr=subprocess.DEVNULL)
			git('init')
			p = root / ORACLE
			p.parent.mkdir(parents=True)
			original = json.dumps(self.report()).encode()
			p.write_bytes(original)
			(root / '.gitignore').write_text('target/\n')
			(root / 'source.rs').write_text('original')
			git('add', '.')
			git('-c', 'user.name=Test', '-c', 'user.email=test@invalid', '-c', 'commit.gpgsign=false', 'commit', '-m', 'fixture')
			out = root / 'target'
			out.mkdir()
			actual = copy.deepcopy(self.report())
			actual['results'][0].update(status='row_order_differs', detail='2,1')
			actual['tally'] = {'row_order_differs': 1}
			encoded = json.dumps(actual).encode()
			p.write_bytes(encoded)
			self.assertIsNone(finish_stage(root, 'other', out))
			self.assertEqual(p.read_bytes(), encoded)
			(root / 'source.rs').write_text('changed')
			with self.assertRaises(AssertionError):
				finish_stage(root, 'polars-suite', out)
			self.assertEqual(p.read_bytes(), encoded)
			(root / 'source.rs').write_text('original')
			receipt = finish_stage(root, 'polars-suite', out)
			self.assertEqual(p.read_bytes(), original)
			self.assertEqual(Path(receipt['archive']).read_bytes(), encoded)
			self.assertEqual(git('status', '--porcelain'), b'')


if __name__ == '__main__':
	unittest.main()
