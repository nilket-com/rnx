import unittest
from i64_compare_identity import PREFIX, selected, resolve, compare, address_comment, UnreviewedForm


class Targets:
	data = b'\x11' * 32
	callee = ['original']
	def read(self, address, size):
		return self.data[:size]
	def pointer(self, address):
		return 200
	def code(self, address):
		return self.callee


def symbol(address=0x100, target=0x200, op='vmovdqa'):
	return dict(address=address, size=8,
		disassembly=f' {address:x}:\tc5 fd 6f 0d 00 00 00 00 \t{op} 0x0(%rip),%ymm1 # {target:x}\n')


class Identity(unittest.TestCase):
	def test_families(self):
		for name in ('comparisons::<false, 0, X, Y>', 'comparisons::<true, 0, X, Y>', 'arithmetic::<0, X, Y>'):
			self.assertTrue(selected(PREFIX + name))
		self.assertFalse(selected(PREFIX + 'i64_comparison_avx2'))

	def test_relocations_equal_data_and_op_changes_fail(self):
		elf = Targets()
		before = resolve(symbol(), elf)
		self.assertEqual(before, resolve(symbol(0x300, 0x400), elf))
		elf.data = b'\x22' * 32
		self.assertNotEqual(before, resolve(symbol(), elf))
		elf.data = b'\x11' * 32
		self.assertNotEqual(before, resolve(symbol(op='vmovdqu'), elf))
		with self.assertRaises(ValueError):
			resolve(symbol(op='vpxor'), elf)

	def test_calls_and_missing_inventory(self):
		elf = Targets()
		s = dict(address=0x100, size=6, disassembly=' 100:\tff 15 00 00 00 00 \tcall *0x0(%rip) # 106\n')
		before = resolve(s, elf)
		elf.callee = ['changed']
		self.assertNotEqual(before, resolve(s, elf))
		self.assertEqual(compare({}, {})['status'], 'FAIL')
		self.assertEqual(compare({'x': before}, {})['status'], 'FAIL')
		self.assertEqual(compare({'x': before}, {'x': before})['status'], 'PASS')

	def test_closure_names_and_address_comments(self):
		class Recording(Targets):
			def code(self, address): return ['callee', address]
		elf = Recording()
		branch = dict(address=0x100, size=5, disassembly=' 100:\te9 00 00 00 00 \tjmp 200 <outer::<inner::{closure#0}>>\n')
		row = resolve(branch, elf)[0]
		self.assertEqual(row['target']['code'], ['callee', 0x200])
		self.assertIsNone(address_comment('jmp 200 <symbol #123>'))
		self.assertEqual(address_comment('call *0x0(%rip) # 106 <name::{closure#9}>')[1], 0x106)
		rip = symbol()
		rip['disassembly'] = rip['disassembly'].rstrip() + ' <name::{closure#9}>\n'
		self.assertEqual(resolve(rip, elf), resolve(symbol(), elf))
		branch['size'] = 2
		branch['disassembly'] = ' 100:\tff e0 \tjmp *%rax\n'
		with self.assertRaisesRegex(UnreviewedForm, 'unreviewed indirect branch'):
			resolve(branch, elf)

	def test_unresolved_plt_is_unreviewed_but_incomplete_is_fatal(self):
		class External(Targets):
			def code(self, address): raise AssertionError('unresolved code target')
		s = dict(address=0x100, size=5, disassembly=' 100:\te8 00 00 00 00 \tcall 200 <_Unwind_Resume@plt>\n')
		with self.assertRaises(UnreviewedForm) as error: resolve(s, External())
		self.assertIn('_Unwind_Resume@plt', error.exception.instruction)
		s['size'] = 6
		with self.assertRaisesRegex(AssertionError, 'incomplete disassembly'): resolve(s, External())
		s['disassembly'] = 'unparseable row'
		with self.assertRaisesRegex(AssertionError, 'incomplete disassembly'): resolve(s, Targets())


if __name__ == '__main__':
	unittest.main()
