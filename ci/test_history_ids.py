import copy
import json
from pathlib import Path
import tempfile
import subprocess
import sys
import unittest
from unittest.mock import patch
import history_ids as h


class HistoryIds(unittest.TestCase):
	def setUp(self): self.data = h.load()

	def test_old_new_and_namespace(self):
		with patch.object(h, 'git', return_value=''):
			old, new = h.resolve(self.data, 'rnx', '3fe44bf9', '.')
			self.assertEqual(new, '48fe3b15f48c92be91f04aa97f1f2f3fc31cf860')
			self.assertEqual(h.resolve(self.data, 'rnx', new, '.'), (old, new))
			for repo, value in [('rnx-bench', old), ('rnx', 'f'*40), ('rnx', 'HEAD')]:
				with self.assertRaises(ValueError): h.resolve(self.data, repo, value, '.')

	def test_unchanged_requires_graph_proof(self):
		value = 'a'*40
		with patch.object(h, 'git', return_value=value):
			self.assertEqual(h.resolve(self.data, 'rnx', value, '.'), (value, value))
		with patch.object(h, 'git', return_value=''):
			with self.assertRaises(ValueError): h.resolve(self.data, 'rnx', value, '.')
		with patch.object(h, 'git', return_value='3fe44bf9'+'a'*32):
			with self.assertRaises(ValueError): h.resolve(self.data, 'rnx', '3fe44bf9', '.')

	def test_missing_duplicate_malformed_and_corrupt(self):
		for mutation in ('missing', 'duplicate', 'malformed', 'corrupt'):
			data = copy.deepcopy(self.data); pairs = data['repositories']['rnx']['pairs']; key = next(iter(pairs))
			if mutation == 'missing': del pairs[key]
			elif mutation == 'duplicate': pairs[key] = list(pairs.values())[1]
			elif mutation == 'malformed': pairs[key] = 'wrong'
			else: pairs[key] = 'f'*40
			with tempfile.TemporaryDirectory() as tmp:
				p = Path(tmp)/'map.json'; p.write_text(json.dumps(data))
				with self.subTest(mutation=mutation), self.assertRaises(ValueError): h.load(p)
		with tempfile.TemporaryDirectory() as tmp:
			p = Path(tmp)/'map.json'; p.write_text('{"schema":1,"schema":1}')
			with self.assertRaises(ValueError): h.load(p)

	def test_map_omits_new_records(self):
		with patch.object(h, 'git', return_value=''):
			with self.assertRaises(ValueError): h.resolve(self.data, 'rnx', 'c909088d0c6933595d42a36f07dd469b5c9d78a5', '.')

	def test_path_checks_do_not_infer_blob_identity(self):
		root = Path(__file__).resolve().parents[1]
		def run(revision, path):
			return json.loads(subprocess.check_output([sys.executable, '-B', str(root/'ci/history_ids.py'), 'rnx', revision, '--repo', str(root), '--path', path]))
		# This root belongs to the preserved Rune closure; its source is present.
		revision = self.data['repositories']['rnx']['unchanged_roots'][-1]
		self.assertTrue(run(revision, 'Cargo.toml')['same_blob'])
		self.assertIsNone(run(revision, 'nonexistent-0241')['same_blob'])
		self.assertEqual(run('3fe44bf9', 'src/main.rs')['archive_tool'], 'tools/archive_data.py')
