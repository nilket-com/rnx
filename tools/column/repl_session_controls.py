"""ADR 0242: persistent-session controls and path-selected final gates."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import traceback

import child_result as child
import gate_policy

ROOT = Path(__file__).resolve().parents[2]


def commands(scope, out):
	cargo = ['cargo', '+1.98.1', 'test', '--locked']
	if scope == 'development':
		return [
			('session', cargo + ['--lib', 'session::', '--', '--test-threads=4']),
			('inspection', cargo + ['--lib', 'inspect::', '--', '--test-threads=4']),
			('domain', cargo + ['-p', 'rune', '--profile', 'engine-dev', '--lib', '--features', 'fmt,workspace,execution', 'experimental_core::domain::tests::record_fields', '--', '--test-threads=4']),
			('repl-cli', cargo + ['--test', 'repl_execution', '--', '--test-threads=4']),
			('inventory', cargo + ['--features', 'project-sources', '--lib', 'routing_inventory', '--', '--test-threads=4']),
		]
	if scope == 'miri':
		return [('ownership-miri', ['python3', '-B', 'tools/column/miri_exact.py', '--package', 'rune',
			'--inventory', 'tools/column/repl_session_miri.json', '--out', str(out/'miri')])]
	selection = gate_policy.select(ROOT, out, ['format', 'clippy', 'async-test-support', 'generated-projects'])
	child.atomic_json(out/'SELECTION.json', selection)
	return selection['stages']


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--scope', choices=('development', 'miri', 'final'), required=True)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args(); out = a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL', scope=a.scope, checks=[])
	try:
		source = child.source(ROOT); candidate = child.head(ROOT)
		r.update(source=source, candidate=candidate)
		env = dict(os.environ, CARGO_BUILD_JOBS='4', PYTHONDONTWRITEBYTECODE='1')
		for name, command in commands(a.scope, out):
			with (out/(name+'.log')).open('x') as stream:
				done = child.run(command, root=ROOT, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT,
					directory=out/(name+'-child'), timeout=28800)
			row = dict(name=name, status='PASS' if done.returncode == 0 else 'FAIL', validation=done.validation)
			child.atomic_json(out/(name+'-stage.json'), row); r['checks'].append(row)
			if done.returncode: raise RuntimeError(name+' failed; no later stage launched')
		if child.source(ROOT) != source or child.head(ROOT) != candidate: raise ValueError('source changed')
		if a.scope == 'final':
			selection = json.loads((out/'SELECTION.json').read_text())
			r['finalization'] = gate_policy.finalize(selection, {c['name']:c['validation'] for c in r['checks']}, ROOT)
		r['status'] = 'PASS'
	except Exception:
		r['error'] = traceback.format_exc()
	finally:
		child.publish(out/'RESULT.json', r)
		print(r['status'], out/'RESULT.json', flush=True)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
