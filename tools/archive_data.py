#!/usr/bin/env python3
"""Locate and verify archived repository evidence without restoring it into git."""
import argparse
import hashlib
import json
from pathlib import Path


def digest(path):
	h = hashlib.sha256()
	with path.open('rb') as stream:
		while chunk := stream.read(1024 * 1024): h.update(chunk)
	return h.hexdigest()


def locate(archive, repository, path, blob=None, catalog_path=None):
	catalog = json.loads((catalog_path or Path(__file__).resolve().parents[1]/'ci/data_archive.json').read_text())
	index_path = archive/'INDEX.json'
	if digest(index_path) != catalog['index_sha256']: raise ValueError('archive index checksum mismatch')
	index = json.loads(index_path.read_text())
	rows = [r for r in index['files'] if r['repository'] == repository and r['path'] == path and (blob is None or r['blob'] == blob)]
	if len(rows) != 1: raise ValueError('archive locator must resolve exactly one original file')
	row = rows[0]; target = (archive/row['archive']).resolve()
	if not target.is_relative_to(archive.resolve()) or digest(target) != row['sha256']:
		raise ValueError('archive artifact checksum/path mismatch')
	return dict(row, location=str(target))


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--archive', required=True, type=Path)
	p.add_argument('--repository', choices=['rnx', 'rnx-bench'], required=True)
	p.add_argument('--path', required=True)
	p.add_argument('--blob')
	a = p.parse_args()
	print(json.dumps(locate(a.archive, a.repository, a.path, a.blob), indent=2))


if __name__ == '__main__': main()
