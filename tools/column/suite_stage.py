"""Required package suites: explicit recipes, live nonempty discovery and receipts."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import tarfile
import traceback

import child_result as child

ROOT = Path(__file__).resolve().parents[2]


from package_gates import PACKAGE_CHECKS, FORMATS, LINTS


def recipes(stage, root=ROOT):
	cargo = ['cargo', '+1.98.1']
	if stage in FORMATS: return [list(FORMATS[stage])]
	if stage in LINTS: return [list(c) for c in LINTS[stage]]
	for name, commands in PACKAGE_CHECKS.values():
		if stage == name: return [list(c) for c in commands]
	if stage in {'rune-suite', 'rune-clippy'}:
		m = json.loads((root/'rune/tools/xtask/manifests/native.json').read_text())
		if stage == 'rune-clippy':
			return [cargo + ['clippy', '--locked', '-p', 'rune', '--profile', 'engine-dev', '--lib', '--tests', '--features', f, '--', '-D', 'warnings'] for f in m['clippy_features']]
		libraries = [cargo + ['test', '--locked', '-p', 'rune', '--profile', 'engine-dev', '--lib', '--features', row['features']] + ([row['selector']] if row.get('selector') else []) for row in m['libraries']]
		# Each API boundary has a compiling twin and a compile-fail control.
		docs = [cargo + ['test', '--locked', '-p', 'rune', '--profile', 'engine-dev', '--doc', '--features', 'fmt,workspace,execution', selector]
			for selector in ('host_function_owned_output', 'host_function_string_input', 'ExecutionRuntime', 'function::host::HostOutput', 'opaque_factory', 'opaque_method')]
		return libraries + docs
	if stage.startswith(('suite-adapter-', 'clippy-adapter-')):
		name = stage.split('-')[-1]
		if name not in {'polars', 'postgres', 'candle'}: raise ValueError('unknown adapter')
		manifest = 'adapters/' + name + '/Cargo.toml'
	elif stage in {'project-suite', 'project-clippy'}:
		manifest = 'tools/project/Cargo.toml'
	else: raise ValueError('unknown suite stage')
	verb = 'clippy' if 'clippy' in stage else 'test'
	return [cargo + [verb, '--locked', '--manifest-path', manifest, '--features', 'test-support', '--lib', '--tests']]


def discovered(text):
	names = re.findall(r'^(.+): test$', text, re.M)
	if not names: raise ValueError('empty test discovery')
	return names


def executed(text):
	counts = re.findall(r'test result: ok\. (\d+) passed;', text)
	if not counts or sum(map(int, counts)) == 0: raise ValueError('empty executed suite')
	return sum(map(int, counts))


def invoke(command, cwd, log):
	environment = None
	if 'rune/tools/xtask/column/probe/Cargo.toml' in command and 'fmt' not in command:
		from fixed_route_gate import valgrind_root
		include = valgrind_root()/'include'
		if not (include/'valgrind/callgrind.h').is_file():
			raise ValueError('column probe requires Valgrind include/valgrind/callgrind.h')
		environment = dict(os.environ, VALGRIND_INCLUDE=str(include))
	with log.open('x') as stream:
		subprocess.run(command, cwd=cwd, env=environment, stdout=stream, stderr=subprocess.STDOUT, check=True, timeout=10800)
	return log.read_text()


def run(stage, out, root=ROOT):
	rows = []
	commands = recipes(stage, root)
	if not commands: raise ValueError('empty recipe')
	for i, command in enumerate(commands):
		row = dict(command=command)
		if 'test' in command:
			row['discovered'] = discovered(invoke(command + ['--', '--list'], root, out/f'{i}-discovery.log'))
			row['passed'] = executed(invoke(command + ['--', '--test-threads=4'], root, out/f'{i}-test.log'))
			if '--doc' in command and (len(row['discovered']) != {'host_function_owned_output': 4, 'host_function_string_input': 8, 'ExecutionRuntime': 2, 'function::host::HostOutput': 4, 'opaque_factory': 2, 'opaque_method': 5}[command[-1]] or row['passed'] != len(row['discovered'])):
				raise ValueError('API boundary requires both compiling and compile-fail twins')
		elif stage in {name for name, _ in PACKAGE_CHECKS.values()}:
			invoke(command, root, out/f'{i}-package.log')
		elif stage in FORMATS:
			invoke(command, root, out/f'{i}-format.log')
		elif stage == 'rune-clippy':
			invoke(command, root, out/f'{i}-clippy.log')
		else:
			# Apply the existing diagnostic identity/multiplicity rule to this
			# manifest and the record baseline, with no new warning allowance.
			import gate_policy
			from clippy import diagnostics
			baseline = gate_policy.record(root)['baseline']
			from final_evidence import assigned_root
			bulk = assigned_root('RNX_FINAL_BASELINE_ROOT',out)
			base = bulk/f'baseline-{i}'; base.mkdir()
			archive = bulk/f'baseline-{i}.tar'
			with archive.open('wb') as f: subprocess.run(['git', 'archive', baseline], cwd=root, stdout=f, check=True)
			with tarfile.open(archive) as t: t.extractall(base, filter='data')
			sets = []
			for role, directory in [('baseline', base), ('candidate', root)]:
				log = out/f'{i}-{role}-clippy.log'
				# Match the established adapter Clippy recipe: keep the existing
				# preview erasing_op diagnostic in the comparison, not as a deny
				# that aborts the baseline before its diagnostics are collected.
				invoke(clippy_command(command, stage), directory, log)
				sets.append(diagnostics(log, directory))
			if sets[1] - sets[0]: raise ValueError('new Clippy diagnostics: ' + repr(sets[1] - sets[0]))
			row['baseline'] = baseline
		rows.append(row)
	return rows


def clippy_command(command, stage):
	from clippy import POLARS_LINT_ARGS
	return command + ['--message-format=json'] + (
		list(POLARS_LINT_ARGS) if stage == 'clippy-adapter-polars' else [])


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--stage', required=True); p.add_argument('--out', type=Path, required=True)
	a = p.parse_args(); out = a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL', stage=a.stage)
	try:
		r['checks'] = run(a.stage, out)
		if a.stage == 'rune-suite':
			from native_contracts import run as contracts
			r['contracts'] = contracts(ROOT, out)
			if r['contracts']['status'] != 'PASS':
				raise ValueError(r['contracts']['error'])
		r['status'] = 'PASS'
	except Exception: r['error'] = traceback.format_exc()
	finally:
		if a.stage == 'suite-adapter-polars':
			try:
				from stage_outputs import finish_stage
				r['declared_output'] = finish_stage(ROOT, 'polars-suite', out)
			except Exception:
				r.update(status='FAIL', output_error=traceback.format_exc())
		r['artifacts'] = {str(p): child.digest(p) for p in out.glob('*.log')}
		if r.get('declared_output'):
			p = r['declared_output']['archive']; r['artifacts'][p] = child.digest(p)
		child.publish(out/'RESULT.json', r)
		print(r['status'], out/'RESULT.json', flush=True)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
