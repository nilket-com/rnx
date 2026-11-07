"""ADR 0223 small diagnostic: no automatic protected-path exemptions."""
import itertools
from boolean_vectors import vector, compare, LAYOUTS
from predicate_fixtures import SPECS

PROTECTED = ('booland', 'boolor', 'boolchain', 'booleq', 'boolxor', 'f64chain', 'multiply3', 'i64add', 'axpy', 'f64const', 'f64add', 'boolnot')


def evaluate(records, builds, sizes=(4096, 65536)):
	from predicate_pins import verify_pair, check_vectors
	verify_pair(builds)
	if tuple(sizes) not in ((4096, 65536), (10000000, 100000000)):
		raise ValueError('unreviewed sizes')
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	expected = set(itertools.product((*PROTECTED, *SPECS), LAYOUTS, sizes, ('baseline', 'candidate')))
	if len(index) != len(records) or set(index) != expected: raise ValueError('incomplete or duplicate matrix')
	failures, comparisons = [], []
	for kind, layout, rows in itertools.product((*PROTECTED, *SPECS), LAYOUTS, sizes):
		a, b = (index[kind, layout, rows, role] for role in ('baseline', 'candidate'))
		if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
			raise ValueError('fixture or call mismatch')
		for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'], strict=True)):
			av, bv = vector(ac), vector(bc)
			diff = compare(av, bv)
			case = dict(kind=kind, layout=layout, rows=rows, call=call)
			entry = 'column::run::predicate::run::'
			hits = sum(v[1] for f, v in bv.items() if entry in f)
			prior = sum(v[1] for f, v in av.items() if entry in f)
			if kind in PROTECTED:
				unreviewed = check_vectors(av, bv, kind)
				if unreviewed or ac['accounting']['other_Ir'] != bc['accounting']['other_Ir'] or hits:
					failures.append(dict(case, rule='protected exact vector', differences=unreviewed))
			elif hits <= 0 or prior:
				failures.append(dict(case, rule='live predicate driver witness'))
			comparisons.append(dict(case, differences=diff, driver_calls=hits,
				baseline_Ir_per_row=ac['Ir_per_row'], candidate_Ir_per_row=bc['Ir_per_row'],
				program_delta=bc['accounting']['rnx_Ir']-ac['accounting']['rnx_Ir'],
				system_delta=bc['accounting']['dynamic_Ir']-ac['accounting']['dynamic_Ir']))
	if tuple(sizes) == (10000000, 100000000):
		from predicate_limits import limits
		failures.extend(limits(index, PROTECTED, LAYOUTS, sizes))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons)
