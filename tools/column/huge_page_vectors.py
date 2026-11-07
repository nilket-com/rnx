"""ADR 0227 exact pair and function pins, including partial final blocks."""
from functools import lru_cache
import itertools
import json
from pathlib import Path
from boolean_vectors import vector, compare, LAYOUTS
from packed_equality import KINDS

POLICY = json.loads(Path(__file__).with_name('0227_pins.json').read_text())


def verify_pair(builds):
	actual = [{k: b[k] for k in ('role', 'source_commit', 'sha256')} for b in builds]
	if sorted(actual, key=lambda b: b['role']) != sorted(POLICY['builds'], key=lambda b: b['role']):
		raise ValueError('ADR0227 measured pair mismatch')


@lru_cache(None)
def execute_counts(rows, layout):
	"""Count calls from fixture chunk boundaries and nonempty validity words."""
	if layout == 'flat':
		return ((rows + 1023) // 1024, 0)
	if layout == 'sliced-chunked-null':
		return ((rows // 2 + 63) // 64, 0)
	if layout not in LAYOUTS or rows < 8:
		raise ValueError('invalid fixture')
	present = [i % 2 == 0 if layout == 'alternating' else
		all((i * 73 + a * 41 + 19) % 251 < 127 for a in range(2)) for i in range(256)]
	prefix = [0]
	for bit in present * 2:
		prefix.append(prefix[-1] + bit)
	boundaries = sorted({0, rows, rows // 3 + 3, rows // 3 + 5, rows - 10, rows - 12})
	all_calls = mixed_calls = 0
	for start, end in zip(boundaries, boundaries[1:]):
		# 1024 is a multiple of 64: only physical chunk boundaries reset words.
		for row in range(start, end, 64):
			n = min(64, end - row); offset = row % 256
			valid = prefix[offset + n] - prefix[offset]
			all_calls += valid == n
			mixed_calls += 0 < valid < n
	return all_calls, mixed_calls


def expected(pin, rows, layout):
	if pin.get('formula') == 'execute':
		whole, mixed = execute_counts(rows, layout)
		return whole - 2 * mixed, [whole + mixed] * 2
	if pin.get('formula') == 'pair':
		calls = (rows + 1023) // 1024
		return -4 * calls, [calls] * 2
	return pin['constant_Ir'], pin['calls']


def size_class(kind, layout, rows):
	from predicate_fixtures import SPECS
	bitmap = (rows + 7) // 8
	data = bitmap if kind.startswith('bool') or kind in SPECS else rows * 8
	return f"data{int(data >= 4 * 1024 * 1024)}-validity{int(layout != 'flat' and bitmap >= 4 * 1024 * 1024)}"


def rules_for(kind, layout, rows, call):
	rules = dict(POLICY['rules'][kind + '/' + layout])
	key = f"{kind}/{layout}/{call}/{size_class(kind, layout, rows)}"
	rules.update(POLICY.get('large_rules', {}).get(key, {}))
	return rules


def check_vectors(a, b, kind, layout, rows, call=0):
	rules = rules_for(kind, layout, rows, call)
	failures = {}
	for name in a.keys() | b.keys() | rules.keys():
		left, right = a.get(name, (0, 0)), b.get(name, (0, 0))
		pin = rules.get(name)
		if pin is None:
			if left != right:
				failures[name] = dict(baseline=left, candidate=right)
		else:
			delta, calls = expected(pin, rows, layout)
			if right[0] - left[0] != delta or [left[1], right[1]] != calls:
				failures[name] = dict(expected_delta=delta, expected_calls=calls,
					actual_delta=right[0]-left[0], actual_calls=[left[1], right[1]])
	return failures


def evaluate(records, builds, sizes=(4096, 65536)):
	verify_pair(builds)
	if tuple(sizes) not in ((4096, 65536), (10000000, 100000000)):
		raise ValueError('unreviewed sizes')
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	keys = set(itertools.product(KINDS, LAYOUTS, sizes, ('baseline', 'candidate')))
	if len(index) != len(records) or set(index) != keys:
		raise ValueError('incomplete or duplicate matrix')
	failures, comparisons = [], []
	for kind, layout, rows in itertools.product(KINDS, LAYOUTS, sizes):
		a, b = (index[kind, layout, rows, role] for role in ('baseline', 'candidate'))
		if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
			raise ValueError('fixture or call mismatch')
		for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'], strict=True)):
			av, bv = vector(ac), vector(bc)
			case = dict(kind=kind, layout=layout, rows=rows, call=call)
			diff = check_vectors(av, bv, kind, layout, rows, call)
			if diff or ac['accounting']['other_Ir'] != bc['accounting']['other_Ir']:
				failures.append(dict(case, rule='exact function vector', differences=diff))
			comparisons.append(dict(case, differences=compare(av, bv),
				program_delta=bc['accounting']['rnx_Ir']-ac['accounting']['rnx_Ir']))
	if sizes[0] == 10000000:
		from boolean_limits import limits
		failures.extend(limits(index, KINDS, (), LAYOUTS, sizes))
		from predicate_limits import CEILINGS
		for kind, layout, rows, call in itertools.product(KINDS, LAYOUTS, sizes, range(2)):
			cap = CEILINGS.get(kind)
			if kind.startswith('bool') and kind != 'boolnot':
				cap = (2.5 if layout == 'flat' else 5) if kind == 'boolchain' else (2 if layout == 'flat' else 4)
			if kind == 'f64chain':
				cap = 22 if layout in ('flat', 'sliced-chunked-null') else 30
			actual = index[kind, layout, rows, 'candidate']['costs'][call]['Ir_per_row']
			if cap is not None and actual > cap:
				failures.append(dict(kind=kind, layout=layout, rows=rows, call=call,
					rule='retained raw ceiling', actual=actual, maximum=cap))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons)
