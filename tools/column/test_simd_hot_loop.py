import unittest
from simd_hot_loop import assess, EXPECTED


def fixture(extra=0, stride='$0x8,%rax'):
	shapes = {}
	for name, (count, length) in EXPECTED.items():
		rows = []
		for number in range(count):
			start = len(rows)
			stores = 4 if name in {'float_pair_uninit', 'floats_uninit'} else 2
			rows += [('vmovdqu', f'%ymm{i},({i * 32}%rdi)') for i in range(stores)]
			ops = [('vpor', '%ymm0,%ymm1,%ymm0')] * (length - stores - 2 + extra)
			if stores == 4:
				op = ['vaddpd', 'vsubpd', 'vmulpd'][number] if count == 3 else 'vaddpd'
				ops[:4] = [(op, '%ymm0,%ymm1,%ymm0')] * 4
			rows += ops
			step = '$0x10,%rax' if stores == 4 and stride == '$0x8,%rax' else stride
			rows += [('add', step), ('jb' if stores == 4 else 'jne', 'local:' + str(start))]
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

	def test_uninit_extra_instruction(self):
		x = fixture()
		x['fused_uninit'] = fixture(1)['fused_uninit']
		self.assertEqual(assess(x)['fused_uninit']['status'], 'FAIL')
		self.assertEqual(assess(x)['fused']['status'], 'PASS')

	def test_float_tail_loops_recorded_and_exact(self):
		x = fixture()
		name = 'floats_uninit'
		start = len(x[name])
		tail = list(x[name]); tail[-1] = ('jne', 'local:' + str(start))
		x[name] += tail
		r = assess(x)[name]
		self.assertEqual(r['status'], 'PASS')
		self.assertEqual(r['extra_loop_offsets'], [start])
		x[name].insert(-1, ('nop', ''))
		self.assertEqual(assess(x)[name]['status'], 'FAIL')

	def test_float_missing_primary(self):
		x = fixture(); x['floats_uninit'][-1] = ('jne', 'local:0')
		self.assertEqual(assess(x)['floats_uninit']['status'], 'FAIL')

	def test_float_mixed_operations(self):
		x = fixture(); x['floats_uninit'][4] = ('vmulpd', '%ymm0,%ymm1,%ymm0')
		self.assertEqual(assess(x)['floats_uninit']['status'], 'FAIL')

	def test_float_missing_operation(self):
		x = fixture()
		x['float_pair_uninit'] = [(('vaddpd' if op == 'vsubpd' else op), args) for op, args in x['float_pair_uninit']]
		self.assertEqual(assess(x)['float_pair_uninit']['status'], 'FAIL')


if __name__ == '__main__':
	unittest.main()
