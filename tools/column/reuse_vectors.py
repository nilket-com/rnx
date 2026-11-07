"""ADR 0228 exact measured-pair exception; all unlisted functions stay exact."""
import json
from pathlib import Path
from boolean_vectors import vector
from reuse_instances import OLD, NEW

POLICY = json.loads(Path(__file__).with_name('0228_pins.json').read_text())


def normalized(cost):
	out = {}
	for name, value in vector(cost).items():
		name = name.replace(OLD, NEW)
		if name in out:
			raise ValueError('normalization collision')
		out[name] = value
	return out


def verify_pair(builds):
	actual = [{k: b[k] for k in ('role', 'source_commit', 'sha256')} for b in builds]
	if actual != POLICY['builds']:
		raise ValueError('measured pair mismatch')


def check(a, b, kind, layout, rows, call):
	key = f'{kind}/{layout}/{call}/{"large" if rows >= 10000000 else "small"}'
	if key not in POLICY['rules']:
		raise ValueError('unmeasured case')
	rules = POLICY['rules'][key]
	blocks = (rows + 1023) // 1024
	failures = {}
	for name in a.keys() | b.keys() | rules.keys():
		left, right = a.get(name, (0, 0)), b.get(name, (0, 0))
		pin = rules.get(name)
		if pin is None:
			ok = left == right
		else:
			ok = (right[0] - left[0] == pin['fixed'] + pin['per_block'] * blocks
				and [left[1], right[1]] == pin['calls'])
		if not ok: failures[name] = {'baseline': left, 'candidate': right, 'pin': pin}
	return failures


def evaluate(records, builds, large=False):
	verify_pair(builds)
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	sizes = (10000000, 100000000) if large else (4096, 65536)
	cases = {(k.split('/')[0], k.split('/')[1]) for k in POLICY['rules'] if k.endswith('/large' if large else '/small')}
	expected = {(k, l, n, role) for k, l in cases for n in sizes for role in ('baseline', 'candidate')}
	if len(records) != len(index) or set(index) != expected:
		raise ValueError('incomplete or duplicate matrix')
	failures = []
	for k, l in sorted(cases):
		for n in sizes:
			a, b = (index[k, l, n, role] for role in ('baseline', 'candidate'))
			if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
				raise ValueError('fixture or call mismatch')
			for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'])):
				diff = check(normalized(ac), normalized(bc), k, l, n, call)
				if diff or ac['accounting']['other_Ir'] != bc['accounting']['other_Ir']:
					failures.append(dict(kind=k, layout=l, rows=n, call=call, differences=diff))
	return {'status': 'FAIL' if failures else 'PASS', 'failures': failures}
