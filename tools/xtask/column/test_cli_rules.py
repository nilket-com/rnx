"""Fail-closed controls for production map capture."""
import json
from pathlib import Path
import unittest
from unittest.mock import Mock, patch
import tempfile
import cli_rules
from cli_capture import validate, case_readiness
import measurement_readiness
from cli_fixtures import valid_count


class CaptureControls(unittest.TestCase):
	def test_performance_failures(self):
		policy = json.loads((Path(__file__).parent / "CK2.json").read_text())
		self.assertEqual(cli_rules.controls(policy), 10)

	def test_rejected_readiness_preserves_exact_observation(self):
		observed = {"load": 2.1, "busy": [], "mem_available_bytes": 10000}
		with self.assertRaisesRegex(ValueError, "not ready") as context:
			measurement_readiness.require_ready(observed, max_load=2, min_available_bytes=100)
		self.assertIs(context.exception.receipt["observation"], observed)
		self.assertEqual(context.exception.receipt["status"], "NOT_READY")
		self.assertEqual(context.exception.receipt["limits"], {"max_load": 2, "min_available_bytes": 100})

	def test_fixed_cooldown_one_rejected_sample_no_retry(self):
		observed = {"load": 2.28, "busy": [], "mem_available_bytes": 50 * 1024**3}
		error = ValueError("measurement host is not ready")
		error.receipt = {"status": "NOT_READY", "observation": observed}
		lease = Mock()
		lease.ready.side_effect = error
		with tempfile.TemporaryDirectory() as temp:
			path = Path(temp) / "READINESS.json"
			with patch("cli_capture.time.sleep") as sleep, patch("cli_capture.time.time", side_effect=[1000, 1120]), patch("cli_capture.time.monotonic", side_effect=[10, 130]):
				with self.assertRaisesRegex(ValueError, "not ready"):
					case_readiness(lease, path)
				sleep.assert_called_once_with(120)
			lease.ready.assert_called_once_with(max_load=2, min_available_bytes=8 * 1024**3)
			receipt = json.loads(path.read_text())
			self.assertEqual(receipt["observation"], observed)
			self.assertEqual(receipt["cooldown"], {"seconds": 120, "start_unix": 1000, "end_unix": 1120, "start_monotonic": 10, "end_monotonic": 130})
			self.assertEqual(receipt["status"], "NOT_READY")

	def test_candidate_pinned_exception_rejects_29_and_other_cases(self):
		policy = json.loads((Path(__file__).parent / "CK2.json").read_text())
		entry = policy["reviewed_exceptions"][0]
		provenance = {k: entry[k] for k in ("candidate", "baseline", "attribution_sha256")}
		def check(delta, context=provenance, kind="boolnot", rows=100000000, call=0):
			return cli_rules.reviewed_exception(policy, context, kind, "flat", rows, call, delta)
		self.assertIsNotNone(check(28))
		self.assertIsNone(check(29))
		self.assertIsNone(check(28, rows=10000000))
		self.assertIsNone(check(28, call=1))
		self.assertIsNone(check(28, kind="i64add"))
		for key in provenance:
			self.assertIsNone(check(28, context=dict(provenance, **{key: "changed"})))
		self.assertIsNone(check(28, context=None))

	def test_output_witness(self):
		valid = valid_count("i64add", 4096, "alternating")
		line = f"COLUMN_OK completed=true; rows=4096; valid_rows={valid}; attempts=1; replays=0; refusal=None; path=avx2;"
		validate(line + "\n" + line, "i64add", 4096, "alternating")
		for bad in [line, (line + "\n" + line).replace("attempts=1", "attempts=0"), (line + "\n" + line).replace("rows=4096", "rows=4095"), (line + "\n" + line).replace("path=avx2", "path=unknown")]:
			with self.assertRaises(AssertionError):
				validate(bad, "i64add", 4096, "alternating")


if __name__ == "__main__":
	unittest.main()
