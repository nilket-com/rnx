"""ADR 0233 portable diagnostic: exact redistribution, six fixed Ir per map."""
PREFIX = 'rune::experimental_core::column::run::predicate::'
DRIVER = PREFIX + 'run::<&mut column_force::never_canceled>'
OLD = PREFIX + 'numeric_comparison'
NEW = PREFIX + 'i64_comparison_scalar'
PAIR = {
	'baseline': ('845572cb5677241db3ce2b75e78cf81e265c81b7', 'f55c3dba5ef35a0d66cfb3be72628a8212320359c3f06a1dc5e8c65709481064'),
	'candidate': ('070006e22a9aca79aba6e1ffc3121677326aa2ed', '8ff2ce06c39fe353b93260af62fae904d0520d98feed9f09fe575d08befd7fa5'),
}


def check(before, after, rows, temperature):
	if rows not in (4096, 65536) or temperature not in ('first', 'warm'):
		raise ValueError('unreviewed portable case')
	blocks = rows // 1024
	# Callgrind retains the uncollected warmup's call counts, but not its Ir.
	calls = 1 if temperature == 'first' else 2
	expected = {OLD: ((30 * blocks, calls * blocks), (0, 0)),
		NEW: ((0, 0), (28 * blocks, calls * blocks))}
	failures = {}
	for name in before.keys() | after.keys() | {OLD, NEW, DRIVER}:
		a, b = before.get(name, (0, 0)), after.get(name, (0, 0))
		if name == DRIVER:
			ok = b[0] - a[0] == 6 + 2 * blocks and a[1] == b[1] == calls
		elif name in expected:
			ok = (tuple(a), tuple(b)) == expected[name]
		else:
			ok = a == b
		if not ok:
			failures[name] = dict(baseline=a, candidate=b)
	if sum(x[0] for x in after.values()) - sum(x[0] for x in before.values()) != 6:
		failures['total'] = 'not exactly six fixed instructions'
	return failures


def evaluate(result):
	from boolean_vectors import vector
	if result.get('capture_status') != 'PASS':
		raise ValueError('capture did not pass')
	records = result['records']
	index = {(r['operator'], r['rows'], r['temperature'], r['build']['role']): r for r in records}
	expected = {(op, n, t, role) for op in ('==', '!=', '<', '<=', '>', '>=')
		for n in (4096, 65536) for t in ('first', 'warm') for role in PAIR}
	if len(records) != len(index) or set(index) != expected:
		raise ValueError('incomplete or duplicate portable matrix')
	for r in records:
		b = r['build']
		if (b['source_commit'], b['sha256']) != PAIR[b['role']]:
			raise ValueError('portable measured pair mismatch')
	failures = []
	for op, n, t, role in sorted(index):
		if role != 'baseline':
			continue
		a, b = (index[op, n, t, role]['cost'] for role in ('baseline', 'candidate'))
		differences = check(vector(a), vector(b), n, t)
		if a['accounting']['other_Ir'] or b['accounting']['other_Ir']:
			differences['unknown'] = 'unexpected unknown costs'
		if differences:
			failures.append(dict(operator=op, rows=n, temperature=t, differences=differences))
	return dict(status='FAIL' if failures else 'PASS', failures=failures,
		scope='Portable program/unknown instructions only; exactly +6 Ir/map, zero net slope.')
