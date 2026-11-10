"""ADR 0238 approved measured-pair exception; no extrapolation or publication grant."""
import argparse
import copy
import json
from pathlib import Path
from child_result import atomic_json, digest
from fixed_route_exception import evaluate, load, PARTITIONS

POLICY = Path(__file__).with_name('fixed_route_0238.json')


def controls(records, binaries, policy):
	"""Every candidate case/partition must reject one additional instruction."""
	checked = 0
	for i, record in enumerate(records):
		if record['role'] != 'candidate':
			continue
		for part in PARTITIONS:
			mutant = copy.deepcopy(records)
			a = mutant[i]['accounting']
			name = next(iter(a[part]), 'unlisted-control-function')
			a[part][name] = a[part].get(name, 0) + 1
			a[part.replace('functions', 'Ir')] += 1
			a['total_Ir'] += 1
			try:
				evaluate(mutant, binaries, policy)
			except ValueError:
				checked += 1
			else:
				raise ValueError('one-instruction negative control escaped')
	bad = dict(binaries, candidate='0' * 64)
	try:
		evaluate(records, bad, policy)
	except ValueError:
		checked += 1
	else:
		raise ValueError('wrong-binary negative control escaped')
	return checked


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--capture', type=Path, required=True)
	p.add_argument('--pair', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args()
	result = {'status': 'FAIL'}
	try:
		policy = json.loads(POLICY.read_text())
		records, binaries = load(a.capture, a.pair)
		result = evaluate(records, binaries, policy)
		result['scope'] = '0238 approved binary pair and captured cases only; publication requires review'
		result['negative_controls'] = controls(records, binaries, policy)
		result.update(capture_sha256=digest(a.capture), pair_sha256=digest(a.pair), policy_sha256=digest(POLICY))
	except Exception as error:
		result = {'status': 'FAIL', 'error': repr(error)}
	atomic_json(a.out, result)
	print(result['status'], a.out)
	return result['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
