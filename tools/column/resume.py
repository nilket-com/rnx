"""Resume an unchanged fresh source after its failed full-Polars inventory prerequisite."""
import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import traceback

from stage_outputs import finish_stage


def sha(path):
	return hashlib.sha256(path.read_bytes()).hexdigest()


def reusable_prefix(prior, steps):
	assert prior['status'] == 'FAIL' and prior['checks'], 'expected a failed acceptance'
	checks = prior['checks']
	assert checks[-1]['name'] == 'polars-suite' and checks[-1]['exit'] != 0
	assert len(checks) <= len(steps)
	for check, (name, command) in zip(checks, steps):
		assert check['name'] == name and check['command'] == command, 'acceptance plan changed'
	assert all(check['exit'] == 0 for check in checks[:-1]), 'cannot reuse a failed stage'
	return len(checks) - 1


def validate_completed_suite(completed, prior, prior_hash, command):
	assert completed['status'] == 'FAIL' and completed['error'] == "AssertionError('source changed')"
	assert completed['candidate'] == prior['candidate'] and completed['prior_sha256'] == prior_hash
	assert len(completed['checks']) == 1
	check = completed['checks'][0]
	assert check['name'] == 'polars-suite' and check['exit'] == 0 and check['command'] == command


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument('--root', type=Path, required=True)
	parser.add_argument('--prior', type=Path, required=True)
	parser.add_argument('--inventories', type=Path, required=True)
	parser.add_argument('--out', type=Path, required=True)
	parser.add_argument('--completed-suite', type=Path, help='reviewed prior resume with Polars EXIT0 followed only by output-drift failure')
	parser.add_argument('--reviewed-oracle-sha256', help='required exact reviewed output hash with --completed-suite')
	args = parser.parse_args()
	root, prior_path, out = args.root.resolve(strict=True), args.prior.resolve(strict=True), args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	started = time.monotonic()
	result = {'status': 'FAIL', 'checks': [], 'reused_checks': []}
	try:
		prior = json.loads(prior_path.read_text())
		result.update(candidate=prior['candidate'], prior_receipt=str(prior_path), prior_sha256=sha(prior_path))
		def exact():
			assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == prior['candidate']
			assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=root, text=True).strip(), 'source changed'
		if not args.completed_suite:
			exact()
		else:
			assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip() == prior['candidate']
		controller = root / 'tools/column/accept.py'
		assert sha(controller) == prior['controller_sha256'], 'original controller changed'
		spec = importlib.util.spec_from_file_location('original_accept', controller)
		accept = importlib.util.module_from_spec(spec)
		spec.loader.exec_module(accept)
		index = reusable_prefix(prior, accept.commands(prior_path.parent))
		for check in prior['checks'][:index]:
			logs = {suffix: sha(prior_path.parent / (check['name'] + '.' + suffix)) for suffix in ('stdout', 'stderr')}
			result['reused_checks'].append(dict(check, log_sha256=logs))
		inventory = json.loads(args.inventories.read_text())
		assert inventory['status'] == 'PASS' and inventory['source_commit'] == prior['candidate']
		assert {row['cfg'] for row in inventory['inventories']} == {'adapter-json', 'adapter', 'adapter-narrow'}
		for row in inventory['inventories']:
			path = root / ('probes/0072/out/0.55.2-' + row['cfg'] + '/result/inventory.json')
			assert path.resolve() == Path(row['inventory']).resolve() and sha(path) == row['sha256']
		result['inventory_receipt_sha256'] = sha(args.inventories)
		if args.completed_suite:
			completed = json.loads(args.completed_suite.read_text())
			validate_completed_suite(completed, prior, sha(prior_path), accept.commands(prior_path.parent)[index][1])
			assert args.reviewed_oracle_sha256 and sha(root / 'adapters/polars/oracle-results.json') == args.reviewed_oracle_sha256, 'unreviewed oracle output'
			output = finish_stage(root, 'polars-suite', out)
			exact()
			check = dict(completed['checks'][0])
			check['declared_output'] = output
			check['log_sha256'] = {suffix: sha(args.completed_suite.parent / ('polars-suite.' + suffix)) for suffix in ('stdout', 'stderr')}
			check['receipt_sha256'] = sha(args.completed_suite)
			result['reused_checks'].append(check)
			index += 1
		else:
			assert not args.reviewed_oracle_sha256, 'output review needs a completed-suite receipt'
		env = dict(os.environ, CARGO_BUILD_JOBS='4', PYTHONDONTWRITEBYTECODE='1')
		assert not env.get('CARGO_TARGET_DIR')
		for name, command in accept.commands(out)[index:]:
			exact()
			before = time.monotonic()
			with (out / (name + '.stdout')).open('x') as stdout, (out / (name + '.stderr')).open('x') as stderr:
				p = subprocess.run(command, cwd=root, env=env, stdout=stdout, stderr=stderr, timeout=10800)
			result['checks'].append({'name': name, 'command': command, 'exit': p.returncode, 'seconds': time.monotonic() - before})
			output = finish_stage(root, name, out)
			if output is not None:
				result['checks'][-1]['declared_output'] = output
			exact()
			assert p.returncode == 0, (name, p.returncode)
		result['status'] = 'PASS'
	except BaseException as error:
		result['error'] = repr(error)
		(out / 'TRACEBACK.txt').write_text(traceback.format_exc())
	finally:
		result['seconds'] = time.monotonic() - started
		result['resume_controller_sha256'] = sha(Path(__file__))
		(out / 'RESULT.json').write_text(json.dumps(result, indent=2) + '\n')
	print(result['status'], str(out / 'RESULT.json'))
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
