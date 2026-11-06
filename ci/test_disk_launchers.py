import importlib.util
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "tools/column"))


class LauncherAdmission(unittest.TestCase):
	def test_low_disk_prevents_systemd_launch(self):
		for filename, arguments in [
			("launch.py", ["--unit", "test", "--cwd", str(ROOT), "--hosts", "unused.json", "--", "true"]),
			("ir_launch.py", ["--unit", "test", "--manifest", "unused.json", "--peak-bytes", "1", "--peak-evidence", "test", "--", "true"]),
		]:
			with self.subTest(filename=filename):
				spec = importlib.util.spec_from_file_location("disk_test_launcher", ROOT / "tools/column" / filename)
				module = importlib.util.module_from_spec(spec)
				spec.loader.exec_module(module)
				calls = []
				def execute(command, **kwargs):
					calls.append(command)
					if any(str(part).endswith("ci/disk_space.py") for part in command):
						raise subprocess.CalledProcessError(1, command)
					return subprocess.CompletedProcess(command, 0)
				with patch.object(sys, "argv", [filename] + arguments), patch.object(module.subprocess, "run", side_effect=execute):
					with self.assertRaises(subprocess.CalledProcessError):
						module.main()
				self.assertTrue(any(any(str(p).endswith("ci/disk_space.py") for p in c) for c in calls))
				self.assertFalse(any(c[0] == "systemd-run" for c in calls))


if __name__ == "__main__":
	unittest.main()
