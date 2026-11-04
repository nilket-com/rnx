"""Preserve a declared oracle report without allowing source mutations."""
from collections import Counter
import hashlib
import json
from pathlib import Path
import subprocess

ORACLE = 'adapters/polars/oracle-results.json'
APPROVED = {'match', 'row_order_differs', 'both_error', 'both_panic'}


def validate_report(original, actual):
	before, after = json.loads(original), json.loads(actual)
	assert before['cases'] == after['cases'] == len(after['results']) == after['verified'], 'incomplete oracle report'
	old = {row['id']: row for row in before['results']}
	new = {row['id']: row for row in after['results']}
	assert len(new) == len(after['results']) and old.keys() == new.keys(), 'oracle cases changed'
	for key, row in new.items():
		assert all(row[field] == old[key][field] for field in ('path', 'policy', 'unordered')), 'oracle policy changed'
		assert row['status'] in APPROVED, 'unapproved oracle outcome'
		assert row['status'] != 'row_order_differs' or row['unordered'], 'ordered oracle mismatch'
	assert dict(Counter(row['status'] for row in new.values())) == after['tally'], 'oracle tally mismatch'


def finish_stage(root, stage, out):
	"""Called after the test, before exact-source checking; never overrides its exit."""
	if stage != 'polars-suite':
		return None
	status = subprocess.check_output(['git', 'status', '--porcelain=v1', '-z'], cwd=root).split(b'\0')
	assert all(row in (b'', b' M ' + ORACLE.encode()) for row in status), 'unexpected source mutation'
	path = root / ORACLE
	assert path.is_file() and not path.is_symlink(), 'oracle report is not a regular file'
	assert subprocess.check_output(['git', 'diff', '--summary', '--', ORACLE], cwd=root) == b'', 'oracle mode changed'
	original = subprocess.check_output(['git', 'show', 'HEAD:' + ORACLE], cwd=root)
	actual = path.read_bytes()
	archive = out / 'polars-oracle-results.json'
	with archive.open('xb') as stream:
		stream.write(actual)
	validate_report(original, actual)
	path.write_bytes(original)
	assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=root).strip(), 'source changed'
	return {'output': ORACLE, 'archive': str(archive),
		'original_sha256': hashlib.sha256(original).hexdigest(),
		'actual_sha256': hashlib.sha256(actual).hexdigest(),
		'verified': json.loads(actual)['verified']}
