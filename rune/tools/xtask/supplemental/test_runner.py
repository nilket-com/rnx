import copy
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import run as runner
from receipts import Run
from runtime import check_tests, overlap


class Runner(unittest.TestCase):
	def test_failed_prerequisite_releases_no_preparation(self):
		with patch.object(runner, 'stage', side_effect=ValueError('format failed')) as stage:
			with self.assertRaises(ValueError):
				runner.run_jobs(None, None, {}, {}, 2)
			self.assertEqual(stage.call_count, 1)
			self.assertEqual(stage.call_args.args[1], 'prerequisites')

	def test_live_inventory_changes_fail_before_workers(self):
		manifest = json.loads((Path(__file__).parent / 'manifest.json').read_text())
		prepared = {feature: {'inventory': []} for feature in manifest['features']}
		with self.assertRaises(ValueError):
			runner.selection(manifest, prepared)

	def test_observed_coverage_rejects_omission_duplicate_and_failure(self):
		for text in ['test a ... ok\ntest result: ok.', 'test a ... ok\ntest a ... ok\ntest result: ok.', 'test a ... FAILED\ntest b ... ignored\ntest result: FAILED.']:
			with self.assertRaises(ValueError):
				check_tests(text, ['a', 'b'])
		self.assertEqual(check_tests('test a ... ok\ntest b ... ignored, Miri\ntest result: ok.', ['a', 'b']), ['b'])

	def test_interpreter_overlap_requires_intersection(self):
		self.assertEqual(overlap([{'intervals': [{'start': 1, 'end': 2}]}, {'intervals': [{'start': 2, 'end': 3}]}]), 1)
		self.assertEqual(overlap([{'intervals': [{'start': 1, 'end': 3}]}, {'intervals': [{'start': 2, 'end': 4}]}]), 2)

	def test_actual_kill_and_explicit_resume(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp) / 'receipts'
			script = '''
import sys, time
from pathlib import Path
from receipts import Run
with Run(Path(sys.argv[1]), {'source': 'fixed'}) as r:
 a,_=r.begin('done'); log=a/'stdout'; log.write_text('ok'); r.finish(a,'PASS',[log])
 a,_=r.begin('pending'); (a/'stdout').write_text('partial')
 print('READY',flush=True)
 time.sleep(60)
'''
			child = subprocess.Popen([sys.executable, '-c', script, str(root)], cwd=Path(__file__).parent, stdout=subprocess.PIPE, text=True)
			try:
				self.assertEqual(child.stdout.readline().strip(), 'READY')
				child.kill()
				child.wait(timeout=5)
			finally:
				if child.poll() is None:
					child.kill()
					child.wait()
				child.stdout.close()
			with Run(root, {'source': 'fixed'}, resume=True) as r:
				self.assertIsNone(r.begin('done')[0])
				attempt, _ = r.begin('pending')
				self.assertEqual(attempt.name, 'attempt-0002')
				self.assertEqual((root / 'pending/attempt-0001/stdout').read_text(), 'partial')

	def test_incremental_cache_is_disabled_in_child_environment(self):
		self.assertEqual(runner.environment()['CARGO_INCREMENTAL'], '0')

	def test_captured_incremental_path_fails_isolation(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp)
			path = root / 'miri/host/debug/build/rune/abc/out/rune-abc'
			path.parent.mkdir(parents=True)
			path.write_text(json.dumps({'args': ['--test', '-C', 'incremental=/shared/cache']}))
			with self.assertRaises(ValueError):
				runner.verify_harness_isolation(root)
			path.write_text(json.dumps({'args': ['--test']}))
			self.assertEqual(runner.verify_harness_isolation(root), [str(path)])

	def test_scope_default_and_explicit_short(self):
		jobs = [{'subset': name} for name in ['n5', 'budget', 'payload_bit_scan', 'column']]
		self.assertEqual(runner.scoped_jobs(jobs, 'full'), jobs)
		self.assertEqual(runner.scoped_jobs(jobs, 'tooling-short'), jobs[1:3])
		with self.assertRaises(ValueError):
			runner.scoped_jobs(jobs[:2], 'tooling-short')

	def test_scope_identity_cannot_reuse_full_or_short(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp)
			with Run(root, {'source': 'same', 'scope': 'tooling-short'}):
				pass
			with self.assertRaises(ValueError), Run(root, {'source': 'same', 'scope': 'full'}, resume=True):
				pass


if __name__ == '__main__':
	unittest.main()
