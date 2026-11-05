import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import receipts
from receipts import Run, atomic_json


class Durability(unittest.TestCase):
	def setUp(self):
		self.temp = tempfile.TemporaryDirectory()
		self.addCleanup(self.temp.cleanup)
		self.root = Path(self.temp.name)

	def test_stale_temporary_does_not_block_publication(self):
		stale = self.root / '.RESULT.json.abandoned.pending'
		stale.write_text('{')
		atomic_json(self.root / 'RESULT.json', {'status': 'PASS'})
		self.assertEqual(json.loads((self.root / 'RESULT.json').read_text()), {'status': 'PASS'})
		self.assertEqual(stale.read_text(), '{')

	def test_serialization_failure_cleans_temporary(self):
		with self.assertRaises(TypeError):
			atomic_json(self.root / 'RESULT.json', {'invalid': object()})
		self.assertEqual(list(self.root.iterdir()), [])

	def test_replace_failure_cleans_temporary_and_retains_old(self):
		path = self.root / 'RESULT.json'
		atomic_json(path, {'old': True})
		with patch.object(receipts.os, 'replace', side_effect=OSError('injected')):
			with self.assertRaises(OSError):
				atomic_json(path, {'new': True})
		self.assertEqual(json.loads(path.read_text()), {'old': True})
		self.assertEqual(list(self.root.iterdir()), [path])

	def test_interrupted_identity_temp_allows_initialization(self):
		stale = self.root / '.IDENTITY.json.abandoned.pending'
		stale.write_text('{')
		with Run(self.root, {'source': 'fixed'}):
			self.assertTrue((self.root / 'IDENTITY.json').exists())
		self.assertTrue(stale.exists())

	def test_artifact_and_directories_flushed_before_result(self):
		with Run(self.root, {'source': 'fixed'}) as run:
			attempt, _ = run.begin('subset')
			nested = attempt / 'nested'
			nested.mkdir()
			log = nested / 'stdout'
			log.write_text('test evidence')
			events = []
			fsync = os.fsync
			replace = os.replace
			def sync(fd):
				events.append(('sync', os.readlink(f'/proc/self/fd/{fd}')))
				return fsync(fd)
			def rename(src, dst):
				events.append(('rename', str(dst)))
				return replace(src, dst)
			with patch.object(receipts.os, 'fsync', side_effect=sync), patch.object(receipts.os, 'replace', side_effect=rename):
				run.finish(attempt, 'PASS', [log])
			before = events[:events.index(('rename', str(attempt / 'RESULT.json')))]
			for path in [log, nested, attempt]:
				self.assertIn(('sync', str(path)), before)
			self.assertEqual(events[-1], ('sync', str(attempt)))

	def test_failed_artifact_flush_never_publishes_result(self):
		with Run(self.root, {'source': 'fixed'}) as run:
			attempt, _ = run.begin('subset')
			log = attempt / 'stdout'
			log.write_text('evidence')
			with patch.object(receipts.os, 'fsync', side_effect=OSError('injected')):
				with self.assertRaises(OSError):
					run.finish(attempt, 'PASS', [log])
			self.assertFalse((attempt / 'RESULT.json').exists())


if __name__ == '__main__':
	unittest.main()
