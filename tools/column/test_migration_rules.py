import unittest
from migration_rules import evaluate, BASELINE, CANDIDATE


def records(program=0, dynamic=(0, 0)):
	out = []
	for n, extra in zip((10000000, 100000000), dynamic):
		for role in ("baseline", "candidate"):
			p = n + (program if role == "candidate" else 0)
			d = 100 + (extra if role == "candidate" else 0)
			out.append(dict(kind="test", rows=n, layout="nullable", role=role,
				costs=[{"accounting": dict(total_Ir=p+d, rnx_Ir=p, other_Ir=0, dynamic_Ir=d)} for _ in range(2)]))
	return out


class Migration(unittest.TestCase):
	def run_rules(self, rows, failures=None):
		return evaluate(rows, {"rows": [10000000, 100000000]},
			lambda *_: {"status": "FAIL", "failures": failures or [{"rule": "strict nullable improvement"}]}, BASELINE, CANDIDATE)

	def test_tie(self):
		self.assertEqual(self.run_rules(records())["status"], "PASS")

	def test_one_program_instruction(self):
		self.assertIn("existing flat program zero growth", {f["rule"] for f in self.run_rules(records(program=1))["failures"]})

	def test_system_slope(self):
		self.assertIn("system library growth", {f["rule"] for f in self.run_rules(records(dynamic=(0, 90001)))["failures"]})

	def test_system_constant(self):
		self.assertEqual(self.run_rules(records(dynamic=(4097, 4097)))["status"], "FAIL")

	def test_ceiling_preserved(self):
		self.assertEqual(self.run_rules(records(), [{"rule": "absolute ceiling"}])["failures"], [{"rule": "absolute ceiling"}])

	def test_wrong_pair(self):
		with self.assertRaises(ValueError):
			evaluate([], {}, None, BASELINE, "wrong")

	def test_layout_proof_tampering_rejected(self):
		from migration_rules import layout_records
		from pathlib import Path
		import tempfile
		with tempfile.TemporaryDirectory() as directory:
			path = Path(directory) / "proof.json"
			path.write_text('{}')
			with self.assertRaisesRegex(ValueError, "proof pin"):
				layout_records([], path, {})
