#!/usr/bin/env python3
"""Durable supplemental Miri jobs; launch once, explicitly resume interruption."""
import argparse
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import traceback

from admission import admit
from partitions import validate
from receipts import Run, atomic_json, digest
from runtime import check_tests, execute, overlap, resources, traces

HERE = Path(__file__).resolve().parent


def output(command, cwd=None):
	return subprocess.check_output(command, cwd=cwd, text=True).strip()


def source_identity(root, manifest, env, scope="full", require_overlap=False):
	if output(['git', 'status', '--porcelain', '--untracked-files=normal'], root):
		raise ValueError('source checkout must be clean')
	paths = subprocess.check_output(['git', 'ls-files', '-z'], cwd=root).decode().split('\0')
	tools = {}
	for version, names in [(manifest['stable'], ['cargo', 'rustc']), (manifest['nightly'], ['cargo', 'rustc', 'miri', 'cargo-miri'])]:
		for name in names:
			path = output(['rustup', 'which', '--toolchain', version, name])
			tools[version + ':' + name] = {'path': path, 'sha256': digest(path)}
	for name in ['strace', 'time', 'cp']:
		path = shutil.which(name)
		if path is None:
			raise ValueError('missing tool: ' + name)
		tools[name] = {'path': path, 'sha256': digest(path)}
	return {'revision': output(['git', 'rev-parse', 'HEAD'], root), 'root': str(root),
		'files': {p: digest(root / p) for p in paths if p}, 'tools': tools,
		'runner': {p.name: digest(p) for p in HERE.glob('*.py')},
		'manifest': manifest, 'environment': env, 'scope': scope, 'require_overlap': require_overlap}


def environment():
	# Fixed allowlist: private credentials and unrelated shell settings do not
	# enter children or receipt identity. Toolchain selection is explicit.
	env = {key: os.environ[key] for key in ['HOME', 'PATH', 'RUSTUP_HOME', 'CARGO_HOME'] if key in os.environ}
	env.update(LC_ALL='C', TZ='UTC', CARGO_BUILD_JOBS='4', CARGO_TERM_COLOR='never', CARGO_INCREMENTAL='0')
	return env


def cargo(manifest, feature):
	return ['cargo', '+' + manifest['nightly'], 'miri', 'test', '--profile', 'engine-dev', '--offline', '--frozen',
		'-p', 'rune', '--lib', '--features', manifest['features'][feature]]


def stage(run, name, action):
	attempt, cached = run.begin(name)
	if cached is not None:
		return cached
	try:
		details, artifacts = action(attempt)
		return run.finish(attempt, 'PASS', artifacts, **details)
	except Exception as error:
		(attempt / 'ERROR.txt').write_text(traceback.format_exc())
		run.finish(attempt, 'FAIL', [p for p in attempt.iterdir() if p.is_file()], error=repr(error))
		raise


def prerequisite(attempt, root, manifest, env):
	controls = execute([sys.executable, '-m', 'unittest', 'discover', '-s', str(HERE), '-p', 'test_*.py'], attempt, env, root, 120, label='controls')
	if controls['exit']:
		raise ValueError('supplemental runner controls failed')
	row = execute(['cargo', '+' + manifest['stable'], 'run', '--package', 'xtask', '--profile', 'engine-dev', '--', 'verify', '--suite', 'clippy',
		'--cold', '--out', str(attempt / 'canonical')], attempt, env, root, 7200)
	if row['exit']:
		raise ValueError('format/Clippy prerequisite failed')
	return row, [attempt / 'controls.stdout', attempt / 'controls.stderr', attempt / 'process.stdout', attempt / 'process.stderr', attempt / 'canonical' / 'RESULT.json']


def verify_harness_isolation(target):
	import re
	harnesses = []
	for path in target.glob('miri/*/*/build/rune/*/out/rune-*'):
		if not re.fullmatch(r'rune-[0-9a-f]+', path.name):
			continue
		record = json.loads(path.read_text())
		if '--test' not in record.get('args', []):
			continue
		if any('incremental=' in arg for arg in record['args']):
			raise ValueError('prepared harness retains mutable incremental-cache path')
		harnesses.append(str(path))
	if not harnesses:
		raise ValueError('missing prepared test harness')
	return harnesses


def preparation(attempt, root, manifest, env, feature):
	target = attempt / 'target'
	row = execute(cargo(manifest, feature) + ['--', '--list'], attempt,
		dict(env, CARGO_TARGET_DIR=str(target)), root, 7200)
	if row['exit']:
		raise ValueError('Miri preparation failed')
	names = [line[:-6] for line in (attempt / 'process.stdout').read_text().splitlines() if line.endswith(': test')]
	if not names or len(names) != len(set(names)):
		raise ValueError('empty or duplicate harness inventory')
	row.update(target=str(target), inventory=names, isolated_harnesses=verify_harness_isolation(target))
	# Bind reuse to every prepared file, not merely a successful build log.
	return row, [attempt / 'process.stdout', attempt / 'process.stderr'] + sorted(p for p in target.rglob('*') if p.is_file())


def selection(manifest, prepared):
	jobs = []
	seen = set()
	for subset in manifest['subsets']:
		all_names = prepared[subset['feature']]['inventory']
		selected = [n for n in all_names if (n == subset['selector'] if subset['exact'] else subset['selector'] in n)]
		if sorted(selected) != sorted(subset['inventory']):
			raise ValueError('manifest differs from live subset inventory: ' + subset['name'])
		for index, names in enumerate(validate(selected, subset['shards'])):
			if any(n in seen for n in names):
				raise ValueError('overlapping supplemental subsets')
			seen.update(names)
			jobs.append(dict(name=f"{subset['name']}-{index:03d}", subset=subset['name'],
				feature=subset['feature'], names=names, ignored=[n for n in names if n in subset['ignored']], timeout=subset['timeout_seconds']))
	return jobs


def semantic(attempt, root, manifest, env, spec, prepared):
	target = attempt / 'target'
	copy = execute(['cp', '-a', '--reflink=auto', prepared['target'], str(target)], attempt, env, root, 600, label='copy')
	if copy['exit']:
		raise ValueError('prepared target copy failed; see copy.stderr')
	cmd = ['/usr/bin/time', '-q', '-f', '%M', '-o', str(attempt / 'peak-kib'),
		'strace', '-ff', '-ttt', '-T', '-yy', '-s', '65535', '-e', 'trace=execve,exit_group,flock',
		'-o', str(attempt / 'trace')] + cargo(manifest, spec['feature']) + ['--', '--test-threads=1', '--exact'] + spec['names']
	row = execute(cmd, attempt, dict(env, CARGO_TARGET_DIR=str(target)), root, spec['timeout'])
	if row['exit']:
		raise ValueError('Miri shard failed')
	stderr = (attempt / 'process.stderr').read_text()
	if 'Compiling ' in stderr or 'file lock on build directory' in stderr:
		raise ValueError('unexpected rebuild or build-directory lock')
	ignored = check_tests((attempt / 'process.stdout').read_text(), spec['names'])
	if sorted(ignored) != sorted(spec['ignored']):
		raise ValueError('ignored-test accounting changed')
	intervals, locks = traces(attempt)
	if not intervals:
		raise ValueError('missing actual interpreter lifetime')
	row.update(names=spec['names'], subset=spec['subset'], ignored=ignored, intervals=intervals,
		peak_rss_bytes=int((attempt / 'peak-kib').read_text().strip()) * 1024,
		package_cache_flock_seconds=sum(event['seconds'] for event in locks), lock_events=locks)
	return row, [p for p in attempt.iterdir() if p.is_file()]


def scoped_jobs(jobs, scope):
	if scope == 'full':
		return jobs
	if scope != 'tooling-short':
		raise ValueError('unknown acceptance scope')
	selected = [job for job in jobs if job['subset'] in ('payload_bit_scan', 'budget')]
	if len(selected) != 2:
		raise ValueError('short acceptance requires both declared real subsets')
	return selected


def run_jobs(run, root, manifest, env, workers, scope='full'):
	stage(run, 'prerequisites', lambda path: prerequisite(path, root, manifest, env))
	prepared = {feature: stage(run, 'prepare-' + feature,
		lambda path, f=feature: preparation(path, root, manifest, env, f)) for feature in manifest['features']}
	# All exact unions are validated before even one interpreter is released.
	jobs = scoped_jobs(selection(manifest, prepared), scope)
	rows = []
	with concurrent.futures.ThreadPoolExecutor(max_workers=workers) as pool:
		futures = [pool.submit(stage, run, spec['name'],
			lambda path, s=spec: semantic(path, root, manifest, env, s, prepared[s['feature']])) for spec in jobs]
		for future in futures:
			rows.append(future.result())
	# A copied harness must not mutate the shared preparation through captured paths.
	for feature in prepared:
		run.begin('prepare-' + feature)
	return {'jobs': rows, 'peak_interpreter_overlap': overlap(rows),
		'covered_names': sorted(name for row in rows for name in row['names'])}


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument('--root', type=Path, required=True)
	parser.add_argument('--out', type=Path, required=True)
	parser.add_argument('--resume', action='store_true')
	parser.add_argument('--scope', choices=['full', 'tooling-short'], default='full')
	parser.add_argument('--workers', type=int)
	parser.add_argument('--launch', action='store_true')
	parser.add_argument('--unit', default='rune-supplemental-miri')
	parser.add_argument('--notify-chatd', action='store_true')
	parser.add_argument('--require-overlap', action='store_true')
	args = parser.parse_args()
	args.require_overlap = args.require_overlap or args.scope == 'tooling-short'
	root = args.root.resolve()
	directory = args.out.resolve()
	manifest = json.loads((HERE / 'manifest.json').read_text())
	if directory.is_relative_to(root) and not directory.is_relative_to(root / 'target/xtask-verify'):
		parser.error('in-source output must be under target/xtask-verify')
	if args.launch:
		limits = admit(*resources(), manifest['measured_peak_rss_bytes'], args.workers)
		command = ['systemd-run', '--user', '--unit=' + args.unit,
			'--property=MemoryMax=' + str(limits['memory_max_bytes']), '--property=MemorySwapMax=0',
			'--property=OOMPolicy=kill', '--property=KillMode=control-group', '--working-directory=' + str(root)]
		for key, value in environment().items():
			command.append('--setenv=' + key + '=' + value)
		command += [sys.executable, str(Path(__file__).resolve())] + [arg for arg in sys.argv[1:] if arg != '--launch']
		subprocess.run(command, check=True)
		return 0
	result = {'status': 'FAIL', 'scope': args.scope, 'full_coverage': args.scope == 'full'}
	try:
		env = environment()
		identity = source_identity(root, manifest, env, args.scope, args.require_overlap)
		limits = admit(*resources(), manifest['measured_peak_rss_bytes'], args.workers)
		# Direct invocation must already be inside a bounded service.
		group = next(s.split(':', 2)[2] for s in Path('/proc/self/cgroup').read_text().splitlines() if s.startswith('0::'))
		if (Path('/sys/fs/cgroup') / group.lstrip('/') / 'memory.max').read_text().strip() == 'max':
			raise ValueError('use --launch or a service with MemoryMax')
		if args.require_overlap and limits['workers'] < 2:
			raise ValueError('overlap acceptance requires admission for two workers')
		with Run(directory, identity, args.resume) as run:
			result['resources'] = limits
			result.update(run_jobs(run, root, manifest, env, limits['workers'], args.scope))
			if args.require_overlap and result['peak_interpreter_overlap'] < 2:
				raise ValueError('acceptance requires actual interpreter overlap')
			if source_identity(root, manifest, env, args.scope, args.require_overlap) != identity:
				raise ValueError('source or toolchain changed during run')
			result['status'] = 'PASS'
	except Exception as error:
		result.update(error=repr(error), traceback=traceback.format_exc())
	finally:
		directory.mkdir(parents=True, exist_ok=True)
		# Invocation receipts never overwrite earlier failures or interrupted work.
		import uuid
		path = directory / ('INVOCATION-' + uuid.uuid4().hex + '.json')
		atomic_json(path, result)
		print(result['status'] + ': ' + str(path), flush=True)
		if args.notify_chatd:
			notice = path.with_suffix('.txt')
			notice.write_text('ADR0210 supplemental Miri ' + result['status'] + '. Receipt: ' + str(path) + '\n')
			subprocess.run(['chatctl', 'send', '--as', 'sluice', '--to', 'codex', '--conversation', 'rune-native',
				'--summary', 'ADR0210 supplemental Miri EXIT ' + result['status'], '--body-file', str(notice)], check=True)
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
