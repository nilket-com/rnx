import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch
from types import SimpleNamespace
from disk_space import observe, MINIMUM_FREE_BYTES


class DiskAdmission(unittest.TestCase):
	def test_boundary_and_nonexistent_output(self):
		with tempfile.TemporaryDirectory() as directory:
			path = Path(directory) / "future" / "output"
			for free, expected in [(MINIMUM_FREE_BYTES - 1, "FAIL"), (MINIMUM_FREE_BYTES, "PASS")]:
				with patch("disk_space.shutil.disk_usage", return_value=SimpleNamespace(free=free)):
					result = observe([path])
					self.assertEqual(result["status"], expected)
					self.assertEqual(result["filesystems"][0]["path"], directory)

	def test_all_destinations_must_fit(self):
		with patch("disk_space.shutil.disk_usage", side_effect=[SimpleNamespace(free=MINIMUM_FREE_BYTES), SimpleNamespace(free=1)]):
			self.assertEqual(observe([Path.cwd(), Path.cwd()])["status"], "FAIL")
		self.assertEqual(observe([])["status"], "FAIL")


if __name__ == "__main__":
	unittest.main()
