import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parent))
import append_diagnostic


def rows(cold=0, per_call=0):
    return [dict(invocations=n, role=role, native='off', checksum=str(n),
                 profile=dict(functions={'rune::experimental_core::heap::Store::push':
                     dict(self_Ir=1000 * n + (cold + per_call * n if role == 'candidate' else 0))}))
            for n in (1000, 2000) for role in ('tip', 'candidate')]


class AppendDiagnostic(unittest.TestCase):
    def test_constant_setup_is_separate_and_profiles_stay_unchanged(self):
        evidence = rows(cold=41)
        before = copy.deepcopy(evidence)
        result = append_diagnostic.assess(evidence)
        self.assertEqual(result['status'], 'PASS')
        self.assertEqual(result['incremental_growth_per_extra_1000'], 0)
        self.assertEqual(evidence, before)

    def test_one_instruction_per_call_is_detected(self):
        for cost in (-1, 1):
            result = append_diagnostic.assess(rows(per_call=cost))
            self.assertEqual(result['status'], 'FAIL')
            self.assertEqual(result['instructions_per_extra_call'], cost)

    def test_missing_duplicate_or_native_enabled_is_rejected(self):
        for mutation in ('missing', 'duplicate', 'native'):
            evidence = rows()
            if mutation == 'missing':
                evidence.pop()
            elif mutation == 'duplicate':
                evidence.append(copy.deepcopy(evidence[0]))
            else:
                evidence[0]['native'] = 'on'
            with self.assertRaises(ValueError):
                append_diagnostic.assess(evidence)

    def test_checksum_mismatch_is_rejected(self):
        evidence = rows()
        evidence[0]['checksum'] = 'wrong'
        with self.assertRaises(ValueError):
            append_diagnostic.assess(evidence)


if __name__ == '__main__':
    unittest.main()
