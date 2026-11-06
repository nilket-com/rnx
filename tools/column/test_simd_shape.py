import unittest
from simd_shape import NAMES, PREFIX, shapes, compare


def fixture(base=4096, op='vpaddq'):
	return '\n'.join(f'{base+i*32:x} <{PREFIX}{name}>:\n {base+i*32:x}: {op} %ymm0,%ymm1,%ymm2\n {base+i*32+4:x}: jne {base+i*32:x} <{PREFIX}{name}>\n {base+i*32+6:x}: ret' for i,name in enumerate(NAMES))


class ShapeTests(unittest.TestCase):
	def test_relocation_preserves_shape(self):
		self.assertTrue(all(r['equal'] for r in compare(shapes(fixture()), shapes(fixture(8192))).values()))

	def test_changed_vector_operation_fails(self):
		self.assertFalse(any(r['equal'] for r in compare(shapes(fixture()), shapes(fixture(op='vpsubq'))).values()))

	def test_duplicate_unrelated_symbols_ignored(self):
		unrelated = '\n8000 <other>:\n 8000: ret\n9000 <other>:\n 9000: ret'
		self.assertEqual(shapes(fixture()), shapes(fixture() + unrelated))

	def test_duplicate_protected_symbol_fails(self):
		with self.assertRaises(ValueError):
			shapes(fixture() + '\n' + fixture(8192))

	def test_missing_symbol_fails(self):
		with self.assertRaises(ValueError):
			shapes(fixture().replace(PREFIX+'fused', PREFIX+'fused_uninit'))

	def test_added_instruction_fails(self):
		changed = fixture().replace(': ret', ': nop')
		self.assertFalse(any(r['equal'] for r in compare(shapes(fixture()), shapes(changed)).values()))


if __name__ == '__main__':
	unittest.main()
