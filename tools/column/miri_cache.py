"""Reusable Miri preparation and exact-test cache; failures are never retried as flakes."""
import fcntl
import hashlib
import json
import re
import uuid
import subprocess
from pathlib import Path
import sys
import time
import traceback

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'rune/tools/xtask/supplemental'))
from receipts import Run, atomic_json, digest, identity, sync_directory


def input_files(root, package='rune', features='', toolchain=None):
	"""Conservatively bind tracked inputs of every local transitive dependency."""
	root = Path(root).resolve()
	command = ['cargo'] + (['+' + toolchain] if toolchain else []) + ['metadata', '--locked', '--offline', '--format-version=1']
	if features: command += ['--features', ','.join(package + '/' + f for f in features.split(',') if f)]
	data = json.loads(subprocess.check_output(command, cwd=root))
	packages = {p['id']: p for p in data['packages']}
	starts = [p['id'] for p in data['packages'] if p['name'] == package and p['source'] is None]
	if len(starts) != 1:
		raise ValueError('ambiguous local package')
	nodes = {n['id']: n for n in data['resolve']['nodes']}
	seen = set(); pending = starts[:]
	while pending:
		item = pending.pop()
		if item in seen: continue
		seen.add(item); pending.extend(nodes[item]['dependencies'])
	directories = [Path(packages[p]['manifest_path']).parent.resolve() for p in seen if packages[p]['source'] is None]
	if any(not p.is_relative_to(root) for p in directories):
		raise ValueError('external path dependency requires an explicit input inventory')
	tracked = subprocess.check_output(['git', 'ls-files', '-z', '--cached', '--others', '--exclude-standard'], cwd=root).decode().split('\0')
	paths = set()
	for name in filter(None, tracked):
		path = root / name
		if any(path.is_relative_to(d) for d in directories): paths.add(name)
	for directory in directories + [root]:
		for parent in [directory, *directory.parents]:
			if not parent.is_relative_to(root): continue
			for name in ('Cargo.toml', 'Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml', '.cargo/config', '.cargo/config.toml'):
				path = parent / name
				if path.is_file(): paths.add(str(path.relative_to(root)))
	return {p: digest(root / p) for p in sorted(paths)}


def description(root, files, toolchain, flags, features, tools, environment, package='rune'):
	# No revision, plan/docs, requested test set or scheduling choices in the key.
	# Byte-sensitive crate inputs deliberately fail closed: an in-crate source
	# edit needs new evidence even when described as a lint-only edit.
	return dict(schema=1, root=str(Path(root).resolve()), inputs=files,
		toolchain=toolchain, miriflags=flags, features=sorted(features.split(',')),
		tools=tools, environment=environment, profile='engine-dev', package=package)


class CacheRun(Run):
	def begin(self, name):
		job = self.directory / name
		job.mkdir(exist_ok=True); sync_directory(self.directory)
		attempts = sorted(job.glob('attempt-*'))
		for attempt in attempts:
			path = attempt / 'RESULT.json'
			if not path.exists(): continue  # Killed before terminal receipt; never a PASS.
			row = json.loads(path.read_text())
			if row.get('identity') != self.key: raise ValueError('cache identity differs')
			if row.get('status') != 'PASS':
				if row.get('error_type') == 'TimeoutExpired': continue
				raise ValueError('semantic failure retained; no automatic retry')
			if not row.get('artifacts'): raise ValueError('missing cached evidence')
			for relative, sha in row['artifacts'].items():
				artifact = attempt / relative
				if not artifact.resolve().is_relative_to(attempt.resolve()) or digest(artifact) != sha:
					raise ValueError('cached artifact corrupt or missing')
			return None, row
		attempt = job / f'attempt-{len(attempts)+1:04d}'
		attempt.mkdir(); sync_directory(job)
		atomic_json(attempt/'START.json', {'identity':self.key, 'job':name})
		return attempt, None


class Cache:
	def __init__(self, directory, recipe):
		self.directory = Path(directory); self.recipe = recipe

	def key(self, kind, name):
		return dict(recipe=self.recipe, kind=kind, test=name)

	def stage(self, kind, name, action):
		desc = self.key(kind, name); directory = self.directory / identity(desc)
		# Serialize identical-key invocations; no competing writer or partial reuse.
		self.directory.mkdir(parents=True, exist_ok=True)
		with (self.directory / (identity(desc) + '.lock')).open('a+') as lock:
			fcntl.flock(lock, fcntl.LOCK_EX)
			with CacheRun(directory, desc, (directory/'IDENTITY.json').exists()) as run:
				attempt, row = run.begin('job')
				if row is not None: return dict(row, reused=True, cache_key=identity(desc))
				start = time.monotonic()
				try:
					details, artifacts = action(attempt)
					row = run.finish(attempt, 'PASS', artifacts, **dict(details, seconds=details.get('seconds', time.monotonic()-start)))
				except Exception as error:
					path=attempt/'ERROR.txt'; path.write_text(traceback.format_exc())
					run.finish(attempt, 'FAIL', [p for p in attempt.iterdir() if p.is_file()], error_type=type(error).__name__, error=repr(error), seconds=time.monotonic()-start)
					raise
				return dict(row, reused=False, cache_key=identity(desc))

	def invalidate(self, key, reason):
		"""Explicit regeneration preserves the rejected evidence; never auto-retry."""
		if not re.fullmatch('[0-9a-f]{64}', key) or not reason.strip():
			raise ValueError('full cache key and reason required')
		with (self.directory / (key + '.lock')).open('a+') as lock:
			fcntl.flock(lock, fcntl.LOCK_EX)
			source = self.directory/key
			if not source.is_dir() or source.is_symlink(): raise ValueError('missing cache key')
			archive = self.directory/'.invalidated'; archive.mkdir(exist_ok=True)
			target = archive/(key+'-'+uuid.uuid4().hex)
			source.rename(target)
			sync_directory(archive); sync_directory(self.directory)
			atomic_json(target/'INVALIDATED.json', dict(key=key, reason=reason))
			return target

	def order(self, jobs):
		# Timings survive source-key changes, but never authorize result reuse.
		costs = {}
		for path in self.directory.glob('*/job/attempt-*/RESULT.json'):
			row=json.loads(path.read_text())
			if row.get('status') != 'PASS': continue
			for name in row.get('names', []): costs[name]=max(costs.get(name,0), row.get('seconds',0))
		return sorted(jobs, key=lambda spec: (-max((costs.get(n,0) for n in spec['names']), default=0), spec['name']))
