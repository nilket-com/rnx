"""Send terminal job notices using a launcher-resolved absolute executable."""
from pathlib import Path
import subprocess


def terminal(executable, summary, body):
	executable = Path(executable)
	if not executable.is_absolute():
		raise ValueError("notice executable must be absolute")
	subprocess.run([str(executable), "send", "--as", "sluice", "--to", "codex",
		"--conversation", "rune-native", "--summary", summary, "--body-file", str(body)], check=True)
