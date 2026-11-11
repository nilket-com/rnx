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

class Observation(unittest.TestCase):
	def run_case(self, failure=None, multiplicity=1):
		import tempfile, json
		from pathlib import Path
		name='<rune::experimental_core::execute::Executor>::invoke_args::<false>'
		with tempfile.TemporaryDirectory() as tmp:
			out=Path(tmp)/'result.json'
			with patch.object(executor_identity,'digest',return_value='sha'), \
				patch.object(executor_identity,'symbols',side_effect=lambda _: {name:[{'address':16+i,'size':1} for i in range(multiplicity)]}), \
				patch.object(executor_identity,'Elf'), \
				patch.object(executor_identity.subprocess,'check_output',return_value='asm'), \
				patch.object(executor_identity,'resolve',side_effect=failure,return_value=['row']):
				status=executor_identity.run(Path('old'),Path('new'),out)
			return status,json.loads(out.read_text())
	def test_identical_resolvable_passes(self):
		status,r=self.run_case();self.assertEqual(status,0);self.assertEqual(r['status'],'PASS')
	def test_unreviewed_form_is_observation(self):
		status,r=self.run_case(executor_identity.UnreviewedForm('unreviewed','jmp *%rax'))
		self.assertEqual(status,1);self.assertNotIn('error',r)
		self.assertEqual(r['affected_symbols'],1)
		self.assertEqual(r['first_unreviewed']['instruction'],'jmp *%rax')
	def test_duplicates_are_not_partially_compared(self):
		status,r=self.run_case(multiplicity=3)
		self.assertEqual(status,1);self.assertNotIn('error',r)
		self.assertEqual(r['unsupported_classes'],{'duplicate symbol':1})
		self.assertEqual(r['resolved'],[{},{}])
	def test_other_errors_still_fatal(self):
		for error in (AssertionError('incomplete disassembly'), OSError('objdump failed'), ValueError('malformed input')):
			with self.subTest(error=str(error)):
				status,r=self.run_case(error);self.assertEqual(status,1);self.assertIn('error',r)

class GatePolicy(unittest.TestCase):
	def test_observation_continues_but_tool_error_stops(self):
		import tempfile, json
		from pathlib import Path
		from types import SimpleNamespace
		import fixed_route_gate as gate
		for fatal in (False,True):
			with self.subTest(fatal=fatal), tempfile.TemporaryDirectory() as tmp:
				out=Path(tmp)
				identity={'status':'FAIL','reason':'instruction identity not established: unreviewed form'}
				if fatal: identity['error']='tool failed'
				(out/'IDENTITY.json').write_text(json.dumps(identity))
				pair={'builds':[{'source_commit':n,'binary':n,'sha256':'sha'} for n in ('old','new')]}
				with patch.object(gate.subprocess,'run',return_value=SimpleNamespace(returncode=1)), \
					patch.object(gate,'stage',return_value={'records':[]}) as capture, \
					patch.object(gate,'capture_key',return_value='key'):
					if fatal:
						with self.assertRaises(ValueError): gate.captures(out,pair,Path('vg'),{}, {})
						capture.assert_not_called()
					else:
						gate.captures(out,pair,Path('vg'),{}, {})
						capture.assert_called_once()

	def test_kernel_identity_keeps_hard_failure(self):
		import i64_compare_identity as kernel
		with patch.object(kernel.subprocess,'check_output',return_value='source'), \
			patch.object(kernel,'Elf'), patch.object(kernel,'extract',return_value={'symbol':{}}), \
			patch.object(kernel,'resolve',side_effect=kernel.UnreviewedForm('unreviewed','jmp *%rax')):
			with self.assertRaises(kernel.UnreviewedForm): kernel.evaluate(['old','new'],['a','b'])
