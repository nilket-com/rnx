"""Negative controls for evidence that must never count as a timed execution."""
import importlib.util
from pathlib import Path
import subprocess
import unittest

spec = importlib.util.spec_from_file_location('routing_bench', Path(__file__).with_name('run.py'))
bench = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bench)


class ProtocolTests(unittest.TestCase):
	def test_fallback_cannot_be_reported_as_execution_in_either_native_mode(self):
		for mode in ('execution-off', 'execution-on'):
			for stderr in (b'rnx: engine=legacy refusal=unsupported\n', b'',
						   b'rnx: engine=execution native-requested=on\nrnx: engine=legacy\n'):
				with self.subTest(mode=mode, stderr=stderr):
					with self.assertRaises(ValueError):
						bench.validate_child(mode, subprocess.CompletedProcess([], 0, b'7\n', stderr), True, 'route')

	def test_route_must_prove_the_requested_native_mode_and_no_extra_diagnostics(self):
		for mode, native in [('execution-off', 'off'), ('execution-on', 'on')]:
			good = f'rnx: engine=execution native-requested={native}\n'.encode()
			bench.validate_child(mode, subprocess.CompletedProcess([], 0, b'7\n', good), True, 'route')
			for stderr in (good + b'warning\n', good.replace(native.encode(), b'unknown')):
				with self.assertRaises(ValueError):
					bench.validate_child(mode, subprocess.CompletedProcess([], 0, b'7\n', stderr), True, 'route')

	def test_nonzero_timeout_signal_or_diagnostics_are_not_measurements(self):
		for mode in bench.MODES:
			for code, stderr in [(1, b''), (124, b''), (137, b''), (-9, b''), (0, b'warning\n')]:
				with self.assertRaises(ValueError):
					bench.validate_child(mode, subprocess.CompletedProcess([], code, b'7\n', stderr), False, 'capture')

	def test_checksum_comparison_is_live_exact_and_requires_integer_output(self):
		expected = bench.compare_checksum(None, b'123\n', 'first')
		self.assertEqual(bench.compare_checksum(expected, b'123\n', 'second'), expected)
		for observed in (b'124\n', b'123', b' 123\n', b'123\nextra\n', b'1.23e2\n', b'NaN\n'):
			with self.assertRaises(ValueError):
				bench.compare_checksum(expected, observed, 'bad')

	def test_short_or_startup_dominated_samples_fail_independently_of_median(self):
		bench.validate_capture(1.0, 0.049, 'good')
		for wall, startup in [(0.49, 0.001), (1.0, 0.05), (1.0, 0.9), (float('nan'), 0.01), (1.0, -0.01)]:
			with self.assertRaises(ValueError):
				bench.validate_capture(wall, startup, 'bad')

	def test_round_domain_is_checked_before_generating_sources(self):
		for rounds in (-1, 2**31 + 1):
			with self.assertRaises(ValueError):
				bench.generated('fib', rounds, Path('/does-not-exist'))


if __name__ == '__main__':
	unittest.main()
