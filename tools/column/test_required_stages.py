import unittest
from required_stages import required, affected
from suite_stage import discovered, executed
from miri_exact import manifest


class Coverage(unittest.TestCase):
	def test_jupyter_only_diff_requires_its_package_and_kernel(self):
		self.assertTrue({'jupyter-format','jupyter-clippy','jupyter-tests','jupyter-kernel'} <= set(required(['jupyter/src/worker.rs'])))

	def test_before_graph_retains_removed_dependents(self):
		old={'Cargo.toml':['tools/project/Cargo.toml'],'tools/project/Cargo.toml':[]};new={'Cargo.toml':[]}
		self.assertEqual(affected(['tools/project/Cargo.toml'],[old,new]),set(old))
		self.assertTrue({'root-tests','inventory','project-suite','cross'} <= set(required(['tools/project/Cargo.toml'],[old,new])))

	def test_runtime_and_adapter(self):
		self.assertIn('fixed-route',required(['rune/crates/rune/src/lib.rs']))
		self.assertNotIn('fixed-route',required(['rune/crates/rune/README.md']))
		self.assertIn('suite-adapter-postgres',required(['adapters/postgres/src/lib.rs']))

	def test_empty_discovery_execution_inventory(self):
		with self.assertRaises(ValueError):discovered('0 tests')
		with self.assertRaises(ValueError):executed('test result: ok. 0 passed;')
		self.assertEqual(discovered('name: test\n'),['name'])
		self.assertEqual(executed('test result: ok. 1 passed;'),1)
		with self.assertRaises(ValueError):manifest('rune',{'schema':1,'tests':[]})

	def test_root_manifest_keeps_all_new_packages_and_unknown_fails_closed(self):
		from suite_stage import PACKAGE_CHECKS
		nodes = {p: [] for p in PACKAGE_CHECKS}
		selected = required(['Cargo.toml'], [nodes, nodes])
		self.assertTrue({name for name, _ in PACKAGE_CHECKS.values()} <= set(selected))
		with self.assertRaisesRegex(ValueError, 'affected package needs a registered suite'):
			required(['Cargo.toml'], [{'unregistered/Cargo.toml': []}])


if __name__=='__main__':unittest.main()
