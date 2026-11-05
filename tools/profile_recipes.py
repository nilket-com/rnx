#!/usr/bin/env python3
"""Compare effective Cargo unit recipes before accepting workspace/profile changes."""
import argparse
import json
import os
from pathlib import Path
import subprocess


def normalized(graph, packages):
	rows = []
	for unit in graph['units']:
		package = packages[unit['pkg_id']]
		profile = dict(unit['profile'])
		profile.pop('name', None)
		rows.append({'package': [package['name'], package['version']],
			'target': [unit['target']['name'], unit['target']['kind']],
			'mode': unit['mode'], 'platform': unit.get('platform'),
			'features': unit['features'], 'profile': profile})
	if not rows:
		raise ValueError('empty compilation graph')
	return sorted(rows, key=lambda row: json.dumps(row, sort_keys=True))


def controls():
	unit = {'pkg_id': 'old', 'target': {'name': 'rune', 'kind': ['lib']},
		'mode': 'build', 'features': ['std'], 'profile': {'name': 'dev', 'opt_level': '2'}}
	packages = {'old': {'name': 'rune', 'version': '0.15.0'}, 'new': {'name': 'rune', 'version': '0.15.0'}}
	before = normalized({'units': [unit]}, packages)
	unit['pkg_id'] = 'new'
	unit['profile']['name'] = 'engine-dev'
	assert normalized({'units': [unit]}, packages) == before
	unit['profile']['opt_level'] = '0'
	assert normalized({'units': [unit]}, packages) != before
	try:
		normalized({'units': []}, packages)
	except ValueError:
		pass
	else:
		raise AssertionError('empty graph accepted')


def main():
	controls()
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--baseline', type=Path, required=True)
	p.add_argument('--candidate', type=Path, required=True)
	p.add_argument('--engine-baseline', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args()
	a.out.mkdir(parents=True, exist_ok=False)
	result = {'status': 'FAIL', 'scope': 'effective unit identities/features/profile fields; no binary equivalence claim', 'cases': []}
	env = dict(os.environ, RUSTC_BOOTSTRAP='1')
	def capture(label, root, args):
		command = ['cargo', '+1.98.1', *args]
		with (a.out / (label + '.stdout')).open('x') as out, (a.out / (label + '.stderr')).open('x') as err:
			subprocess.run(command, cwd=root, env=env, stdout=out, stderr=err, check=True)
		return json.loads((a.out / (label + '.stdout')).read_text())
	try:
		for family, manifest, package in [('Cargo', 'Cargo.toml', 'rnx'), ('adapters-candle-Cargo', 'adapters/candle/Cargo.toml', 'rnx-candle'), ('adapters-polars-Cargo', 'adapters/polars/Cargo.toml', 'rnx-polars'), ('adapters-postgres-Cargo', 'adapters/postgres/Cargo.toml', 'rnx-postgres'), ('demos-candle-Cargo', 'demos/candle/Cargo.toml', 'rnx-candle-demo'), ('demos-polars-Cargo', 'demos/polars/Cargo.toml', 'rnx-polars-demo'), ('servers-http-postgres-Cargo', 'servers/http-postgres/Cargo.toml', 'rnx-http-postgres'), ('tools-project-Cargo', 'tools/project/Cargo.toml', 'rnx-project'), ('engine', 'Cargo.toml', 'rune')]:
			base = a.engine_baseline if family == 'engine' else a.baseline
			maps = {}
			for role, root in [('baseline', base), ('candidate', a.candidate)]:
				metadata = capture(f'{family}-{role}-metadata', root, ['metadata', '--offline', '--locked', '--format-version=1', '--manifest-path', manifest])
				maps[role] = {entry['id']: entry for entry in metadata['packages']}
			for profile in (['engine-dev', 'engine-release', 'engine-bench'] if family == 'engine' else ['dev', 'test', 'release']):
				graphs = []
				for role, root in [('baseline', base), ('candidate', a.candidate)]:
					args = ['-Z', 'unstable-options', 'test' if profile == 'test' else 'build', '--unit-graph', '--offline', '--locked', '--manifest-path', manifest, '-p', package, '--profile', profile]
					if family == 'engine':
						args += ['--lib']
					elif manifest == 'Cargo.toml':
						args += ['--features', 'project-sources']
					graph = capture(f'{family}-{profile}-{role}', root, args)
					graphs.append(normalized(graph, maps[role]))
				row = {'family': family, 'profile': profile, 'baseline_units': len(graphs[0]), 'candidate_units': len(graphs[1]), 'status': 'PASS' if graphs[0] == graphs[1] else 'FAIL'}
				result['cases'].append(row)
				if row['status'] != 'PASS':
					(a.out / (family + '-' + profile + '-difference.json')).write_text(json.dumps(graphs, indent=2) + '\n')
		if all(row['status'] == 'PASS' for row in result['cases']):
			result['status'] = 'PASS'
	except Exception as error:
		result['error'] = repr(error)
	finally:
		(a.out / 'RESULT.json').write_text(json.dumps(result, indent=2) + '\n')
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
