import subprocess
import tempfile
import unittest

import stage_fakes
from pathlib import Path

import final


class DocsChange(unittest.TestCase):
	def setUp(self):
		self.tmp = tempfile.TemporaryDirectory(); self.root = Path(self.tmp.name)
		self.git('init', '-q'); self.git('config', 'user.email', 'a@b'); self.git('config', 'user.name', 'a')
		self.write('src/lib.rs', 'a'); self.write('plans/adr/0001_x.md', 'one'); self.commit('base')
		self.base = self.head()
		self.write('src/lib.rs', 'b'); self.write('plans/adr/0001_x.md', 'two'); self.commit('impl')
		self.passed = self.head()

	def tearDown(self): self.tmp.cleanup()
	def git(self, *a): return subprocess.check_output(['git', *a], cwd=self.root, text=True).strip()
	def write(self, name, text):
		p = self.root/name; p.parent.mkdir(parents=True, exist_ok=True); p.write_text(text)
	def commit(self, message): self.git('add', '-A'); self.git('commit', '-q', '-m', message)
	def head(self): return self.git('rev-parse', 'HEAD')
	def amend(self, name, text):
		self.git('checkout', '-q', self.passed); self.write(name, text)
		self.git('add', '-A'); self.git('commit', '-q', '--amend', '-m', 'impl'); return self.head()

	def test_plans_only_amend_is_a_docs_change(self):
		amended = self.amend('plans/adr/0001_x.md', 'three')
		self.assertEqual(final.docs_change(self.root, self.passed, amended), ['plans/adr/0001_x.md'])

	def test_amend_outside_plans_is_refused(self):
		amended = self.amend('src/lib.rs', 'c')
		with self.assertRaisesRegex(ValueError, 'plans/ only'): final.docs_change(self.root, self.passed, amended)

	def test_plans_named_file_outside_plans_is_refused(self):
		amended = self.amend('src/plans/notes.md', 'x')
		with self.assertRaisesRegex(ValueError, 'plans/ only'): final.docs_change(self.root, self.passed, amended)

	def test_different_parent_is_refused(self):
		self.git('checkout', '-q', self.passed); self.write('plans/adr/0001_x.md', 'later'); self.commit('next')
		with self.assertRaisesRegex(ValueError, 'same parent'): final.docs_change(self.root, self.passed, self.head())

	def test_source_moved_into_plans_is_refused(self):
		self.git('checkout', '-q', self.passed); self.git('mv', 'src/lib.rs', 'plans/lib.rs')
		self.git('commit', '-q', '--amend', '-m', 'impl')
		with self.assertRaisesRegex(ValueError, 'src/lib.rs'): final.docs_change(self.root, self.passed, self.head())

	def test_plan_moved_out_of_plans_is_refused(self):
		self.git('checkout', '-q', self.passed); self.git('mv', 'plans/adr/0001_x.md', 'src/0001_x.md')
		self.git('commit', '-q', '--amend', '-m', 'impl')
		with self.assertRaisesRegex(ValueError, 'src/0001_x.md'): final.docs_change(self.root, self.passed, self.head())

	def test_successor_runs_only_the_docs_selection(self):
		# The record changed src/lib.rs, so its required selection is not
		# just commit discipline; the successor must not reselect it.
		from unittest.mock import patch
		amended = self.amend('plans/adr/0001_x.md', 'three')
		prior = self.root.parent/(self.root.name + '-prior'); prior.mkdir()
		(prior/'RESULT.json').write_text('{}')
		calls = []
		def docs_run(root, out, names=None, **kw):
			calls.append((names, kw)); out.mkdir(parents=True); (out/'RESULT.json').write_text('{}')
			return dict(status='PASS')
		earlier = dict(complete_final=True, publication_eligible=True, candidate=self.passed)
		try:
			with patch.object(final, 'revalidate', return_value=earlier), patch.object(final, 'run', side_effect=docs_run):
				r = final.successor(self.root, self.root.parent/(self.root.name + '-successor'), prior)
		finally:
			import shutil
			shutil.rmtree(prior); shutil.rmtree(self.root.parent/(self.root.name + '-successor'), ignore_errors=True)
		self.assertEqual(r['status'], 'PASS'); self.assertEqual(r['candidate'], amended)
		self.assertEqual(calls, [(None, {'scope': 'docs'})])

	def test_successor_prints_its_own_receipt(self):
		import contextlib, io, sys
		from unittest.mock import patch
		out = self.root/'successor'
		for argv, receipt in ((['--docs-successor', str(self.root/'prior')], 'SUCCESSOR.json'), ([], 'RESULT.json')):
			printed = io.StringIO()
			with self.subTest(receipt=receipt), patch.object(sys, 'argv', ['final.py', '--out', str(out), *argv]), \
					patch.object(final, 'successor', return_value=dict(status='PASS')), \
					patch.object(final, 'run', return_value=dict(status='PASS')), patch.object(final.signal, 'signal'), \
					contextlib.redirect_stdout(printed):
				final.main()
			self.assertTrue(printed.getvalue().strip().endswith(str(out/receipt)), printed.getvalue())

	def test_identical_commit_is_not_a_docs_change(self):
		with self.assertRaisesRegex(ValueError, 'plans/ only'): final.docs_change(self.root, self.passed, self.passed)


class WallBudget(unittest.TestCase):
	def test_complete_final_over_an_hour_fails(self):
		self.assertRegex(final.wall_budget_error('complete', 61*60), 'took 61.0 minutes; its budget is 60')
		self.assertIsNone(final.wall_budget_error('complete', 59*60))

	def complete_run(self, release_seconds):
		"""A complete final whose one stage ends at 59 minutes."""
		import child_result as child
		import final_evidence
		from types import SimpleNamespace
		from unittest.mock import patch
		clock = [0.0]
		def execute(command, **kw):
			clock[0] = 59*60
			d = kw['directory']; d.mkdir()
			child.atomic_json(d/'RESULT.json', dict(status='PASS'))
			return SimpleNamespace(returncode=0, validation={'path': str(d/'RESULT.json'), 'sha256': child.digest(d/'RESULT.json')})
		def release(*a, **kw):
			clock[0] += release_seconds
			return dict(status='PASS')
		selection = dict(scope='complete', record={'candidate': 'c'}, source='s', policy='p', stages=[['only', ['command']]])
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp)
			with patch.object(final.time, 'monotonic', side_effect=lambda: clock[0]), patch.object(final, 'disk_check'), \
					patch.object(final, 'tools_identity', return_value={}), patch.object(final.child, 'head', return_value='c'), \
					patch.object(final.gate, 'select', return_value=selection), patch.object(final.gate, 'check_selection'), \
					patch.object(final.gate, 'finalize', return_value={'status': 'PASS'}), \
					patch.object(final.child, 'run', side_effect=execute), patch.object(final, 'terminal', side_effect=release), \
					patch.object(final_evidence.Owners, 'retire'), stage_fakes.inline(final):
				return final.run(root, root/'final', scope='complete')

	def test_release_time_counts_toward_the_budget(self):
		quick = self.complete_run(0)
		self.assertEqual(quick['status'], 'PASS', quick)
		self.assertTrue(quick['complete_final'])
		slow = self.complete_run(120)
		self.assertEqual(slow['status'], 'FAIL')
		self.assertFalse(slow['complete_final']); self.assertFalse(slow['publication_eligible'])
		self.assertRegex(slow['wall_budget_error'], 'took 61.0 minutes')
		import failure_detail
		self.assertRegex(failure_detail.projection(slow, {})['error'], 'took 61.0 minutes')

	def test_narrower_finals_have_no_budget(self):
		for scope in ('required', 'explicit'):
			self.assertIsNone(final.wall_budget_error(scope, 10*3600))
