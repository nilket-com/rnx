import copy
import hashlib
import json
import unittest
from redact_metadata import checked, digest, redact


class Redaction(unittest.TestCase):
	def test_only_reviewed_provenance_changes(self):
		value={'path':'/'+'home/person/evidence/x','source_sha256':'a'*64,'result':{'status':'PASS','count':9}}
		data=json.dumps(value).encode();entry={'before_sha256':digest(data),'fields':[['path']]}
		result=checked(data,entry)
		self.assertEqual(result,dict(value,path='~/evidence/x'))
		self.assertEqual(value['path'],'/'+'home/person/evidence/x')

	def test_old_pin_and_unreviewed_fields_refused(self):
		data=json.dumps({'path':'/'+'home/person/evidence'}).encode()
		with self.assertRaisesRegex(ValueError,'digest'):
			checked(data,{'before_sha256':'0'*64,'fields':[['path']]})
		with self.assertRaisesRegex(ValueError,'unreviewed'):
			checked(data,{'before_sha256':digest(data),'fields':[]})

	def test_semantic_edit_fails_old_identity(self):
		data=json.dumps({'path':'/'+'home/person/evidence','count':3}).encode()
		entry={'before_sha256':digest(data),'fields':[['path']]}
		with self.assertRaisesRegex(ValueError,'digest'):
			checked(data.replace(b'3',b'4'),entry)

	def test_nested_path_and_evidence_hash_preserved(self):
		v={'rows':[{'source':'/'+'home/person/r','sha256':'1'*64}]}
		result=redact(v,[['rows',0,'source']])
		self.assertEqual(result['rows'][0],{'source':'~/r','sha256':'1'*64})
