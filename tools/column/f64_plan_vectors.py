"""ADR0221 small diagnostic: exact protected vectors, no automatic exemptions."""
import itertools
import re
from boolean_vectors import vector, compare, LAYOUTS

KINDS = ('booland', 'boolor', 'boolchain', 'booleq', 'boolxor', 'f64chain', 'multiply3', 'i64add', 'axpy', 'f64const', 'f64add', 'boolnot')


def evaluate(records, builds, sizes=(4096, 65536)):
	from f64_plan_pins import verify_pair, check_vectors
	verify_pair(builds)
	assert tuple(sizes) in ((4096, 65536), (10000000, 100000000))
	assert {b['role'] for b in builds} == {'baseline', 'candidate'}
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	assert len(index) == len(records)
	assert set(index) == set(itertools.product(KINDS, LAYOUTS, sizes, ('baseline', 'candidate')))
	failures, comparisons = [], []
	for kind, layout, rows in itertools.product(KINDS, LAYOUTS, sizes):
		a, b = (index[kind, layout, rows, role] for role in ('baseline', 'candidate'))
		assert a['fixture_sha256'] == b['fixture_sha256']
		assert len(a['costs']) == len(b['costs']) == 2
		for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'], strict=True)):
			av, bv = vector(ac), vector(bc)
			case = dict(kind=kind, layout=layout, rows=rows, call=call)
			diff = compare(av, bv)
			entry = 'column::run::f64_plan::run::'
			hits = sum(v[1] for f, v in bv.items() if entry in f)
			prior = sum(v[1] for f, v in av.items() if entry in f)
			if kind == 'f64chain':
				if hits <= 0 or prior:
					failures.append(dict(case, rule='live F64 driver witness'))
			else:
				unreviewed, applied = check_vectors(av, bv, kind, layout, rows)
				if unreviewed or ac['accounting']['other_Ir'] != bc['accounting']['other_Ir'] or hits:
					failures.append(dict(case, rule='protected exact vector', differences=unreviewed))
			comparisons.append(dict(case, differences=diff, driver_calls=hits,
				baseline_Ir_per_row=ac['Ir_per_row'], candidate_Ir_per_row=bc['Ir_per_row'],
				program_delta=bc['accounting']['rnx_Ir']-ac['accounting']['rnx_Ir'],
				system_delta=bc['accounting']['dynamic_Ir']-ac['accounting']['dynamic_Ir']))
	if tuple(sizes) == (10000000, 100000000):
		from f64_plan_limits import limits
		failures.extend(limits(index, KINDS, LAYOUTS, sizes))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons)


def executed_shapes(records, shape):
	"""Every executed lane specialization must have packed arithmetic."""
	functions = {}
	for function in shape['functions']:
		functions.setdefault(function['name'], []).append(function)
	failures, executed = [], set()
	for record in records:
		if record['kind'] != 'f64chain' or record['role'] != 'candidate':
			continue
		for call, cost in enumerate(record['costs']):
			case = dict(layout=record['layout'], rows=record['rows'], call=call)
			hits = {name for name, (ir, calls) in vector(cost).items()
				if 'column::run::f64_plan::lanes::' in name and (ir > 0 or calls > 0)}
			if not hits:
				failures.append(dict(case, rule='missing executed lane specialization'))
			for name in sorted(hits):
				executed.add(name)
				matches = functions.get(name, [])
				if len(matches) != 1:
					failures.append(dict(case, rule='missing or ambiguous lane disassembly', name=name))
				elif matches[0]['packed_arithmetic'] <= 0:
					failures.append(dict(case, rule='executed scalar lane specialization', name=name, assembly=matches[0]['assembly']))
				else:
					from f64_plan_loop import primary_loop
					loop = primary_loop(matches[0]['assembly'])
					op = re.search(r'lanes::<([012]),', name)
					expected = ('addpd', 'subpd', 'mulpd')[int(op[1])] if op else None
					if loop['status'] != 'PASS' or not expected or any(x[1].removeprefix('v') != expected for x in loop.get('packed', [])):
						failures.append(dict(case, rule='primary eight-lane packed loop', name=name, loop=loop))
	if not executed:
		failures.append(dict(rule='no executed F64 lane evidence'))
	return dict(status='FAIL' if failures else 'PASS', failures=failures,
		executed=sorted(executed), unexecuted=sorted(set(functions) - executed))
