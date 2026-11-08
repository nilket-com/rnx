"""Exact first/warm portable host vectors; capture one named map boundary."""
import argparse
from concurrent.futures import ProcessPoolExecutor
import json
import os
from pathlib import Path
import platform
import subprocess
import sys
import traceback
from child_result import atomic_json, digest
from ir_pool import capture_environment
from ir_accounting import account
from boolean_vectors import vector, compare


def job(spec):
	out = Path(spec['directory']); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL', **spec)
	try:
		binary = Path(spec['build']['binary'])
		assert digest(binary) == spec['build']['sha256']
		profile = out / 'callgrind.out'
		command = ['/usr/bin/setarch', platform.machine(), '-R', spec['valgrind'],
			'--tool=callgrind', '--collect-atstart=no', '--toggle-collect=measured_portable_map',
			'--dump-instr=yes', '--collect-jumps=yes', '--callgrind-out-file=' + str(profile),
			str(binary), str(spec['rows']), spec['temperature'], spec['operator']]
		with (out / 'stdout').open('x') as stdout, (out / 'stderr').open('x') as stderr:
			p = subprocess.run(command, env=capture_environment(), stdout=stdout, stderr=stderr, timeout=1200)
		assert p.returncode == 0, 'probe failed'
		assert (out / 'stdout').read_text().strip() == f"PASS {spec['rows']} {spec['operator']} portable"
		cost = dict(profile=str(profile), sha256=digest(profile), accounting=account(profile, binary))
		v = vector(cost)
		assert v.get('measured_portable_map', (0, 0))[0] > 0, 'empty measured boundary'
		assert any('::column::run::predicate::run::<' in n and x[0] > 0 for n, x in v.items()), 'missing driver'
		assert not any('simd::compare_' in n and x[0] > 0 for n, x in v.items()), 'AVX2 ran in portable mode'
		r.update(status='PASS', cost=cost, command=command)
	except Exception:
		r['error'] = traceback.format_exc()
	finally:
		atomic_json(out / 'RESULT.json', r)
	if r['status'] != 'PASS':
		raise RuntimeError(str(out / 'RESULT.json'))
	return r


def run(args):
	args.out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL')
	try:
		build = json.loads(args.builds.read_text()); assert build['status'] == 'PASS'
		resources = json.loads(args.resources.read_text())
		jobs = [dict(build=b, rows=n, temperature=t, operator=op, valgrind=str(args.valgrind.resolve()),
			directory=str(args.out / f'{b["role"]}-{name}-{n}-{t}'))
			for b in build['builds'] for name, op in (('eq', '=='), ('ne', '!='), ('lt', '<'), ('le', '<='), ('gt', '>'), ('ge', '>='))
			for n in (4096, 65536) for t in ('first', 'warm')]
		atomic_json(args.out / 'MANIFEST.json', dict(builds=build, jobs=jobs,
			workers=min(4, resources['workers']), resources=resources, controller_sha256=digest(Path(__file__))))
		with ProcessPoolExecutor(max_workers=min(4, resources['workers'])) as pool:
			records = list(pool.map(job, jobs))
		index = {(x['operator'], x['rows'], x['temperature'], x['build']['role']): x for x in records}
		assert len(index) == 48
		comparisons = []
		for op in ('==', '!=', '<', '<=', '>', '>='):
			for t in ('first', 'warm'):
				for n in (4096, 65536):
					a, b = (index[op, n, t, role]['cost'] for role in ('baseline', 'candidate'))
					comparisons.append(dict(operator=op, rows=n, temperature=t,
						differences=compare(vector(a), vector(b)),
						program_delta=b['accounting']['rnx_Ir']-a['accounting']['rnx_Ir'],
						baseline_program_Ir=a['accounting']['rnx_Ir'], candidate_program_Ir=b['accounting']['rnx_Ir']))
		# Raw exact verdict: fixed deltas require review; no automatic new allowance.
		r.update(status='FAIL' if any(x['differences'] for x in comparisons) else 'PASS',
			capture_status='PASS', records=records, comparisons=comparisons)
	except Exception:
		r['error'] = traceback.format_exc()
	finally:
		atomic_json(args.out / 'RESULT.json', r)
		print(r['status'], args.out / 'RESULT.json', flush=True)
	return r['status'] != 'PASS'


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--builds', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	p.add_argument('--resources', type=Path, required=True)
	p.add_argument('--valgrind', type=Path, required=True)
	a = p.parse_args(); a.out = a.out.resolve()
	return run(a)


if __name__ == '__main__':
	raise SystemExit(main())
