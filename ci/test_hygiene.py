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


class DataHygiene(unittest.TestCase):
	def check_data(self, files, entries=None, baseline=None, exemptions=None):
		from data_hygiene import check
		return check(files, {'entries':entries or {}, 'duplicate_exemptions':exemptions or {}}, baseline or {})

	def test_thresholds_and_config(self):
		self.assertFalse(self.check_data({'a.json':b'x'*16384}))
		self.assertTrue(self.check_data({'a.json':b'x'*16385}))
		self.assertFalse(self.check_data({'Cargo.lock':b'x'*20000}))
		entry={'class':'source-configuration','reason':'reviewed policy','max_bytes':20000}
		self.assertFalse(self.check_data({'policy.json':b'x'*20000},{'policy.json':entry}))

	def test_live_loader_and_dead_reference(self):
		data=b'x'*20000;expression='Path("fixtures").glob("*.json")'
		entry={'class':'live-fixture','reason':'fixture loader','max_bytes':20000,'consumer':{'path':'reader.py','expression':expression}}
		self.assertFalse(self.check_data({'a.json':data,'reader.py':expression.encode()},{'a.json':entry}))
		self.assertTrue(self.check_data({'a.json':data,'README.md':expression.encode()},{'a.json':entry}))
		entry['consumer']['expression']='NAME = "a.json"'
		self.assertTrue(self.check_data({'a.json':data,'reader.py':b'NAME = "a.json"'},{'a.json':entry}))

	def test_new_megabyte_and_growth(self):
		files={'a.json':b'x'*1048576};entry={'class':'reviewed-retention','reason':'fixture retained','max_bytes':1048576}
		self.assertTrue(self.check_data(files,{'a.json':entry}))
		self.assertFalse(self.check_data(files,{'a.json':entry},baseline={'a.json':1048576}))
		self.assertTrue(self.check_data(files,{'a.json':entry},baseline={'a.json':1024}))
		entry['allow_large']=True
		self.assertFalse(self.check_data(files,{'a.json':entry}))
		entry['max_bytes']-=1
		self.assertTrue(self.check_data(files,{'a.json':entry}))

	def test_duplicate_exact_exemption(self):
		import hashlib
		data=b'x'*4096;files={'a.json':data,'b.json':data};sha=hashlib.sha256(data).hexdigest()
		self.assertTrue(self.check_data(files))
		exempt={sha:{'paths':['a.json','b.json'],'reason':'distinct pinned corpus cases'}}
		self.assertFalse(self.check_data(files,exemptions=exempt))
		files['c.json']=data
		self.assertTrue(self.check_data(files,exemptions=exempt))
		self.assertFalse(self.check_data({'a.rs':data,'b.rs':data}))
