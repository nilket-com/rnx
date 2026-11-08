"""ADR 0233 exact reviewed setup and F64-driver block costs, tied to a binary pair."""
import json
import re
from pathlib import Path

POLICY = json.loads(Path(__file__).with_name('0233_pins.json').read_text())


def verify_pair(builds):
	fields = ('source_commit', 'sha256')
	actual = {b['role']: {k: b[k] for k in fields} for b in builds}
	expected = {b['role']: {k: b[k] for k in fields} for b in POLICY['builds']}
	if len(builds) != 2 or actual != expected:
		raise ValueError('ADR0233 measured pair mismatch')


def check_vectors(before, after, kind, layout, call, rows):
	if type(rows) is not int or rows <= 0:
		raise ValueError('invalid row count')
	pins = POLICY['rules'][f'{kind}/{layout}/{call}']
	blocks = (rows + POLICY['block_rows'] - 1) // POLICY['block_rows']
	failures = {}
	for name in before.keys() | after.keys() | pins.keys():
		a, b = before.get(name, (0, 0)), after.get(name, (0, 0))
		pin = pins.get(name)
		if pin is None:
			if a != b:
				failures[name] = dict(baseline=a, candidate=b)
		else:
			expected = pin['constant_Ir'] + pin['per_block_Ir'] * blocks
			if b[0] - a[0] != expected or [a[1], b[1]] != pin['calls']:
				failures[name] = dict(expected_Ir=expected, actual_Ir=b[0]-a[0],
					expected_calls=pin['calls'], actual_calls=[a[1], b[1]])
	return failures



def unknown_signature(functions):
	"""Preserve every cost and relative address within the anonymous JIT image."""
	addresses = {}
	named = {}
	for name, cost in functions.items():
		match = re.fullmatch(r'\?\?\?::0x([0-9a-fA-F]+)', name)
		if match:
			addresses[int(match[1], 16)] = cost
		else:
			named[name] = cost
	base = min(addresses, default=0)
	return named, {address-base: cost for address, cost in addresses.items()}


def evaluate(records, builds):
	"""Protected-program gate only; changed-path and system gates are separate."""
	from boolean_vectors import vector
	verify_pair(builds)
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	sizes = {r['rows'] for r in records}
	kinds = {k.split('/')[0] for k in POLICY['rules']} | {'i64eq'}
	if sizes == {10000000}:
		kinds = {'f64gt', 'f64le', 'arithcmp', 'f64and', 'mixedor'}
	elif sizes != {4096, 65536}:
		raise ValueError('unreviewed protected matrix sizes')
	layouts = {k.split('/')[1] for k in POLICY['rules']}
	expected = {(k, l, n, role) for k in kinds for l in layouts for n in sizes
		for role in ('baseline', 'candidate')}
	if len(index) != len(records) or set(index) != expected:
		raise ValueError('missing, duplicate or unexpected case')
	failures, checked = [], []
	for kind, layout, rows, role in sorted(index):
		if role != 'baseline' or kind == 'i64eq':
			continue
		a, b = index[kind, layout, rows, role], index[kind, layout, rows, 'candidate']
		if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
			raise ValueError('fixture or call mismatch')
		for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'], strict=True)):
			diff = check_vectors(vector(ac), vector(bc), kind, layout, call, rows)
			unknown = unknown_signature(ac['accounting']['other_functions']) != unknown_signature(bc['accounting']['other_functions'])
			case = dict(kind=kind, layout=layout, rows=rows, call=call)
			checked.append(case)
			if diff or unknown:
				failures.append(dict(case, differences=diff, unknown_partition_changed=unknown))
	if not checked:
		raise ValueError('empty protected capture')
	return dict(status='FAIL' if failures else 'PASS', failures=failures, checked=checked,
		scope='Protected program and unknown partitions only; no changed-path or system acceptance inferred.')
