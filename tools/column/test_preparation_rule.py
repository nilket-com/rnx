import copy
import unittest
import preparation_rule as rule


class PreparationRule(unittest.TestCase):
	def samples(self):
		return [dict(kind="multiply3", layout="flat", rows=n, call=c,
			baseline=dict(program_Ir=1000, functions={}),
			candidate=dict(program_Ir=1048, functions=dict(zip(rule.FUNCTIONS, rule.VECTORS["multiply3"], strict=True))))
			for n in (4096, 10000000, 100000000) for c in (0, 1)]

	def test_reviewed_constant(self):
		self.assertEqual(rule.check(self.samples(), rule.BASE, rule.CANDIDATE)["status"], "PASS")

	def test_57_fails(self):
		samples = self.samples()
		for s in samples:
			s["candidate"]["functions"][rule.FUNCTIONS[0]] = 1061
		with self.assertRaisesRegex(ValueError, "exceeds reviewed cap"):
			rule.check(samples, rule.BASE, rule.CANDIDATE)

	def test_slope_and_unattributed_fail(self):
		samples = self.samples()
		samples[0]["candidate"]["functions"][rule.FUNCTIONS[0]] = 1059
		with self.assertRaisesRegex(ValueError, "size/call-dependent"):
			rule.check(samples, rule.BASE, rule.CANDIDATE)
		samples = self.samples()
		samples[0]["candidate"]["program_Ir"] = 1057
		with self.assertRaisesRegex(ValueError, "unattributed"):
			rule.check(samples, rule.BASE, rule.CANDIDATE)

	def test_scope_coverage_pair(self):
		for change in ("pair", "path", "coverage"):
			samples = copy.deepcopy(self.samples())
			if change == "path":
				samples[0]["layout"] = "unreviewed"
			if change == "coverage":
				samples.pop()
			with self.assertRaises(ValueError):
				rule.check(samples, rule.BASE, "wrong" if change == "pair" else rule.CANDIDATE)

	def test_each_kernel_exact_cap_and_plus_one(self):
		for kind, vector in rule.VECTORS.items():
			samples = self.samples()
			for s in samples:
				s["kind"] = kind
				s["candidate"]["functions"] = dict(zip(rule.FUNCTIONS, vector, strict=True))
				s["candidate"]["program_Ir"] = 1000 + min(0, sum(vector))
			self.assertEqual(rule.check(samples, rule.BASE, rule.CANDIDATE)["status"], "PASS")
			for s in samples:
				s["candidate"]["functions"][rule.FUNCTIONS[0]] += 1
			with self.assertRaisesRegex(ValueError, "exceeds reviewed cap"):
				rule.check(samples, rule.BASE, rule.CANDIDATE)
