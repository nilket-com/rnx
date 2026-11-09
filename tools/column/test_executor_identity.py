"""Complete executor impl symbols must not silently disappear from the guard."""
import unittest
from unittest.mock import patch
import executor_identity

class Symbols(unittest.TestCase):
	def test_impl_angle_brackets_and_new_sliced_exclusion(self):
		name='<rune::experimental_core::execute::Executor>::invoke_args::<false>::{closure#0}'
		text=f'10 20 t {name}\n40 10 T <rune::experimental_core::execute::SlicedInvocation>::resume\n'
		with patch('executor_identity.subprocess.check_output',return_value=text):
			self.assertEqual(executor_identity.symbols('binary'),{name:[{'address':16,'size':32}]})
	def test_missing_fixed_invocation_fails(self):
		with patch('executor_identity.subprocess.check_output',return_value='10 20 t rune::experimental_core::other\n'):
			with self.assertRaises(AssertionError):executor_identity.symbols('binary')

if __name__=='__main__':unittest.main()
