import unittest
from simd_hot_loop import assess, EXPECTED


def fixture(extra=0, stride='$0x8,%rax'):
	shapes = {}
	for name, (count, length) in EXPECTED.items():
		rows = []
		for _ in range(count):
			start = len(rows)
			rows += [('vmovdqu', '%ymm0,(%rdi)'), ('vmovdqu', '%ymm1,0x20(%rdi)')]
			rows += [('vpor', '%ymm0,%ymm1,%ymm0')] * (length - 4 + extra)
			rows += [('add', stride), ('jne', 'local:' + str(start))]
		shapes[name] = rows
	return shapes


class HotLoopTests(unittest.TestCase):
	def test_exact(self):
		self.assertTrue(all(x['status'] == 'PASS' for x in assess(fixture()).values()))

	def test_extra_instruction(self):
		self.assertTrue(all(x['status'] == 'FAIL' for x in assess(fixture(1)).values()))

	def test_wrong_lane_stride(self):
		self.assertTrue(all(x['status'] == 'FAIL' for x in assess(fixture(stride='$0x4,%rax')).values()))

	def test_missing_loop(self):
		x = fixture(); x['fused'] = []
		self.assertEqual(assess(x)['fused']['status'], 'FAIL')


if __name__ == '__main__':
	unittest.main()
