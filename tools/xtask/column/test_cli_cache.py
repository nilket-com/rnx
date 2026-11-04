"""Crash-boundary and stale-evidence controls for durable case reuse."""
import json
from pathlib import Path
import tempfile
import unittest
from cli_cache import atomic_json, commit_case, load_case, resume_index


class DurableCases(unittest.TestCase):
	def test_resume_crash_boundaries_and_invalid_evidence(self):
		with tempfile.TemporaryDirectory() as temp:
			root = Path(temp)
			case = root / "case"
			case.mkdir()
			identity = {"binary": "original"}
			record = {"kind": "i64add", "rows": 10, "layout": "flat", "role": "candidate"}
			for name in ("program.rn", "CONFIG.json", "GDB.json", "READINESS.json", "direct.stdout", "program.stdout", "map.callgrind.1", "map.callgrind.2"):
				(case / name).write_text(name)
			index = root / "INDEX.json"
			atomic_json(index, {"identity": identity, "cases": []})
			self.assertEqual(resume_index(index, identity), {}) # no commit marker
			committed = commit_case(case, record, identity)
			self.assertEqual(len(resume_index(index, identity)), 1) # crash before index update
			with self.assertRaisesRegex(ValueError, "identity"):
				load_case(committed, {"binary": "changed"})
			(case / "map.callgrind.1").write_text("corrupt")
			with self.assertRaisesRegex(ValueError, "artifact changed"):
				load_case(committed, identity)
			(case / "map.callgrind.1").write_text("map.callgrind.1")
			atomic_json(index, {"identity": identity, "cases": [committed], "failures": [{"status": "FAIL"}]})
			with self.assertRaisesRegex(ValueError, "failed cases"):
				resume_index(index, identity)
			value = json.loads(Path(committed).read_text())
			value["status"] = "FAIL"
			atomic_json(committed, value)
			with self.assertRaisesRegex(ValueError, "not PASS"):
				load_case(committed, identity)


if __name__ == "__main__":
	unittest.main()
