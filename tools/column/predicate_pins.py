"""Reviewed constant-only setup vectors; exact binary pair and absolute calls."""
import json
from pathlib import Path

POLICY = json.loads(Path(__file__).with_name('0223_pins.json').read_text())


def verify_pair(builds):
	actual = {b['role']: {k: b[k] for k in ('source_commit', 'sha256')} for b in builds}
	expected = {b['role']: {k: b[k] for k in ('source_commit', 'sha256')} for b in POLICY['builds']}
	if len(builds) != 2 or actual != expected: raise ValueError('ADR0223 measured pair mismatch')


def check_vectors(a, b, kind):
	pins = POLICY['rules'][kind]
	failures = {}
	for name in a.keys() | b.keys() | pins.keys():
		left, right = a.get(name, (0, 0)), b.get(name, (0, 0))
		pin = pins.get(name)
		if pin is None:
			if left != right: failures[name] = dict(baseline=left, candidate=right)
		elif right[0] - left[0] != pin['constant_Ir'] or [left[1], right[1]] != pin['calls']:
			failures[name] = dict(expected=pin, actual_delta=right[0]-left[0], actual_calls=[left[1],right[1]])
	return failures
