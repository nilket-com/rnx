#!/usr/bin/env python3
"""Refresh explicitly reviewed live census pins; historical provenance is immutable."""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess


def digest(data):
	return hashlib.sha256(data).hexdigest()


def refresh(manifest, paths, before, after, current, *, registry_only=False):
	"""Validate every live pin before returning a replacement; never mutate input."""
	result = copy.deepcopy(manifest)
	pins = result['registration_policy']['whole_files_sha256']
	if (not paths and not registry_only) or len(paths) != len(set(paths)) or not set(paths) <= pins.keys():
		raise ValueError('refresh requires distinct, existing live pin paths')
	for path, old in pins.items():
		p = Path(path)
		if p.is_absolute() or '..' in p.parts:
			raise ValueError('invalid live pin path')
		new, live = digest(after(path)), digest(current(path))
		if live != new:
			raise ValueError('working file differs from after revision: ' + path)
		if path in paths:
			if digest(before(path)) != old:
				raise ValueError('before revision does not match old pin: ' + path)
			if new == old:
				raise ValueError('named pin did not change: ' + path)
			pins[path] = new
		elif new != old:
			raise ValueError('unreviewed live pin drift: ' + path)
	function = result['registration_policy']['install_core']
	text = current(function['path']).decode()
	start, end = function['start'], function['end']
	if text.count(start) != 1:
		raise ValueError('ambiguous install_core boundary')
	a = text.index(start); b = text.index(end, a)
	if digest(text[a:b].encode()) != function['sha256']:
		raise ValueError('unreviewed install_core drift')
	return result


def refresh_registry(manifest, before, after, policy):
	"""Permit only the reviewed 0224 registry addition and doc-field change."""
	for witness in (before, after):
		if witness['lifecycle_closed'] is not True or witness['execution'] != 'not run; compile-only':
			raise ValueError('invalid lifecycle or execution witness')
	def index(witness):
		rows = witness['registry']; result = {r['path']: r for r in rows}
		if len(result) != len(rows):
			raise ValueError('duplicate registry item')
		return result
	for name, witness in [('before', before), ('after', after)]:
		if digest(json.dumps(witness['registry'], sort_keys=True, separators=(',', ':')).encode()) != policy[name + '_registry_sha256']:
			raise ValueError('registry witness identity mismatch')
	a, b = index(before), index(after)
	if len(a) != policy['before_count'] or len(b) != policy['after_count'] or manifest['registry_rows'] != len(a):
		raise ValueError('registry count mismatch')
	expected = copy.deepcopy(a)
	for name, change in policy['changed'].items():
		if expected.get(name) != change['before']:
			raise ValueError('old registry item mismatch')
		expected[name] = change['after']
	for name, value in policy['added'].items():
		if name in expected:
			raise ValueError('added registry item already exists')
		expected[name] = value
	if b != expected:
		raise ValueError('unreviewed registry difference')
	result = copy.deepcopy(manifest); result['registry_rows'] = len(b)
	return result


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument('--before', required=True)
	parser.add_argument('--after', required=True)
	parser.add_argument('--path', action='append', default=[])
	parser.add_argument('--registry-before', type=Path)
	parser.add_argument('--registry-after', type=Path)
	parser.add_argument('--write', action='store_true', help='apply the validated live-pin update')
	args = parser.parse_args()
	root = Path(__file__).resolve().parents[1]
	def revision(value):
		return subprocess.check_output(['git', 'rev-parse', '--verify', value + '^{commit}'], cwd=root, text=True).strip()
	before, after = revision(args.before), revision(args.after)
	def read(rev, path):
		return subprocess.check_output(['git', 'show', rev + ':' + path], cwd=root)
	path = root / 'tools/verify/fixtures/census_manifest.json'
	original = path.read_bytes(); manifest = json.loads(original)
	registry = args.registry_before is not None or args.registry_after is not None
	if registry and (args.registry_before is None or args.registry_after is None):
		raise ValueError('both registry witnesses required')
	updated = refresh(manifest, args.path, lambda p: read(before, p), lambda p: read(after, p), lambda p: (root / p).read_bytes(), registry_only=registry)
	if registry:
		policy = json.loads((root / 'tools/verify/fixtures/0224_registry_refresh.json').read_text())
		if before != policy['before_revision'] or after != policy['after_revision']:
			raise ValueError('unreviewed registry revision pair')
		if digest(read(before, 'src/time.rs')) != policy['before_source_sha256'] or digest(read(after, 'src/time.rs')) != policy['after_source_sha256']:
			raise ValueError('registry source identity mismatch')
		updated = refresh_registry(updated, json.loads(args.registry_before.read_bytes()), json.loads(args.registry_after.read_bytes()), policy)
	if path.read_bytes() != original:
		raise ValueError('manifest changed during validation')
	if args.write:
		path.write_text(json.dumps(updated, indent=2) + '\n')
	print(json.dumps(dict(status='PASS',before=before,after=after,paths=args.path,written=args.write,
		old_manifest_sha256=digest(original),new_manifest_sha256=digest((json.dumps(updated,indent=2)+'\n').encode())),indent=2))


if __name__ == '__main__':
	main()
