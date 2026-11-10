import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('archive_data', Path(__file__).resolve().parents[1]/'tools/archive_data.py')
archive = importlib.util.module_from_spec(spec)
spec.loader.exec_module(archive)


class Archive(unittest.TestCase):
	def test_original_identity_and_fail_closed(self):
		with tempfile.TemporaryDirectory() as tmp:
			root=Path(tmp);payload=root/'payload';payload.write_bytes(b'original')
			row=dict(repository='rnx',path='probes/old.json',blob='b'*40,archive='payload',sha256=archive.digest(payload))
			index=root/'INDEX.json';catalog=root/'catalog.json'
			def install(rows):
				index.write_text(json.dumps({'files':rows}));catalog.write_text(json.dumps({'index_sha256':archive.digest(index)}))
			def locate():return archive.locate(root,'rnx','probes/old.json','b'*40,catalog)
			install([row]);self.assertEqual(locate()['blob'],'b'*40)
			payload.write_bytes(b'changed')
			with self.assertRaises(ValueError):locate()
			payload.write_bytes(b'original');install([row,row])
			with self.assertRaises(ValueError):locate()
			install([dict(row,archive='../outside')])
			with self.assertRaises(ValueError):locate()
			install([row]);index.write_text('{}')
			with self.assertRaises(ValueError):locate()
