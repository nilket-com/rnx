"""Exact reviewed per-function setup deltas, bound to the measured binary pair."""
import json
from pathlib import Path

POLICY = json.loads(Path(__file__).with_name('0220_pins.json').read_text())


def verify_pair(builds):
	actual = {b['role']: {k: b[k] for k in ('source_commit', 'sha256')} for b in builds}
	if len(builds) != 2 or actual != POLICY['binaries']:
		raise ValueError('ADR0220 reviewed binary pair mismatch')


def check_vectors(a, b, kind, layout, rows):
	pins = POLICY['vectors'][kind + '/' + layout]
	failures, applied = {}, {}
	for name in a.keys() | b.keys() | pins.keys():
		left, right = a.get(name, (0, 0)), b.get(name, (0, 0))
		pin = pins.get(name)
		if pin is None:
			if left != right:
				failures[name] = {'baseline': left, 'candidate': right}
			continue
		delta = pin['delta_Ir'] if 'delta_Ir' in pin else 2 - (rows + 1023) // 1024
		actual = right[0] - left[0]
		if actual != delta or [left[1], right[1]] != pin['calls']:
			failures[name] = {'expected_delta': delta, 'actual_delta': actual, 'expected_calls': pin['calls'], 'actual_calls': [left[1], right[1]]}
		applied[name] = {'delta_Ir': actual, 'expected_delta_Ir': delta, 'calls': [left[1], right[1]]}
	return failures, applied
