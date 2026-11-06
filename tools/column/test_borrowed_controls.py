import unittest
from borrowed_controls import commands, positive_tests


class Controls(unittest.TestCase):
	def test_zero_test_and_failed_test_output_cannot_pass(self):
		self.assertFalse(positive_tests("test result: ok. 0 passed; 0 failed"))
		self.assertFalse(positive_tests("test result: FAILED. 1 passed; 1 failed"))
		self.assertTrue(positive_tests("test result: ok. 2 passed; 0 failed"))

	def test_format_and_clippy_precede_tests_and_scopes_are_explicit(self):
		for scope in ["core", "host", "miri", "all"]:
			stages = commands(scope)
			self.assertEqual([s[0] for s in stages[:3]], ["preflight", "format", "engine-clippy"])
			self.assertEqual(len(stages), len({s[0] for s in stages}))
			for name, command, tests in stages:
				if name.startswith("miri-"):
					self.assertIn(scope, {"miri", "all"})
					self.assertTrue(tests)
					self.assertIn("nightly-2026-09-01", " ".join(command))


if __name__ == "__main__":
	unittest.main()
