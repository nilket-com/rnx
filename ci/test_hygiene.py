import subprocess
import tempfile
from pathlib import Path
import unittest
from hygiene import blob, scan


class Hygiene(unittest.TestCase):
	def setUp(self):
		self.tmp = tempfile.TemporaryDirectory()
		self.addCleanup(self.tmp.cleanup)
		self.root = Path(self.tmp.name)
		subprocess.run(['git', 'init', '-q', str(self.root)], check=True)

	def track(self, path, data):
		p = self.root / path
		p.parent.mkdir(parents=True, exist_ok=True)
		p.write_bytes(data)
		subprocess.run(['git', '-C', str(self.root), 'add', '--', path], check=True)

	def test_code_and_binary_are_scanned(self):
		for data in (b'chat' + b'ctl', b'/' + b'home/person/work'):
			self.track('a name\nwith-newline.bin', b'\0' + data)
			self.assertEqual(len(scan(self.root, {})), 1)

	def test_new_plan_and_unlisted_history_fail(self):
		self.track('plans/adr/9999_new.md', b'/' + b'home/person/x')
		self.track('probes/old.txt', b'chat' + b'ctl')
		self.assertEqual(len(scan(self.root, {})), 2)

	def test_exact_history_only(self):
		data = b'/' + b'home/person/x'
		self.track('plans/old.md', data)
		allow = {'plans/old.md': blob(data)}
		self.assertFalse(scan(self.root, allow))
		(self.root / 'plans/old.md').write_bytes(data + b' edited')
		self.assertTrue(scan(self.root, allow))
		self.assertFalse(scan(self.root, allow, indexed=True))

	def test_code_cannot_be_exempted(self):
		data = b'chat' + b'ctl'
		self.track('tools/a.py', data)
		self.assertTrue(scan(self.root, {'tools/a.py': blob(data)}))

	def test_clean_and_untracked(self):
		self.track('ok.txt', b'./local/tool')
		(self.root / 'untracked').write_bytes(b'chat' + b'ctl')
		self.assertFalse(scan(self.root, {}))
