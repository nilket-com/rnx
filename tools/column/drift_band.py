"""Source-conditioned integer drift bands. Historical exact scorers remain intact."""
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

from child_result import digest, identity

POLICY = Path(__file__).with_name('fixed_route_policy.json')
INITIAL_ANCHOR = '3fe44bf9245c780cf784554293e0a1b1e7c00c26'


def git(root, *args):
	return subprocess.check_output(['git', '-C', str(root), *args])


def validate_policy(policy):
	from old_path_programs import KERNELS
	if policy['schema'] != 1 or set(policy['kernels']) != set(KERNELS):
		raise ValueError('policy schema/kernel set changed: review required')
	if policy['modes'] != ['off', 'on'] or policy['sizes'] != [1, 5]:
		raise ValueError('policy matrix changed: review required')
	if policy['per_record_per_mille'] != 5 or policy['cumulative_per_mille'] != 10:
		raise ValueError('policy limits changed: review required')
	items = policy['inventory']
	if not items or len({identity(v) for v in items}) != len(items):
		raise ValueError('empty/duplicate source inventory')
	for item in items:
		if set(item) != {'file', 'kind', 'receiver', 'name'} or item['kind'] not in {'struct', 'method', 'fn'}:
			raise ValueError('invalid source selector')
		if Path(item['file']).is_absolute() or '..' in Path(item['file']).parts:
			raise ValueError('source outside repository')


def anchor_transition(policy, previous):
	"""Validate the link before checking the plan's cryptographic/ancestry proof."""
	old = INITIAL_ANCHOR if previous is None else previous['anchor']
	new = policy['anchor']; approval = policy['anchor_transition']
	if new == old:
		if previous is None:
			if approval is not None: raise ValueError('unexpected bootstrap transition')
		elif approval != previous['anchor_transition']:
			raise ValueError('unchanged anchor changed its approval history')
		return None
	if not isinstance(approval, dict) or set(approval) != {'old', 'new', 'plan', 'path'}:
		raise ValueError('structured approval required')
	if approval['old'] != old or approval['new'] != new:
		raise ValueError('anchor transition skips baseline lineage')
	return approval


def anchor_approval(root, baseline, candidate, policy):
	anchor = policy['anchor']
	if len(anchor) != 40 or git(root, 'rev-parse', anchor + '^{commit}').decode().strip() != anchor:
		raise ValueError('anchor must be a full commit')
	path = 'tools/column/fixed_route_policy.json'
	present = git(root, 'ls-tree', '--name-only', baseline, path).decode().splitlines()
	previous = json.loads(git(root, 'show', baseline + ':' + path)) if present else None
	if previous is None and baseline != INITIAL_ANCHOR: raise ValueError('unreviewed anchor bootstrap')
	approval = anchor_transition(policy, previous)
	if approval is None: return
	plan = approval['plan']
	if len(plan) != 40: raise ValueError('approval plan must be full commit')
	subprocess.run(['git', '-C', str(root), 'merge-base', '--is-ancestor', plan, candidate], check=True)
	if not git(root, 'show', '-s', '--format=%s', plan).decode().startswith('plan: adr '):
		raise ValueError('approval is not a plan commit')
	subprocess.run(['git', '-C', str(root), 'verify-commit', plan], check=True, capture_output=True)
	text = git(root, 'show', plan + ':' + approval['path']).decode()
	for declaration in ('User-approved anchor transition', 'Old anchor: ' + approval['old'], 'New anchor: ' + anchor):
		if declaration not in text: raise ValueError('approval declaration absent')


def source_condition(root, baseline, candidate, policy, guard):
	"""Inventory tokens before identity; bootstrap policy at the initial anchor only."""
	validate_policy(policy)
	path = POLICY.relative_to(Path(__file__).resolve().parents[2]).as_posix()
	old_paths = git(root, 'ls-tree', '--name-only', baseline, path).decode().splitlines()
	if old_paths:
		old = json.loads(git(root, 'show', baseline + ':' + path))
		for field in ('inventory', 'kernels', 'modes', 'sizes', 'per_record_per_mille', 'cumulative_per_mille'):
			if old[field] != policy[field]: raise ValueError('policy inventory/limit change: STOP ' + field)
	elif baseline != INITIAL_ANCHOR:
		raise ValueError('missing baseline policy outside reviewed bootstrap')
	rows = []
	with tempfile.TemporaryDirectory(prefix='fixed-source-') as temp:
		for item in policy['inventory']:
			values = []
			for rev in (baseline, candidate):
				blob = git(root, 'show', rev + ':' + item['file'])
				p = Path(temp) / 'source.rs'; p.write_bytes(blob)
				values.append(subprocess.check_output([str(guard), '--item', str(p), item['kind'], item['receiver'], item['name']], text=True).strip())
			if values[0] != values[1]: raise ValueError('fixed source changed: ' + item['file'] + ':' + item['name'])
			rows.append(dict(selector=item, sha256=hashlib.sha256(values[0].encode()).hexdigest(), canonical=values[0]))
	return {'status': 'PASS', 'baseline': baseline, 'candidate': candidate, 'inventory': identity(policy['inventory']), 'items': rows}


def index(records, policy):
	result = {}
	for row in records:
		key = (row['kind'], row['policy'], row['n'])
		if key in result or row['status'] != 'PASS': raise ValueError('duplicate/failed case')
		a = row['accounting']
		for k in ('rnx_Ir', 'other_Ir', 'dynamic_Ir', 'total_Ir'):
			if type(a[k]) is not int or a[k] < 0: raise ValueError('invalid count')
		if a['total_Ir'] != a['rnx_Ir'] + a['other_Ir'] + a['dynamic_Ir']: raise ValueError('partitions do not reconcile')
		if not row.get('stdout_sha256') or not row.get('fixture_sha256'): raise ValueError('missing output/fixture')
		result[key] = row
	expected = {(k, m, n) for k in policy['kernels'] for m in policy['modes'] for n in policy['sizes']}
	if set(result) != expected: raise ValueError('incomplete or extraneous case matrix')
	return result


def compare(left, right, policy, limit):
	indices = [index(rows, policy) for rows in (left, right)]
	rows = []
	for kind in policy['kernels']:
		for mode in policy['modes']:
			for n in policy['sizes']:
				a, b = [i[kind, mode, n] for i in indices]
				if any(a[field] != b[field] for field in ('stdout_sha256', 'fixture_sha256')):
					raise ValueError('output/fixture mismatch')
			for metric in ('W', 'T'):
				def count(i, n):
					a = indices[i][kind, mode, n]['accounting']
					return a['total_Ir'] if metric == 'T' else a['rnx_Ir'] + a['other_Ir']
				b1, b5, c1, c5 = count(0, 1), count(0, 5), count(1, 1), count(1, 5)
				if b5 <= 0 or b5 <= b1: raise ValueError('invalid baseline denominator')
				level = 1000 * (c5 - b5) <= limit * b5
				growth = 1000 * ((c5 - c1) - (b5 - b1)) <= limit * (b5 - b1)
				rows.append(dict(kind=kind, mode=mode, metric=metric, B1=b1, B5=b5, C1=c1, C5=c5,
					level_pass=level, growth_pass=growth, pass_all=level and growth))
	return rows


def measurement_recipe(current):
	# Only execution inputs determine counts once the rebuilt bytes match.
	return {k: current[k] for k in ('cpu', 'environment', 'run_environment', 'valgrind', 'fixtures', 'policy')} | {
		'tooling': {k:v for k,v in current['tooling'].items() if k != 'tools/column/paired_build.py'}}


def score(baseline, candidate, anchor, policy):
	validate_policy(policy)
	if anchor is None: raise ValueError('anchor evidence required even on identity PASS')
	for receipt in (baseline, candidate, anchor):
		if receipt['status'] != 'PASS': raise ValueError('failed capture')
		if receipt['policy_sha256'] != identity(policy): raise ValueError('stale policy')
		if receipt['recipe'] != candidate['recipe']:
			proof = receipt.get('reuse_proof', {})
			if receipt is not anchor or proof.get('current_recipe') != candidate['recipe']:
				raise ValueError('recipe/toolchain/CPU/environment mismatch')
			if measurement_recipe(receipt['recipe']) != measurement_recipe(candidate['recipe']):
				raise ValueError('anchor measurement inputs mismatch')
			if not receipt.get('binary_sha256') or proof.get('binary_sha256') != receipt['binary_sha256'] or proof.get('current_build', {}).get('sha256') != receipt['binary_sha256']:
				raise ValueError('anchor reuse lacks identical current binary')
	if anchor['revision'] != policy['anchor']: raise ValueError('wrong anchor')
	per_record = compare(baseline['records'], candidate['records'], policy, policy['per_record_per_mille'])
	cumulative = compare(anchor['records'], candidate['records'], policy, policy['cumulative_per_mille'])
	return dict(status='PASS' if all(r['pass_all'] for r in per_record + cumulative) else 'FAIL',
		per_record=per_record, cumulative=cumulative,
		dynamic=[dict(role=role, kind=r['kind'], mode=r['policy'], n=r['n'], Ir=r['accounting']['dynamic_Ir'])
			for role, receipt in [('baseline', baseline), ('candidate', candidate), ('anchor', anchor)] for r in receipt['records']])
