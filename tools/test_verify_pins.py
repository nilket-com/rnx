"""Compatibility entry point: pin checks now enforce canonical path sources."""
from pathlib import Path
import tempfile
import unittest
import sys
sys.path.insert(0, str(Path(__file__).resolve().parent))
from engine_source import check_tree
from verify import validate_request


class CanonicalSources(unittest.TestCase):
	def test_live_tree_has_canonical_engine(self):
		result = check_tree(Path(__file__).resolve().parents[1])
		self.assertIn("Cargo.lock", result["locks"])
		self.assertIn("tools/project/Cargo.lock", result["locks"])

	def test_new_lock_and_historical_exclusion(self):
		with tempfile.TemporaryDirectory() as directory:
			root = Path(directory)
			for name in ["Cargo.lock", "tools/project/Cargo.lock"]:
				p = root / name
				p.parent.mkdir(parents=True, exist_ok=True)
				p.write_text('[[package]]\nname="rune"\nversion="0.15.0"\n')
			old = root / "probes/old/Cargo.lock"
			old.parent.mkdir(parents=True)
			old.write_text('[[package]]\nname="rune"\nversion="0.15.0"\nsource="git+old"\n')
			self.assertEqual(len(check_tree(root)["locks"]), 2)
			new = root / "adapters/new/Cargo.lock"
			new.parent.mkdir(parents=True)
			new.write_bytes(old.read_bytes())
			with self.assertRaises(ValueError):
				check_tree(root)


class RequestShape(unittest.TestCase):
	def test_missing_path_fails(self):
		for request in ({"rune_rev": None}, {"rune_rev": None, "rune_path": None}):
			with self.assertRaisesRegex(ValueError, "missing request rune_path"):
				validate_request(request)

	def test_canonical_path_and_wrong_path(self):
		with tempfile.TemporaryDirectory() as directory:
			root = Path(directory)
			engine = root / "rune"
			engine.mkdir()
			validate_request({"rune_rev": None, "rune_path": engine, "rune_root": engine})
			with self.assertRaisesRegex(ValueError, "differs"):
				validate_request({"rune_rev": None, "rune_path": root, "rune_root": engine})


if __name__ == "__main__":
	unittest.main()
