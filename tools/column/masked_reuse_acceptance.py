"""ADR 0230: scoped, user-approved output-fault criterion; retain total faults."""
import argparse
import hashlib
import itertools
import json
from pathlib import Path

KINDS = ('f64const', 'f64add')
LAYOUTS = ('alternating', 'deterministic-random', 'sliced-chunked-null')


def check(faults, phases, timing):
	calls = faults['calls']
	assert len(calls) == 18
	assert {(x['kind'], x['layout'], x['call']) for x in calls} == set(itertools.product(KINDS, LAYOUTS, range(3)))
	for x in calls:
		if x['call'] and x['layout'] != 'sliced-chunked-null':
			assert x['candidate_minor'] * 100 <= x['baseline_minor'], 'total warm fault limit'
	assert set(phases) == set(KINDS)
	for kind, events in phases.items():
		pointers = [x for x in events if x['label'] == 'output-first-block']
		assert len(pointers) == 3 and {x['call'] for x in pointers} == {0, 1, 2}
		assert len({x['output_pointer'] for x in pointers}) == 1, 'output moved'
		assert all(x['output_capacity'] == 100000000 for x in pointers)
		for call in (1, 2):
			rows = [x for x in events if x['call'] == call]
			e = {x['label']: x for x in rows}
			assert len(e) == len(rows), 'duplicate phase'
			for a, b in (('run-start', 'run-end'), ('validity-start', 'validity-end'), ('transfer-start', 'transfer-end')):
				assert e[b]['minor'] == e[a]['minor'], 'faults in output/validity/transfer'
			assert e['run-start']['memory_kib']['AnonHugePages'] == e['run-end']['memory_kib']['AnonHugePages']
			assert e['clock-end']['minor'] - e['clock-start']['minor'] == e['validity-start']['minor'] - e['clock-start']['minor'], 'unattributed fault'
	comparisons = timing['comparisons']
	assert len(comparisons) == 12
	assert {(x['kind'], x['layout'], x['mode']) for x in comparisons} == set(itertools.product(KINDS, LAYOUTS, ('first', 'warm')))
	assert timing['status'] == 'PASS'
	assert all(x['ratio'] < 1 for x in comparisons if x['mode'] == 'warm'), 'warm timing target'
	return dict(status='PASS', original_total_fault_policy_pass=faults['policy_pass'], total_faults=calls,
		note='User approved sliced-null zero output-execution faults with stable donation; report total setup faults. Other layouts retain <=1% total warm faults. Original FAIL retained.')


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--root', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args()
	policy = json.loads(Path(__file__).with_name('0230_evidence.json').read_text())
	data = {}
	for name, digest in policy['receipts'].items():
		raw = (a.root / name).read_bytes()
		assert hashlib.sha256(raw).hexdigest() == digest, 'evidence hash mismatch: ' + name
		data[name] = json.loads(raw)
	build = data['BUILD-v1/RESULT.json']
	assert build['head'] == policy['measured_commit']
	assert build['builds'][1]['sha256'] == policy['binary_sha256']
	for name in ('CONTROLS-v1', 'MIRI-v1', 'BUILD-v1', 'SMALL-v1'):
		assert data[name + '/RESULT.json']['status'] == 'PASS'
	result = check(data['FAULTS-v1/RESULT.json'], {k: data['PHASES-v2/' + k + '/PHASES.json'] for k in KINDS}, data['TIMING-EVIDENCE-v1/RESULT.json'])
	result['evidence'] = policy
	a.out.write_text(json.dumps(result, indent=2) + '\n')
	print('PASS', a.out)


if __name__ == '__main__':
	main()
