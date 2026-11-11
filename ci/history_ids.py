#!/usr/bin/env python3
"""Resolve reviewed history-rewrite identities without fetching or restoring data."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

MAP = Path(__file__).with_name('history_map.json')
HEX = re.compile(r'[0-9a-f]{40}')


def load(path=MAP):
	def unique(items):
		result = {}
		for key, value in items:
			if key in result: raise ValueError('duplicate map key: ' + key)
			result[key] = value
		return result
	data = json.loads(Path(path).read_text(), object_pairs_hook=unique)
	if data['schema'] != 1 or set(data['repositories']) != {'rnx', 'rnx-bench'}:
		raise ValueError('unknown map schema/repositories')
	for name, row in data['repositories'].items():
		pairs = row['pairs']
		if row['count'] != len(pairs) or len(set(pairs.values())) != len(pairs):
			raise ValueError('incomplete or duplicate map')
		if row['count'] != {'rnx': 342, 'rnx-bench': 281}[name]:
			raise ValueError('unexpected pair count')
		if any(not HEX.fullmatch(x) for x in [*pairs, *pairs.values(), *row['unchanged_roots'], row['old_tip'], row['new_tip']]):
			raise ValueError('malformed commit identity')
		if set(pairs) & set(pairs.values()) or pairs.get(row['old_tip']) != row['new_tip']:
			raise ValueError('invalid rewrite identities')
	if hashlib.sha256(json.dumps(data, sort_keys=True, separators=(',', ':')).encode()).hexdigest() != '25c7287e9a75054c564e61a3392bf2561ac9a19b8715400c6b289e7cf035dd40':
		raise ValueError('map differs from reviewed projection')
	return data


def git(root, *args):
	return subprocess.check_output(['git', '--no-replace-objects', '-C', str(root), *args], stderr=subprocess.PIPE).decode().strip()


def resolve(data, repository, value, root):
	if repository not in data['repositories'] or not re.fullmatch(r'[0-9a-f]{7,40}', value):
		raise ValueError('unknown repository or invalid identity')
	row = data['repositories'][repository]
	# Only the preserved ancestor graph may justify an omitted identity.
	unchanged = set(git(root, 'rev-list', *row['unchanged_roots']).splitlines()) if row['unchanged_roots'] else set()
	if unchanged & (set(row['pairs']) | set(row['pairs'].values())):
		raise ValueError('rewrite overlaps unchanged closure')
	choices = {(old, new) for old, new in row['pairs'].items() if old.startswith(value) or new.startswith(value)}
	choices.update((x, x) for x in unchanged if x.startswith(value))
	if len(choices) != 1: raise ValueError('unknown or ambiguous identity in repository')
	return next(iter(choices))


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('repository', choices=['rnx', 'rnx-bench']); p.add_argument('identity')
	p.add_argument('--repo', type=Path, default=Path.cwd()); p.add_argument('--path')
	a = p.parse_args(); old, new = resolve(load(), a.repository, a.identity, a.repo)
	result = dict(repository=a.repository, old=old, new=new)
	if a.path:
		path = Path(a.path)
		if path.is_absolute() or '..' in path.parts or ':' in a.path: raise ValueError('invalid repository path')
		try:
			blobs = [git(a.repo, 'rev-parse', '--verify', rev + ':' + a.path) for rev in (old, new)]
			if any(git(a.repo, 'cat-file', '-t', b) != 'blob' for b in blobs): raise ValueError('not a file')
			result['same_blob'] = blobs[0] == blobs[1]
		except (subprocess.CalledProcessError, ValueError): result['same_blob'] = None
		if result['same_blob'] is not True: result['archive_tool'] = 'tools/archive_data.py'
	print(json.dumps(result, indent=2))


if __name__ == '__main__': main()
