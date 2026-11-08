import unittest
from masked_float_identity import selected, evaluate, PREFIX


class Identity(unittest.TestCase):
	def test_no_uninit_or_empty_match(self):
		self.assertTrue(selected(PREFIX + 'masked_float_pair::<1>'))
		self.assertFalse(selected(PREFIX + 'float_pair_uninit'))
		self.assertFalse(selected(PREFIX + 'masked_pair_uninit::<1>'))

	def test_any_byte_and_missing_symbol_fail(self):
		a = {'kernel': {'bytes': '00ff'}}
		self.assertEqual(evaluate(a, a)['status'], 'PASS')
		self.assertEqual(evaluate(a, {'kernel': {'bytes': '00fe'}})['status'], 'FAIL')
		self.assertEqual(evaluate(a, {})['status'], 'FAIL')



class Resolved(unittest.TestCase):
	def test_changed_opcode_constant_and_call_target(self):
		from masked_float_identity import resolve
		class Elf:
			constant = b'\x11' * 32
			callee = ['panic_bounds_check']
			def read(self, address, size):
				return self.constant[:size]
			def pointer(self, address):
				return 200
			def code(self, address):
				return self.callee
		elf = Elf()
		def symbol(op='vmovdqa'):
			return dict(address=0x100, size=14, disassembly=f' 100:\tc5 fd 6f 0d 00 00 00 00 \t{op} 0x0(%rip),%ymm1 # 108\n 108:\tff 15 00 00 00 00 \tcall *0x0(%rip) # 10e\n')
		base = resolve(symbol(), elf)
		elf.constant = b'\x12' * 32
		self.assertNotEqual(base, resolve(symbol(), elf))
		elf.constant = b'\x11' * 32
		elf.callee = ['different_function']
		self.assertNotEqual(base, resolve(symbol(), elf))
		with self.assertRaises(ValueError): resolve(symbol('vpxor'), elf)


if __name__ == '__main__':
	unittest.main()
