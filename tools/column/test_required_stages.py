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
		with self.assertRaisesRegex(ValueError, 'package unregistered/Cargo.toml: declare'):
			required(['Cargo.toml'], [{'unregistered/Cargo.toml': []}])

	def test_every_registered_package_source_has_format_and_lint(self):
		from package_gates import PACKAGES, quality
		for manifest in list(PACKAGES) + ['rune/crates/rune/Cargo.toml', 'rune/crates/rune-alloc/Cargo.toml']:
			path = str(__import__('pathlib').Path(manifest).parent/'src/lib.rs')
			for source in (path, path.replace('src/lib.rs','tests/control.rs'), path.replace('src/lib.rs','build.rs')):
				with self.subTest(manifest=manifest, source=source):
					stages = required([source], manifests=list(PACKAGES) + [manifest])
					tests, formatting, lint = quality(manifest)
					self.assertTrue(set((*tests, formatting, lint)) <= set(stages))

	def test_longest_owner_and_unregistered_source_fail_closed(self):
		manifests = ['Cargo.toml','rune/tools/xtask/Cargo.toml','rune/tools/xtask/measure/harness/Cargo.toml','ci/execute_guard/Cargo.toml']
		for path, package in [('rune/tools/xtask/src/main.rs','rune/tools/xtask/Cargo.toml'),
			('rune/tools/xtask/measure/harness/src/main.rs','rune/tools/xtask/measure/harness/Cargo.toml'),
			('ci/execute_guard/src/main.rs','ci/execute_guard/Cargo.toml')]:
			with self.subTest(path=path), self.assertRaisesRegex(ValueError, package+': declare.*format and lint'):
				required([path], manifests=manifests)

	def test_documentation_does_not_gain_quality_stages(self):
		self.assertEqual(required(['README.md','plans/adr/example.md']), ['commit-discipline','controller-controls'])
		stages = required(['rune/crates/rune/README.md'])
		self.assertNotIn('format',stages)
		self.assertIn('rune-clippy',stages)  # Existing conservative selection stays.
		self.assertEqual(required(['ci/execute_guard/README.md'],manifests=['Cargo.toml','ci/execute_guard/Cargo.toml']), ['commit-discipline','controller-controls'])

	def test_graph_selection_is_closed_under_quality_and_removed_owner(self):
		from package_gates import PACKAGES, quality
		for manifest in PACKAGES:
			selected = set(required([manifest],[{manifest:[]},{}]))
			tests, formatting, lint = quality(manifest)
			self.assertTrue(set((*tests,formatting,lint)) <= selected)
		with self.assertRaisesRegex(ValueError,'ci/execute_guard/Cargo.toml: declare'):
			required(['ci/execute_guard/Cargo.toml'],[{'ci/execute_guard/Cargo.toml':[]},{}])

	def test_new_quality_commands_are_canonical_and_formats_precede_compilation(self):
		import accept
		from pathlib import Path
		from package_gates import FORMATS, LINTS
		commands = accept.commands(Path('output'))
		names = [name for name,_ in commands]
		self.assertTrue(set(FORMATS)|set(LINTS) <= set(names))
		self.assertTrue(all(names.index(n)<names.index('clippy') for n in FORMATS))
		self.assertTrue(all(accept.BULK_ROOTS[n]==('baseline',) for n in LINTS))


if __name__=='__main__':unittest.main()
