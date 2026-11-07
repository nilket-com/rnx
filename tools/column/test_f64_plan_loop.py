import unittest
from f64_plan_loop import primary_loop


def loop(op='addpd', stride='0x80', count=8):
	rows = ['10: movupd (%rdx,%rax,1),%xmm0']
	rows += [f'{20+i*4:x}: {op} %xmm1,%xmm0' for i in range(count)]
	rows += [f'80: add ${stride},%rax', '84: cmp %rax,%rcx', '88: jne 10 <loop>']
	return '\n'.join(rows)


class LoopControls(unittest.TestCase):
	def test_primary16(self):
		self.assertEqual(primary_loop(loop())['status'], 'PASS')

	def test_primary8(self):
		self.assertEqual(primary_loop(loop(stride='0x40', count=4))['status'], 'PASS')

	def test_packed_tail_does_not_cover_scalar_primary(self):
		self.assertEqual(primary_loop(loop(op='addsd')+'\n90: addpd %xmm1,%xmm0\n94: jne 90 <tail>')['status'], 'FAIL')

	def test_wrong_stride(self):
		self.assertEqual(primary_loop(loop(stride='0x40'))['status'], 'FAIL')

	def test_missing_loop(self):
		self.assertEqual(primary_loop('10: addpd %xmm1,%xmm0')['status'], 'FAIL')

class PredicateArithmetic(unittest.TestCase):
	def test_four_lanes(self):
		assembly = loop(stride='0x20', count=2)
		self.assertEqual(primary_loop(assembly, driver='predicate')['status'], 'PASS')
		self.assertEqual(primary_loop(assembly)['status'], 'FAIL')

	def test_scalar_primary_vector_tail(self):
		assembly = loop(op='addsd') + '\n90: addpd %xmm1,%xmm0\n94: jne 90 <tail>'
		self.assertEqual(primary_loop(assembly, driver='predicate')['status'], 'FAIL')

	def test_wrong_stride(self):
		self.assertEqual(primary_loop(loop(stride='0x40', count=2), driver='predicate')['status'], 'FAIL')

	def test_bounds_branch(self):
		assembly = loop(stride='0x20', count=2).replace('10:', 'e: jae ff <panic>\n10:')
		# Place the bounds branch inside the primary loop, rather than before it.
		assembly = assembly.replace('e: jae ff <panic>\n', '').replace('14:', '12: jae ff <panic>\n14:')
		self.assertEqual(primary_loop(assembly, driver='predicate')['status'], 'FAIL')

