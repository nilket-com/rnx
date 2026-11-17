import copy
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import child_result as child
import native_contracts as contracts


class Coverage(unittest.TestCase):
	def setUp(self):
		self.m = contracts.manifest(Path(__file__).resolve().parents[2])
		self.rows = [dict(name=c['name'], specification=child.identity(c), status='PASS', exit=0,
			selected=[c['selector']], passed=1, contract_rows=c.get('rows')) for c in self.m['cases']]

	def test_missing_manifest_contract_fails_the_final_catalogue(self):
		# Both ignored and nonignored cases come from the manifest. Removing an
		# ignored case from otherwise passing evidence must reject the final.
		self.assertEqual(contracts.catalogue(self.m, self.rows)['cases'], len(self.m['cases']))
		for ignored in (True, False):
			removed = next(c for c in self.m['cases'] if c['ignored'] == ignored)
			with self.subTest(removed=removed['name']), self.assertRaisesRegex(ValueError, 'catalogue differs'):
				contracts.catalogue(self.m, [r for r in self.rows if r['name'] != removed['name']])
		# A newly declared contract is required without editing a runner list.
		added = copy.deepcopy(self.m)
		added['cases'].append(dict(self.m['cases'][0], name='planted-contract', selector='planted::contract'))
		with self.assertRaisesRegex(ValueError, 'planted-contract'):
			contracts.catalogue(added, self.rows)

	def test_partial_failed_or_differently_selected_execution_cannot_pass(self):
		for mutation in ('exit', 'status', 'selected', 'passed', 'contract_rows', 'specification'):
			rows = copy.deepcopy(self.rows)
			rows[0][mutation] = {'exit': 101, 'status': 'FAIL', 'selected': ['other'], 'passed': 0,
				'contract_rows': self.rows[0]['contract_rows']+1, 'specification': 'different'}[mutation]
			with self.subTest(mutation=mutation), self.assertRaises(ValueError): contracts.catalogue(self.m, rows)

	def test_manifest_case_keeps_ignored_policy_and_declared_fixture_environment(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); binary = root/'binary'; binary.write_bytes(b'fixture')
			receipt = root/'binary.json'; receipt.write_text(json.dumps({'binary_sha256': child.digest(binary)}))
			case = dict(name='fixture', selector='fixture::contract', exact=True, ignored=True,
				timeout=17, rows=2, sandbox=False, empty_stderr=True, env={'GAP_COMPARE_ENGINE':'fresh'})
			calls = []
			def execute(command, **kw):
				calls.append((command, kw))
				if '--list' in command:
					kw['stdout'].write('fixture::contract: test\n')
				else:
					kw['stdout'].write('CONTRACT ROWS 2\ntest result: ok. 1 passed; 0 failed;\n')
					child.atomic_json(Path(kw['env']['RNX_CONTRACT_DIFFS']), dict(rows_checked=2, differences=[]))
				return __import__('subprocess').CompletedProcess(command, 0)
			with patch.dict(os.environ, {'PLANTED_SECRET':'do-not-copy'}, clear=True), patch.object(contracts, 'run_process', side_effect=execute):
				r = contracts.run_case(case, binary, receipt, root, root)
			self.assertEqual(r['status'], 'PASS')
			self.assertEqual(len(calls), 2)
			for command, _ in calls:
				self.assertIn('--ignored', command); self.assertIn('--exact', command)
			self.assertEqual(calls[1][1]['env']['GAP_COMPARE_ENGINE'], 'fresh')
			self.assertNotIn('PLANTED_SECRET', calls[1][1]['env'])
			self.assertEqual(calls[1][1]['timeout'], 17)


if __name__ == '__main__': unittest.main()


def _timed_case(c, binary, receipt, root, bulk):
	import time
	started = time.monotonic(); time.sleep(0.3)
	row = dict(name=c['name'], status='FAIL' if c['name'] == 'broken' else 'PASS',
		started=started, ended=time.monotonic(), pid=os.getpid())
	return row


class Concurrency(unittest.TestCase):
	def setUp(self):
		# Fake cases never lease a real final's jobserver they happen to inherit.
		ambient = patch.dict(os.environ)
		ambient.start(); self.addCleanup(ambient.stop)
		for key in ('MAKEFLAGS', 'CARGO_MAKEFLAGS', 'RNX_FINAL_STAGE_TOKENS'): os.environ.pop(key, None)

	def cases(self):
		names = ['a', 'alone', 'b', 'broken', 'c', 'd']
		return [dict(name=n, exclusive=n == 'alone') for n in names]

	def run_all(self, width):
		with patch.object(contracts, 'run_case', _timed_case), patch('host_budget.case_width', return_value=width):
			return contracts.run_cases(self.cases(), None, None, None, None)

	def test_exclusive_case_runs_alone_and_the_rest_overlap(self):
		rows = self.run_all(4)
		self.assertEqual([r['name'] for r in rows], [c['name'] for c in self.cases()])
		by = {r['name']: r for r in rows}
		alone = by.pop('alone')
		for other in by.values():
			self.assertTrue(other['started'] >= alone['ended'] or other['ended'] <= alone['started'], other['name'])
		shared = sorted(by.values(), key=lambda r: r['started'])
		self.assertTrue(any(b['started'] < a['ended'] for a, b in zip(shared, shared[1:])), 'no shared cases overlapped')
		self.assertGreater(len({r['pid'] for r in shared}), 1)

	def test_every_case_runs_when_one_fails(self):
		rows = self.run_all(2)
		self.assertEqual({r['name']: r['status'] for r in rows}['broken'], 'FAIL')
		self.assertEqual(sum(r['status'] == 'PASS' for r in rows), 5)
