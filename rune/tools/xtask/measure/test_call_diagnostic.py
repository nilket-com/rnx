import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parent))
import call_diagnostic


def rows(cold=0, per_call=0):
    return [dict(invocations=n, role=role, native='on', native_entries=0,
                 kernel='interpreter_call', checksum=str(n),
                 profile=dict(functions={'rune::experimental_core::execute::Executor::push_call':
                     dict(self_Ir=1000 * n + (cold + per_call * n if role == 'candidate' else 0))}))
            for n in (1000, 2000) for role in ('tip', 'candidate')]


class CallDiagnostic(unittest.TestCase):
    def test_fixed_setup_is_separate(self):
        evidence = rows(cold=41)
        before = copy.deepcopy(evidence)
        result = call_diagnostic.assess(evidence)
        self.assertEqual(result['status'], 'PASS')
        self.assertEqual(result['cold_setup_delta'], 41)
        self.assertEqual(evidence, before)

    def test_nonpositive_passes_and_one_added_instruction_fails(self):
        for cost in (-1, 0, 1):
            result = call_diagnostic.assess(rows(per_call=cost))
            self.assertEqual(result['status'], 'FAIL' if cost > 0 else 'PASS')
            self.assertEqual(result['instructions_per_extra_call'], cost)

    def test_missing_duplicate_wrong_mode_or_native_entry_is_rejected(self):
        for mutation in ('missing', 'duplicate', 'mode', 'entries', 'kernel'):
            evidence = rows()
            if mutation == 'missing': evidence.pop()
            elif mutation == 'duplicate': evidence.append(copy.deepcopy(evidence[0]))
            elif mutation == 'mode': evidence[0]['native'] = 'off'
            elif mutation == 'entries': evidence[0]['native_entries'] = 1
            else: evidence[0]['kernel'] = 'fib'
            with self.assertRaises(ValueError): call_diagnostic.assess(evidence)

    def test_checksum_mismatch_is_rejected(self):
        evidence = rows()
        evidence[0]['checksum'] = 'wrong'
        with self.assertRaises(ValueError): call_diagnostic.assess(evidence)


if __name__ == '__main__':
    unittest.main()
