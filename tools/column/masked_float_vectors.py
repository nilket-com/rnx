"""ADR 0229: exact pair pins, including reviewed REP-store timing exceptions."""
import hashlib
import json
from pathlib import Path
from boolean_vectors import vector

POLICY = Path(__file__).with_name('0229_pins.json')
CHANGED = {'f64const', 'f64add', 'f64sub', 'f64mul'}


def changed(kind, layout):
	return kind in CHANGED and layout != 'flat'


def seal(a, b):
	return hashlib.sha256(json.dumps([a, b], sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def check_fixed(a, b, rules):
	fail = {}
	for name in a.keys() | b.keys() | rules.keys():
		left, right = a.get(name, (0, 0)), b.get(name, (0, 0))
		pin = rules.get(name)
		ok = left == right if pin is None else (right[0]-left[0] == pin['delta'] and [left[1], right[1]] == pin['calls'])
		if not ok: fail[name] = dict(baseline=left, candidate=right, pin=pin)
	return fail


def check_faults(calls, policy):
	keys = ('kind','layout','call','baseline_minor','candidate_minor')
	return [{k:c[k] for k in keys} for c in calls] == policy['fault_deltas']


def rep_exception(ac, bc, pin):
	"""Only the approved 10M sliced-null captures may substitute timing for Ir."""
	for side, cost in (('baseline', ac), ('candidate', bc)):
		if seal(cost['accounting'], {}) != pin[side + '_accounting']: return False
		profile = Path(cost['profile'])
		if hashlib.sha256(profile.read_bytes()).hexdigest() != pin[side + '_profile']: return False
	return True


def evaluate(records, builds, policy=None):
	policy = policy or json.loads(POLICY.read_text())
	assert [{k:b[k] for k in ('role','source_commit','sha256')} for b in builds] == policy['builds'], 'binary pair mismatch'
	index = {(x['kind'],x['layout'],x['rows'],x['role']):x for x in records}
	assert len(index) == len(records), 'duplicate case'
	actual = {f'{k}/{l}/{n}' for k,l,n,_ in index}
	expected = set(policy['cases'])
	assert actual == expected and len(records) == len(expected)*2, 'missing or extra case'
	failures, table = [], []
	for case in sorted(expected):
		kind, layout, n = case.split('/'); n = int(n)
		a,b = (index[kind,layout,n,role] for role in ('baseline','candidate'))
		assert a['fixture_sha256'] == b['fixture_sha256'] and len(a['costs']) == len(b['costs']) == 2
		for call,(ac,bc) in enumerate(zip(a['costs'],b['costs'])):
			av,bv = vector(ac),vector(bc)
			aa,bb = ac['accounting'],bc['accounting']
			if changed(kind,layout):
				key = f'{case}/{call}'
				pin = policy.get('rep_store_exceptions', {}).get(key)
				cost_ok = bb['total_Ir'] <= aa['total_Ir']
				if pin is not None:
					cost_ok = layout == 'sliced-chunked-null' and n == 10000000 and rep_exception(ac,bc,pin)
				ok = cost_ok and seal(av,bv) == policy['changed_vectors'][key]
				if not ok: failures.append(dict(case=case,call=call,rule='changed total or exact capture vector'))
			else:
				diff = check_fixed(av,bv,policy['fixed'][f'{kind}/{layout}/{call}'])
				if diff or aa['other_Ir'] != bb['other_Ir']: failures.append(dict(case=case,call=call,rule='unchanged fixed-only',differences=diff))
			table.append(dict(case=case,call=call,changed=changed(kind,layout),
				baseline={k:aa[k] for k in ('rnx_Ir','dynamic_Ir','other_Ir','total_Ir')},
				candidate={k:bb[k] for k in ('rnx_Ir','dynamic_Ir','other_Ir','total_Ir')}))
	return dict(status='FAIL' if failures else 'PASS',failures=failures,comparisons=table)
