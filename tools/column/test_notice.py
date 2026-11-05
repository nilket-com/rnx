from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
from notice import terminal


class Notice(unittest.TestCase):
	def test_failure_sends_notice_with_empty_path(self):
		with tempfile.TemporaryDirectory() as directory:
			root = Path(directory)
			sender = root / "sender"
			output = root / "sent"
			sender.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "' + str(output) + '"\n')
			sender.chmod(0o700)
			body = root / "NOTICE.txt"
			body.write_text("job FAIL; receipt RESULT.json")
			with patch.dict("os.environ", {"PATH": ""}):
				try:
					raise RuntimeError("controlled job failure")
				except RuntimeError:
					terminal(sender, "job EXIT FAIL", body)
			self.assertIn("job EXIT FAIL", output.read_text())
			self.assertIn(str(body), output.read_text())

	def test_relative_sender_rejected(self):
		with self.assertRaises(ValueError):
			terminal("chatctl", "FAIL", "notice")
