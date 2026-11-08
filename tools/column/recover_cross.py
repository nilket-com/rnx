"""Finish reviewed acceptance after a test-only cross-harness configuration fix."""
import accept
import gate_policy
import child_result
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import time
import traceback

from identity import verify_pair

ROOT = Path(__file__).resolve().parents[2]
PASSED_TAIL = ['check-demos-candle-Cargo.toml', 'check-demos-polars-Cargo.toml', 'check-servers-http-postgres-Cargo.toml']


def sha(path):
	return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_tail(prior):
	assert prior['status'] == 'FAIL'
	checks = prior['checks']
	assert [row['name'] for row in checks] == PASSED_TAIL + ['cross'], 'unexpected recovery point'
	assert all(row['exit'] == 0 for row in checks[:-1]) and checks[-1]['exit'] != 0
	assert all(row['exit'] == 0 for row in prior['reused_checks']), 'failed check cannot be reused'


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument('--prior', required=True, type=Path)
	parser.add_argument('--completed-suite', required=True, type=Path)
	parser.add_argument('--out', required=True, type=Path)
	parser.add_argument('--identity-pair', required=True, type=Path)
	parser.add_argument('--verified-prerequisites', type=Path, help='reuse the paired candidate prerequisite run only for a tooling-only amendment')
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	started = time.monotonic()
	result = {'status': 'FAIL', 'checks': [], 'reused_checks': []}
	env = dict(os.environ, CARGO_BUILD_JOBS='4', PYTHONDONTWRITEBYTECODE='1')
	try:
		prior = json.loads(args.prior.read_text())
		validate_tail(prior)
		candidate = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
		def exact():
			assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == candidate
			assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
		exact()
		changes = subprocess.check_output(['git', 'diff', '--name-only', prior['candidate'], candidate], cwd=ROOT, text=True).splitlines()
		assert all(p.startswith('tools/column/') or p in ('probes/0072/inventory/doc.sh', 'src/column.rs') for p in changes)
		original = subprocess.check_output(['git', 'show', prior['candidate'] + ':src/column.rs'], cwd=ROOT, text=True)
		assert original.count('#[cfg(test)]\nmod tests;') == 1
		assert (ROOT / 'src/column.rs').read_text() == original.replace('#[cfg(test)]\nmod tests;', '#[cfg(all(test, not(feature = "cross-verification")))]\nmod tests;'), 'unreviewed product change'
		origin_path = Path(prior['prior_receipt'])
		assert sha(origin_path) == prior['prior_sha256']
		origin = json.loads(origin_path.read_text())
		assert origin['candidate'] == prior['candidate']
		assert [row['name'] for row in prior['reused_checks']] == [row['name'] for row in origin['checks'][:-1]] + ['polars-suite'], 'missing prior acceptance stage'
		for row, old in zip(prior['reused_checks'], origin['checks'][:-1]):
			assert row['command'] == old['command'] and row['exit'] == old['exit'] == 0
		suite = json.loads(args.completed_suite.read_text())
		assert suite['candidate'] == prior['candidate'] and suite['prior_sha256'] == prior['prior_sha256']
		assert len(suite['checks']) == 1 and suite['checks'][0]['name'] == 'polars-suite' and suite['checks'][0]['exit'] == 0
		for row in prior['reused_checks']:
			base = args.completed_suite.parent if row['name'] == 'polars-suite' else origin_path.parent
			for suffix, digest in row['log_sha256'].items():
				assert sha(base / (row['name'] + '.' + suffix)) == digest
			if row['name'] == 'polars-suite':
				assert sha(args.completed_suite) == row['receipt_sha256']
				output = row['declared_output']
				assert sha(Path(output['archive'])) == output['actual_sha256']
			result['reused_checks'].append(row)
		for row in prior['checks'][:-1]:
			logs = {suffix: sha(args.prior.parent / (row['name'] + '.' + suffix)) for suffix in ('stdout', 'stderr')}
			result['reused_checks'].append(dict(row, log_sha256=logs))
		result.update(candidate=candidate, prior_candidate=prior['candidate'], prior_receipt_sha256=sha(args.prior), source_diff=changes)
		pair = json.loads(args.identity_pair.read_text())
		result['production_identity'] = verify_pair(pair, prior['candidate'])
		result['production_identity']['receipt_sha256'] = sha(args.identity_pair)
		product_candidate = result['production_identity']['candidate']
		later = subprocess.check_output(['git', 'diff', '--name-only', product_candidate, candidate], cwd=ROOT, text=True).splitlines()
		assert all(p.startswith('tools/column/') for p in later), 'product changed after identity pair'
		prerequisites = ('prerequisites-unit-and-production', ['python3', '-B', 'tools/column/verify.py', '--scope', 'all', '--cli', '--release', '--out', str(out / 'prerequisites')])
		if args.verified_prerequisites:
			checked = json.loads(args.verified_prerequisites.read_text())
			assert checked['status'] == 'PASS' and checked['commit'] == product_candidate
			names = {row['name'] for row in checked['checks']}
			assert {'fmt', 'adapter-fmt', 'host-clippy', 'adapter-clippy', 'host-tests', 'routing-inventory', 'adapter-unit-tests', 'adapter-tests', 'cli-build', 'cli-controls'}.issubset(names)
			assert all(row['exit'] == 0 for row in checked['checks'])
			logs = {row['name']: {suffix: sha(args.verified_prerequisites.parent / (row['name'] + '.' + suffix)) for suffix in ('stdout', 'stderr')} for row in checked['checks']}
			result['reused_prerequisites'] = {'receipt_sha256': sha(args.verified_prerequisites), 'commit': product_candidate, 'log_sha256': logs}
			prerequisites = ('final-tooling-controls', ['python3', '-B', '-m', 'unittest', 'discover', '-s', 'tools/column', '-p', 'test_*.py', '-v'])
		steps = [
			('inventory-preflight', ['python3', '-B', 'tools/column/inventories.py', '--preflight', '--out', str(out / 'inventory-preflight')]),
			prerequisites,
			('cross', dict(accept.commands(out))['cross']),
			('fresh-inventories', ['python3', '-B', 'tools/column/inventories.py', '--out', str(out / 'inventories')]),
		]
		for name, command in steps:
			exact()
			before = time.monotonic()
			with (out / (name + '.stdout')).open('x') as stdout, (out / (name + '.stderr')).open('x') as stderr:
				p = child_result.run(command, cwd=ROOT, env=env, stdout=stdout, stderr=stderr, timeout=21600)
			result['checks'].append({'name': name, 'command': command, 'exit': p.returncode, 'seconds': time.monotonic() - before, 'validation': p.validation})
			exact()
			assert p.returncode == 0, name
		selection = gate_policy.select(ROOT, out, ['cross'])
		refs = {r['name']: r['validation'] for r in result['reused_checks'] + result['checks'] if 'validation' in r}
		result['finalization'] = gate_policy.finalize(selection, {n: refs[n] for n, _ in selection['stages']}, ROOT)
		result['status'] = 'PASS'
	except BaseException as error:
		result['error'] = repr(error)
		(out / 'TRACEBACK.txt').write_text(traceback.format_exc())
	finally:
		result['seconds'] = time.monotonic() - started
		result['controller_sha256'] = sha(Path(__file__))
		child_result.publish(out / "RESULT.json", result)
	print(result['status'], str(out / 'RESULT.json'))
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
