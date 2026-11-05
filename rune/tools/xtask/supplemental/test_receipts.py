import json
from pathlib import Path
import tempfile
import unittest
from receipts import Run


class Receipts(unittest.TestCase):
	def setUp(self):
		self.temp = tempfile.TemporaryDirectory()
		self.addCleanup(self.temp.cleanup)
		self.root = Path(self.temp.name) / 'run'
		self.identity = {'source': 'exact', 'toolchain': 'pinned'}

	def finish(self, run, name='subset', status='PASS'):
		attempt, cached = run.begin(name)
		self.assertIsNone(cached)
		log = attempt / 'stdout'
		log.write_text('verified test result\n')
		run.finish(attempt, status, [log])
		return attempt

	def test_explicit_resume_and_rehash(self):
		with Run(self.root, self.identity) as run:
			attempt = self.finish(run)
		with self.assertRaises(ValueError), Run(self.root, self.identity):
			pass
		with Run(self.root, self.identity, resume=True) as run:
			self.assertIsNone(run.begin('subset')[0])
		(attempt / 'stdout').write_text('corrupted')
		with Run(self.root, self.identity, resume=True) as run:
			with self.assertRaises(ValueError):
				run.begin('subset')

	def test_failure_persists(self):
		with Run(self.root, self.identity) as run:
			self.finish(run, status='FAIL')
		with Run(self.root, self.identity, resume=True) as run:
			with self.assertRaises(ValueError):
				run.begin('subset')

	def test_interruption_keeps_attempt_and_reuses_success(self):
		with Run(self.root, self.identity) as run:
			self.finish(run, 'done')
			old, _ = run.begin('interrupted')
			(old / 'stdout').write_text('partial')
		with Run(self.root, self.identity, resume=True) as run:
			self.assertIsNone(run.begin('done')[0])
			new, _ = run.begin('interrupted')
			self.assertNotEqual(old, new)
			self.assertEqual((old / 'stdout').read_text(), 'partial')

	def test_identity_refusal(self):
		with Run(self.root, self.identity):
			pass
		with self.assertRaises(ValueError), Run(self.root, {'source': 'different'}, resume=True):
			pass

	def test_exclusive_lock(self):
		with Run(self.root, self.identity):
			with self.assertRaises(BlockingIOError), Run(self.root, self.identity, resume=True):
				pass

	def test_terminal_immutable(self):
		with Run(self.root, self.identity) as run:
			attempt = self.finish(run)
			with self.assertRaises(ValueError):
				run.finish(attempt, 'FAIL', [attempt / 'stdout'])
			self.assertFalse((attempt / 'RESULT.json.pending').exists())
			self.assertEqual(json.loads((attempt / 'RESULT.json').read_text())['status'], 'PASS')


if __name__ == '__main__':
	unittest.main()
