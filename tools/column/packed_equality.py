"""ADR 0226 exact unchanged vectors and raw packed-Boolean targets."""
import itertools
import json
from pathlib import Path
from boolean_vectors import vector, compare, LAYOUTS
from predicate_vectors import PROTECTED
from predicate_fixtures import SPECS
from predicate_limits import CEILINGS
from boolean_limits import limits as prior_limits

POLICY = json.loads(Path(__file__).with_name('0226_pins.json').read_text())
NEW = ('booleq', 'boolne', 'boolxor')
CHANGED = ('booland', 'boolor', 'boolchain', *NEW)
KINDS = (*PROTECTED, *SPECS, 'boolne')
EXACT = tuple(k for k in KINDS if k not in CHANGED)


def verify_pair(builds):
	actual = {b['role']: {k: b[k] for k in ('source_commit', 'sha256')} for b in builds}
	expected = {b['role']: {k: b[k] for k in ('source_commit', 'sha256')} for b in POLICY['builds']}
	if len(builds) != 2 or actual != expected:
		raise ValueError('ADR0226 measured pair mismatch')


def check_vectors(a, b, kind):
	pins = POLICY['rules'][kind]
	failures = {}
	for name in a.keys() | b.keys() | pins.keys():
		left, right = a.get(name, (0, 0)), b.get(name, (0, 0))
		pin = pins.get(name)
		if pin is None:
			if left != right:
				failures[name] = dict(baseline=left, candidate=right)
		elif right[0] - left[0] != pin['constant_Ir'] or [left[1], right[1]] != pin['calls']:
			failures[name] = dict(expected=pin, actual_delta=right[0]-left[0], actual_calls=[left[1], right[1]])
	return failures


def changed_rules(kind, layout, baseline, candidate, rows):
	cap = (2.5 if layout == 'flat' else 5) if kind == 'boolchain' else (2 if layout == 'flat' else 4)
	failures = []
	if candidate > cap * rows:
		failures.append('Boolean whole-map ceiling')
	if kind in NEW and candidate * 10 > baseline:
		failures.append('Boolean baseline ratio')
	return failures


def evaluate(records, builds, sizes=(4096, 65536)):
	verify_pair(builds)
	if tuple(sizes) not in ((4096, 65536), (10000000, 100000000)):
		raise ValueError('unreviewed sizes')
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	expected = set(itertools.product(KINDS, LAYOUTS, sizes, ('baseline', 'candidate')))
	if len(index) != len(records) or set(index) != expected:
		raise ValueError('incomplete or duplicate matrix')
	failures, comparisons = [], []
	for kind, layout, rows in itertools.product(KINDS, LAYOUTS, sizes):
		a, b = (index[kind, layout, rows, role] for role in ('baseline', 'candidate'))
		if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
			raise ValueError('fixture or call mismatch')
		for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'], strict=True)):
			for cost in (ac, bc):
				if type(cost['Ir']) is not int or cost['Ir'] <= 0 or cost['Ir_per_row'] != cost['Ir'] / rows:
					raise ValueError('invalid whole-map total')
			av, bv = vector(ac), vector(bc)
			case = dict(kind=kind, layout=layout, rows=rows, call=call)
			hits = sum(v[1] for f, v in bv.items() if 'boolean::run::' in f)
			prior = sum(v[1] for f, v in av.items() if 'boolean::run::' in f)
			if kind in EXACT:
				diff = check_vectors(av, bv, kind)
				if diff or hits or ac['accounting']['other_Ir'] != bc['accounting']['other_Ir']:
					failures.append(dict(case, rule='protected exact vector', differences=diff))
			else:
				if hits <= 0 or (kind in NEW and prior != 0) or (kind not in NEW and prior <= 0):
					failures.append(dict(case, rule='live packed driver witness'))
				if sizes[0] == 10000000:
					failures.extend(dict(case, rule=rule) for rule in changed_rules(kind, layout, ac['Ir'], bc['Ir'], rows))
			comparisons.append(dict(case, differences=compare(av, bv), driver_calls=hits,
				baseline_Ir_per_row=ac['Ir_per_row'], candidate_Ir_per_row=bc['Ir_per_row'],
				program_delta=bc['accounting']['rnx_Ir']-ac['accounting']['rnx_Ir'],
				system_delta=bc['accounting']['dynamic_Ir']-ac['accounting']['dynamic_Ir']))
	if sizes[0] == 10000000:
		# Prior numeric ceilings and unchanged-system bands; changed Bool paths
		# use raw ceilings above, with no system-library allowance.
		failures.extend(prior_limits(index, EXACT, (), LAYOUTS, sizes))
		for kind, layout, rows, call in itertools.product(('f64chain', *SPECS), LAYOUTS, sizes, range(2)):
			cap = (22 if layout in ('flat', 'sliced-chunked-null') else 30) if kind == 'f64chain' else CEILINGS[kind]
			actual = index[kind, layout, rows, 'candidate']['costs'][call]['Ir_per_row']
			if actual > cap:
				failures.append(dict(kind=kind, layout=layout, rows=rows, call=call, rule='retained ceiling', actual=actual, ceiling=cap))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons)
