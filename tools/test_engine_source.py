import copy
from pathlib import Path
import tempfile
import unittest
from engine_source import check_manifest, check_lock, check_metadata


class Sources(unittest.TestCase):
	def setUp(self):
		self.temp = tempfile.TemporaryDirectory()
		self.addCleanup(self.temp.cleanup)
		self.root = Path(self.temp.name)
		self.engine = self.root / 'rune/crates/rune'
		self.engine.mkdir(parents=True)
		(self.engine / 'Cargo.toml').write_text('[package]\nname="rune"\nversion="0.15.0"\n')
		self.manifest = self.root / 'Cargo.toml'
		self.metadata = {'packages': [{'name': 'rune', 'source': None, 'manifest_path': str(self.engine / 'Cargo.toml')}]}

	def test_canonical_path(self):
		self.manifest.write_text('[dependencies]\nrune={path="rune/crates/rune"}\n')
		self.assertEqual(check_manifest(self.root, self.manifest), ['rune'])
		self.assertEqual(check_metadata(self.root, self.metadata), ['rune'])

	def test_external_git_rejected(self):
		self.manifest.write_text('[dependencies]\nrune={git="https://example.invalid/rune"}\n')
		with self.assertRaises(ValueError):
			check_manifest(self.root, self.manifest)

	def test_path_escape_rejected(self):
		other = self.root / 'other'
		other.mkdir()
		(other / 'Cargo.toml').write_text('[package]\nname="rune"\n')
		self.manifest.write_text('[dependencies]\nrune={path="other"}\n')
		with self.assertRaises(ValueError):
			check_manifest(self.root, self.manifest)

	def test_duplicate_resolved_engine(self):
		self.metadata['packages'] *= 2
		with self.assertRaises(ValueError):
			check_metadata(self.root, self.metadata)

	def test_locked_external_or_duplicate(self):
		lock = self.root / 'Cargo.lock'
		entry = '[[package]]\nname="rune"\nversion="0.15.0"\n'
		for text in [entry+'source="git+external"\n', entry+entry]:
			lock.write_text(text)
			with self.assertRaises(ValueError):
				check_lock(lock)

	def test_empty_graph_not_vacuous(self):
		with self.assertRaises(ValueError):
			check_metadata(self.root, {'packages': []})


if __name__ == '__main__':
	unittest.main()
