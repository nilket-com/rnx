"""Score saved ADR0213 captures under reviewed exact preparation constants."""
import argparse
import hashlib
import json
from pathlib import Path
import sys

import ir_accounting
import preparation_rule as prep


def evaluate(preflight, large, adversarial, builds, raw_evaluator, policy):
	if (builds['baseline'], builds['candidate']) != (prep.BASE, prep.CANDIDATE):
		raise ValueError('unreviewed pair')
	paths = {(k, l) for k in prep.VECTORS for l in prep.LAYOUTS}
	def index(records, sizes):
		result = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
		expected = {(k, l, n, role) for k, l in paths for n in sizes for role in ('baseline', 'candidate')}
		if set(result) != expected or len(result) != len(records):
			raise ValueError('matrix coverage')
		for row in records:
			if len(row['costs']) != 2:
				raise ValueError('call coverage')
			for cost in row['costs']:
				a = cost['accounting']
				if cost['Ir'] != a['rnx_Ir'] + a['dynamic_Ir'] + a['other_Ir'] or cost['Ir'] != a['total_Ir']:
					raise ValueError('partition mismatch')
		return result
	ix = index(preflight['records'], [4096]) | index(large['records'], [10000000, 100000000])
	raw = raw_evaluator(large['records'], policy)
	failures = [f for f in raw['failures'] if not (f['kind'] != 'multiply3' and f['rule'] in ('strict nullable improvement', 'existing flat zero growth'))]
	proofs, bands = [], []
	for kind, layout in sorted(paths):
		samples = []
		for n in (4096, 10000000, 100000000):
			for call in (0, 1):
				s = dict(kind=kind, layout=layout, rows=n, call=call)
				for role in ('baseline', 'candidate'):
					a = ix[kind, layout, n, role]['costs'][call]['accounting']
					s[role] = dict(program_Ir=a['rnx_Ir'], functions=a['rnx_functions'])
				a = ix[kind, layout, n, 'baseline']['costs'][call]['accounting']
				b = ix[kind, layout, n, 'candidate']['costs'][call]['accounting']
				if b['other_Ir'] != a['other_Ir']:
					raise ValueError('unknown partition growth/change')
				samples.append(s)
		proofs.append(dict(kind=kind, layout=layout, proof=prep.check(samples, prep.BASE, prep.CANDIDATE)))
		for call in (0, 1):
			deltas = []
			for n in (10000000, 100000000):
				a = ix[kind, layout, n, 'baseline']['costs'][call]
				b = ix[kind, layout, n, 'candidate']['costs'][call]
				deltas.append(b['accounting']['dynamic_Ir'] - a['accounting']['dynamic_Ir'])
				if kind == 'multiply3':
					if b['Ir'] > a['Ir'] or (layout in ('alternating', 'deterministic-random') and b['Ir'] > 25 * n):
						failures.append(dict(rule='0213 multiply3 target', layout=layout, rows=n, call=call))
			if kind != 'multiply3':
				band = ir_accounting.system_band(*deltas)
				bands.append(dict(kind=kind, layout=layout, call=call, **band))
				if not band['passes']:
					failures.append(dict(rule='system library band', kind=kind, layout=layout, call=call))
	ai = {(r['rows'], r['layout'], r['role']): r for r in adversarial['records']}
	if len(ai) != 8 or set(ai) != {(n, l, role) for n in (4096, 65536) for l in ('benign', 'adversarial') for role in ('baseline', 'candidate')}:
		raise ValueError('adversarial coverage')
	for n in (4096, 65536):
		for call in (0, 1):
			if ai[n, 'adversarial', 'candidate']['costs'][call]['Ir'] > ai[n, 'adversarial', 'baseline']['costs'][call]['Ir']:
				failures.append(dict(rule='adversarial regression', rows=n, call=call))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, preparation_proofs=proofs, system_bands=bands, raw_verdict=raw)


def main():
	p = argparse.ArgumentParser()
	for name in ('preflight', 'large', 'adversarial', 'builds', 'tooling', 'out'):
		p.add_argument('--' + name, type=Path, required=True)
	a = p.parse_args()
	sys.path.insert(0, str(a.tooling / 'tools/xtask/column'))
	import cli_rules
	inputs = {k: getattr(a, k) for k in ('preflight', 'large', 'adversarial', 'builds')}
	d = {k: json.loads(v.read_text()) for k, v in inputs.items()}
	result = evaluate(**d, raw_evaluator=cli_rules.evaluate, policy=json.loads(Path(__file__).with_name('CK2.json').read_text()))
	result['input_sha256'] = {k: hashlib.sha256(v.read_bytes()).hexdigest() for k, v in inputs.items()}
	a.out.write_text(json.dumps(result, indent=2) + '\n')
	print(result['status'], a.out)
	return result['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
