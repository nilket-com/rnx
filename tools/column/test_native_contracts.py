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

	def test_finalizer_checks_contracts_and_continues_to_the_following_stage(self):
		import gate_policy as gate
		root = Path(__file__).resolve().parents[2]
		with tempfile.TemporaryDirectory() as tmp:
			receipt = Path(tmp)/'RESULT.json'
			selection = dict(source='source', scope='complete', record={'candidate':'candidate'},
				stages=[['rune-suite',['suite']], ['later-stage',['later']]])
			evidence = {name:{} for name, _ in selection['stages']}
			for missing in (False, True):
				cases = self.rows[:-1] if missing else self.rows
				receipt.write_text(json.dumps({'contracts':dict(manifest_sha256=child.digest(root/contracts.MANIFEST), cases=cases)}))
				rows = [dict(command=command, context=dict(command=command, kind='producer', candidate='candidate'),
					producer={'receipt':str(receipt)}) for _, command in selection['stages']]
				with patch.object(gate, 'check_selection'), patch.object(gate.child, 'evidence', side_effect=rows) as read:
					if missing:
						with self.assertRaisesRegex(ValueError, 'catalogue differs'): gate.finalize(selection, evidence, root)
					else:
						result = gate.finalize(selection, evidence, root)
						self.assertEqual(result['status'], 'PASS')
						self.assertEqual(read.call_count, 2)
						self.assertIn('later-stage', result['durations'])

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
