"""ADR0215 reviewed fixed preparation costs; no per-row or per-block allowance."""
import json
from pathlib import Path

POLICY = json.loads(Path(__file__).with_name('0215_constants.json').read_text())
KINDS = {'i64add', 'axpy', 'f64const', 'f64add', 'multiply3', 'boolnot'}
LAYOUTS = {'flat', 'alternating', 'deterministic-random', 'sliced-chunked-null'}


def evaluate(records, builds, sizes=(4096, 65536), wrappers=False):
	kinds = {"add-right", "add-left", "sub-right", "sub-left", "float-generic", "float-sub", "float-mul"} if wrappers else KINDS
	layouts = {"benign", "adversarial"} if wrappers else LAYOUTS
	if tuple(sizes) not in ((4096, 65536), (10000000, 100000000)):
		raise ValueError('unreviewed size domain')
	pins = dict(POLICY['baselines'], candidate=POLICY['candidate'])
	roles = {r['role']: r for r in builds['builds']}
	if len(roles) != len(builds['builds']) or set(roles) != set(pins):
		raise ValueError('missing/duplicated build role')
	if any(roles[k]['source_commit'] != v for k, v in pins.items()):
		raise ValueError('unreviewed pair')
	expected = {(k, n, l, role) for k in kinds for n in sizes for l in layouts for role in pins}
	index = {(r['kind'], r['rows'], r['layout'], r['role']): r for r in records}
	if len(records) != len(expected) or set(index) != expected:
		raise ValueError('incomplete/duplicated matrix')
	failures, comparisons = [], []
	for kind in sorted(kinds):
		for layout in sorted(layouts):
			for role in POLICY['baselines']:
				entry = POLICY['entries'].get('/'.join((role, kind, layout)))
				for n in sizes:
					a, b = index[kind, n, layout, role], index[kind, n, layout, 'candidate']
					if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
						raise ValueError('fixture or first/warm mismatch')
					for call in range(2):
						x, y = (z['costs'][call]['accounting'] for z in (a, b))
						if any(type(z['rnx_Ir']) is not int or z['rnx_Ir'] <= 0 for z in (x, y)):
							raise ValueError('nonpositive program work')
						delta = y['rnx_Ir'] - x['rnx_Ir']
						vector = [y['rnx_functions'].get(f, 0) - x['rnx_functions'].get(f, 0) for f in POLICY['functions']]
						cap = entry['cap'] if entry else 0
						passed = delta <= cap and (not entry or vector == entry['vector'])
						# Unidentified objects receive no allowance.
						passed &= sum(y['other_functions'].values()) == sum(x['other_functions'].values())
						row = dict(kind=kind, layout=layout, rows=n, call=call, baseline_role=role, delta=delta, cap=cap, vector=vector)
						comparisons.append(row)
						if not passed:
							failures.append(row)
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons,
		sizes=list(sizes), approval=POLICY['approval'], scope='program/unknown partitions only; system bands and large ceilings separate')
