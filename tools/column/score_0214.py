"""ADR 0214 targets: no protected program growth; constant chains <=32 and <=80%."""
from copy import deepcopy
from ir_accounting import evaluate as object_evaluate

KINDS = {'add-right', 'add-left', 'sub-right', 'sub-left'}
SIZES = {10000000, 100000000}
LAYOUTS = {'benign', 'adversarial'}
BASELINE = '49eb4453cf333e926357ecd71448175449d5a07c'
CANDIDATE = '019d729e2944b0baf4fd573274ab49bad6d7bf63'


def constant_targets(records):
	expected = {(k, n, l, r) for k in KINDS for n in SIZES for l in LAYOUTS for r in ('baseline', 'candidate')}
	index = {(x['kind'], x['rows'], x['layout'], x['role']): x for x in records}
	if len(records) != len(expected) or set(index) != expected:
		raise ValueError('constant matrix missing, duplicated, or unexpected case')
	failures, comparisons = [], []
	for kind, n, layout, role in sorted(expected):
		if role != 'candidate':
			continue
		a, b = index[kind, n, layout, 'baseline'], index[kind, n, layout, 'candidate']
		if a['fixture_sha256'] != b['fixture_sha256']:
			raise ValueError('same-expression fixture mismatch')
		if len(a['costs']) != 2 or len(b['costs']) != 2:
			raise ValueError('first/warm pair required')
		for call, (before, after) in enumerate(zip(a['costs'], b['costs'], strict=True)):
			x, y = before['Ir'], after['Ir']
			if type(x) is not int or type(y) is not int or min(x, y) <= 0:
				raise ValueError('positive integer instruction counts required')
			row = dict(kind=kind, rows=n, layout=layout, call=call, baseline_Ir=x, candidate_Ir=y)
			comparisons.append(row)
			if y > 32 * n:
				failures.append(dict(row, rule='constant whole-map <=32 Ir/row'))
			if 5 * y > 4 * x:
				failures.append(dict(row, rule='constant whole-map <=80% baseline'))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons)


def protected_targets(records, policy, whole_map_evaluator):
	kinds = {'i64add', 'axpy', 'multiply3', 'f64const', 'f64add', 'boolnot'}
	layouts = {'flat', 'sliced-chunked-null', 'alternating', 'deterministic-random'}
	expected = {(k, n, l, r) for k in kinds for n in SIZES for l in layouts for r in ('baseline', 'candidate')}
	index = {(x['kind'], x['rows'], x['layout'], x['role']): x for x in records}
	if len(records) != len(expected) or set(index) != expected:
		raise ValueError('protected matrix missing, duplicated, or unexpected case')
	for k, n, l, r in expected:
		a = index[k, n, l, r]
		if len(a['costs']) != 2 or a['fixture_sha256'] != index[k, n, l, 'baseline']['fixture_sha256']:
			raise ValueError('protected first/warm or fixture mismatch')
	approved = deepcopy(policy)
	approved['system_band_scope'] = {k: {l: 'unchanged' for l in layouts} for k in kinds}
	def ceilings(rows, rules):
		raw = whole_map_evaluator(rows, rules)
		# This record changes none of these kernels. Preserve all other ceilings.
		failures = [f for f in raw['failures'] if f['rule'] != 'strict nullable improvement']
		return dict(raw, failures=failures, status='FAIL' if failures else 'PASS')
	return object_evaluate(records, approved, ceilings)


def evaluate(constants, protected, builds, policy, whole_map_evaluator):
	if (builds.get('baseline'), builds.get('candidate')) != (BASELINE, CANDIDATE):
		raise ValueError('unreviewed build pair')
	roles = {r['role']: r for r in builds.get('builds', [])}
	if len(builds.get('builds', [])) != 2 or set(roles) != {'baseline', 'candidate'}:
		raise ValueError('missing or duplicated build role')
	if any(roles[role]['source_commit'] != pin for role, pin in [('baseline', BASELINE), ('candidate', CANDIDATE)]):
		raise ValueError('build role source mismatch')
	a = constant_targets(constants['records'])
	b = protected_targets(protected['records'], policy, whole_map_evaluator)
	return dict(status='PASS' if a['status'] == b['status'] == 'PASS' else 'FAIL',
		constants=a, protected=b, baseline=BASELINE, candidate=CANDIDATE, approval='rune-native27834/27850')


def main():
	import argparse
	import hashlib
	import json
	from pathlib import Path
	import sys
	p = argparse.ArgumentParser(description=__doc__)
	for name in ['constants', 'protected', 'builds', 'tooling', 'policy', 'out']:
		p.add_argument('--' + name, type=Path, required=True)
	a = p.parse_args()
	if a.out.exists():
		raise ValueError('new scoring receipt required')
	inputs = {k: getattr(a, k) for k in ['constants', 'protected', 'builds', 'policy']}
	d = {k: json.loads(v.read_text()) for k, v in inputs.items()}
	for row in d['builds']['builds']:
		if hashlib.sha256(Path(row['binary']).read_bytes()).hexdigest() != row['sha256']:
			raise ValueError('binary digest mismatch')
	for capture in [d['constants'], d['protected']]:
		for record in capture['records']:
			for cost in record['costs']:
				if hashlib.sha256(Path(cost['profile']).read_bytes()).hexdigest() != cost['sha256']:
					raise ValueError('profile digest mismatch')
	if d['protected']['identity']['builds'] != hashlib.sha256(a.builds.read_bytes()).hexdigest():
		raise ValueError('protected capture build identity mismatch')
	sys.path.insert(0, str(a.tooling / 'tools/xtask/column'))
	import cli_rules
	r = evaluate(**d, whole_map_evaluator=cli_rules.evaluate)
	r['input_sha256'] = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in inputs.items()}
	r['scorer_sha256'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest()
	a.out.write_text(json.dumps(r, indent=2) + '\n')
	print(r['status'], a.out)
	return 0 if r['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
