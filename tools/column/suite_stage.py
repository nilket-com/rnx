"""The rune-suite stage: its native-manifest test recipes, live nonempty
discovery, the API-boundary twins, its native contract cases, and receipts.

Every other package, format, lint and Clippy stage runs as the Rust
controller's stage mode (`rnx-final stage suite`), from its catalogue."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import traceback

import child_result as child

ROOT = Path(__file__).resolve().parents[2]
# Each API boundary has a compiling twin and a compile-fail control.
TWINS = {'host_function_owned_output': 4, 'host_function_string_input': 8, 'ExecutionRuntime': 2,
	'function::host::HostOutput': 4, 'opaque_factory': 2, 'opaque_method': 5}


def recipes(stage, root=ROOT):
	if stage != 'rune-suite': raise ValueError('only rune-suite runs here; other suite stages are the Rust stage mode')
	cargo = ['cargo', '+1.98.1']
	m = json.loads((root/'rune/tools/xtask/manifests/native.json').read_text())
	libraries = [cargo + ['test', '--locked', '-p', 'rune', '--profile', 'engine-dev', '--lib', '--features', row['features']] + ([row['selector']] if row.get('selector') else []) for row in m['libraries']]
	docs = [cargo + ['test', '--locked', '-p', 'rune', '--profile', 'engine-dev', '--doc', '--features', 'fmt,workspace,execution', selector]
		for selector in TWINS]
	return libraries + docs


def discovered(text):
	names = re.findall(r'^(.+): test$', text, re.M)
	if not names: raise ValueError('empty test discovery')
	return names


def executed(text):
	counts = re.findall(r'test result: ok\. (\d+) passed;', text)
	if not counts or sum(map(int, counts)) == 0: raise ValueError('empty executed suite')
	return sum(map(int, counts))


def invoke(command, cwd, log):
	with log.open('x') as stream:
		subprocess.run(command, cwd=cwd, stdout=stream, stderr=subprocess.STDOUT, check=True, timeout=10800)
	return log.read_text()


def run(stage, out, root=ROOT):
	rows = []
	commands = recipes(stage, root)
	if not commands: raise ValueError('empty recipe')
	for i, command in enumerate(commands):
		row = dict(command=command)
		row['discovered'] = discovered(invoke(command + ['--', '--list'], root, out/f'{i}-discovery.log'))
		row['passed'] = executed(invoke(command + ['--', '--test-threads=4'], root, out/f'{i}-test.log'))
		if '--doc' in command and (len(row['discovered']) != TWINS[command[-1]] or row['passed'] != len(row['discovered'])):
			raise ValueError('API boundary requires both compiling and compile-fail twins')
		rows.append(row)
	return rows


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--stage', required=True); p.add_argument('--out', type=Path, required=True)
	a = p.parse_args(); out = a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL', stage=a.stage)
	try:
		r['checks'] = run(a.stage, out)
		from native_contracts import run as contracts
		r['contracts'] = contracts(ROOT, out)
		if r['contracts']['status'] != 'PASS':
			raise ValueError(r['contracts']['error'])
		r['status'] = 'PASS'
	except Exception: r['error'] = traceback.format_exc()
	finally:
		r['artifacts'] = {str(p): child.digest(p) for p in out.glob('*.log')}
		child.publish(out/'RESULT.json', r)
		print(r['status'], out/'RESULT.json', flush=True)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
