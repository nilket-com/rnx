"""ADR0222 small diagnostic: exact protected vectors, no automatic exemptions."""
import itertools
import re
from boolean_vectors import vector, compare, LAYOUTS

KINDS = ('booland', 'boolor', 'boolchain', 'booleq', 'boolxor', 'f64chain', 'multiply3', 'i64add', 'axpy', 'f64const', 'f64add', 'boolnot')


def evaluate(records, builds, sizes=(4096, 65536)):
	from f64_output_pins import verify_pair, check_vectors
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
				if hits <= 0 or prior <= 0:
					failures.append(dict(case, rule='live F64 driver witness'))
				if bc['Ir_per_row'] > ac['Ir_per_row']:
					failures.append(dict(case, rule='F64 whole-map nonpositive'))
			else:
				unreviewed, _ = check_vectors(av, bv, kind, layout, rows)
				if unreviewed or ac['accounting']['other_Ir'] != bc['accounting']['other_Ir'] or hits:
					failures.append(dict(case, rule='protected exact vector', differences=unreviewed))
			comparisons.append(dict(case, differences=diff, driver_calls=hits,
				baseline_Ir_per_row=ac['Ir_per_row'], candidate_Ir_per_row=bc['Ir_per_row'],
				program_delta=bc['accounting']['rnx_Ir']-ac['accounting']['rnx_Ir'],
				system_delta=bc['accounting']['dynamic_Ir']-ac['accounting']['dynamic_Ir']))
	from f64_output_edges import check
	failures.extend(check(records))
	if tuple(sizes) == (10000000, 100000000):
		from f64_output_limits import limits
		failures.extend(limits(index, KINDS, LAYOUTS, sizes))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons)


from f64_plan_vectors import executed_shapes
