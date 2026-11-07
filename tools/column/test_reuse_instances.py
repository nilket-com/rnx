import unittest
from reuse_instances import OLD, NEW, inventory, evaluate


class Instances(unittest.TestCase):
	def test_only_reviewed_rename_and_entries(self):
		a = '0001 T rune::experimental_core::column::run::f64_plan::run::<&mut ' + OLD + '>\n'
		b = a.replace(OLD, NEW)
		self.assertEqual(evaluate(inventory(a), inventory(b))['status'], 'PASS')
		for extra in [b, a.replace('script::map::', 'script::map_reusing::'), '0002 T rune::experimental_core::column::run::new_helper\n']:
			self.assertEqual(evaluate(inventory(a), inventory(b + extra))['status'], 'FAIL')
		self.assertTrue(evaluate(inventory(a), inventory(''))['removed'])
		extra = '0002 T rune::experimental_core::column::run::PreparedColumn::execute_reusing::<T>\n'
		self.assertEqual(evaluate(inventory(a), inventory(b + extra))['status'], 'PASS')

	def test_only_named_per_call_frames_can_be_outlined(self):
		a = inventory('0001 T rune::experimental_core::column::run::f64_plan::run::<T>')
		for frame in ['<rune::experimental_core::column::PreparedColumn>::allocate_batch::<T>',
			'<rune::experimental_core::column::run::flat::FlatBatch>::run_flat::<T>',
			'core::ptr::drop_glue::<rune::experimental_core::column::run::flat::Output>']:
			b = a + inventory('0002 T ' + frame)
			self.assertEqual(evaluate(a, b)['status'], 'PASS')
			self.assertEqual(evaluate(a, b + inventory('0003 T ' + frame))['status'], 'FAIL')
		for hot in ['f64_plan::lanes', 'f64_plan::arithmetic', 'f64_plan::publish',
			'boolean::run', 'predicate::run', 'masked::masked_fast', 'operands::execute']:
			b = a + inventory('0002 T rune::experimental_core::column::run::' + hot + '::<T>')
			self.assertEqual(evaluate(a, b)['status'], 'FAIL')


if __name__ == '__main__':
	unittest.main()
