#!/usr/bin/env python3
"""Checked provenance-only redaction and regeneration of dependent proof pins."""
import argparse
import hashlib
import json
from pathlib import Path
import re

HOME = re.compile(r"/" + r"home/[^/\s\"']+")


def digest(data):
	return hashlib.sha256(data).hexdigest()


def redact(value, allowed, path=()):
	if isinstance(value, dict):
		return {k: redact(v, allowed, path + (k,)) for k, v in value.items()}
	if isinstance(value, list):
		return [redact(v, allowed, path + (i,)) for i, v in enumerate(value)]
	if isinstance(value, str) and HOME.search(value):
		if list(path) not in allowed:
			raise ValueError('unreviewed provenance field: ' + repr(path))
		return HOME.sub('~', value)
	return value


def checked(data, entry):
	if digest(data) != entry['before_sha256']:
		raise ValueError('old metadata digest differs')
	before = json.loads(data)
	after = redact(before, entry['fields'])
	if before == after:
		raise ValueError('no redaction')
	return after


def refresh(root, policy):
	root = Path(root)
	updates = {}
	for path, entry in policy['files'].items():
		if Path(path).is_absolute() or '..' in Path(path).parts:
			raise ValueError('invalid metadata path')
		data = (root / path).read_bytes()
		value = checked(data, entry)
		# Keep byte formatting and every non-path field unchanged.
		new = HOME.sub('~', data.decode()).encode()
		if json.loads(new) != value:
			raise ValueError('text substitution exceeds reviewed metadata fields')
		updates[path] = new
	proof = policy['proof']
	old = policy['files'][proof]['before_sha256'];new = digest(updates[proof])
	for path in policy['proof_consumers']:
		data = (root / path).read_bytes()
		if digest(data) != policy['consumer_sha256'][path] or data.count(old.encode()) != 1:
			raise ValueError('old proof consumer differs')
		updates[path] = data.replace(old.encode(), new.encode())
	snapshot = policy['snapshot'];source = policy['snapshot_source']
	data = (root / snapshot).read_bytes()
	if digest(data) != policy['consumer_sha256'][snapshot]:
		raise ValueError('old snapshot consumer differs')
	pattern = re.compile(rb'(const C9_PINNED_DISPOSITIONS: &str = r########")(.+?)("########;)', re.S)
	match = pattern.search(data)
	if not match or json.loads(match[2]) != json.loads((root / source).read_bytes()):
		raise ValueError('old snapshot does not match metadata')
	payload = json.dumps(json.loads(updates[source]), sort_keys=True, separators=(',', ':')).encode()
	updates[snapshot] = data[:match.start(2)] + payload + data[match.end(2):]
	return updates


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--repo', type=Path, default=Path(__file__).resolve().parents[1])
	p.add_argument('--policy', type=Path, required=True)
	p.add_argument('--receipt', type=Path, required=True)
	p.add_argument('--apply', action='store_true')
	a = p.parse_args();policy = json.loads(a.policy.read_text())
	updates = refresh(a.repo, policy)
	rows = [{'path':k, 'before_sha256':digest((a.repo/k).read_bytes()), 'after_sha256':digest(v)} for k,v in updates.items()]
	if a.apply:
		for k,v in updates.items(): (a.repo/k).write_bytes(v)
	a.receipt.parent.mkdir(parents=True, exist_ok=True)
	a.receipt.write_text(json.dumps({'status':'PASS','applied':a.apply,'files':rows},indent=2)+'\n')
	print('PASS:', a.receipt)


if __name__ == '__main__':
	main()
