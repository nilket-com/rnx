import unittest
from perf_cycles import equivalent_output, user_cycles


class PerfCycles(unittest.TestCase):
	def test_hybrid(self):
		r = user_cycles("<not counted>;;cpu_atom/cycles/u;0;0.00;;\n1331502743;;cpu_core/cycles/u;394860772;100.00;;\n")
		self.assertEqual(r["user_cycles"], 1331502743)
		self.assertEqual(r["inactive"], ["cpu_atom/cycles/u"])

	def test_generic(self):
		self.assertEqual(user_cycles("123;;cycles:u;100;100.00;;")["user_cycles"], 123)

	def test_output_only_declared_elapsed_fields_may_differ(self):
		a = "COLUMN_PREP ms=3 inputs=[I64]\nCOLUMN_OK call=0 ms=28 path=avx2; rows=100\n"
		b = a.replace("ms=3 ", "ms=6 ").replace("ms=28 ", "ms=40 ")
		self.assertTrue(equivalent_output(a, b))
		for bad in [b.replace("rows=100", "rows=101"), b.replace("path=avx2", "path=portable"), b + "extra\n", b.replace("ms=40", "ms=NaN")]:
			self.assertFalse(equivalent_output(a, bad))

	def test_missing_unsupported_multiplexed_and_duplicate(self):
		for text in ["", "<not counted>;;cpu_atom/cycles/u;0;0.00;;", "<not supported>;;cycles:u;0;0.00;;", "123;;cycles:u;100;99.00;;", "123;;cycles:u;100;100.00;;\n123;;cycles:u;100;100.00;;"]:
			with self.subTest(text=text), self.assertRaises(ValueError):
				user_cycles(text)


if __name__ == "__main__":
	unittest.main()
