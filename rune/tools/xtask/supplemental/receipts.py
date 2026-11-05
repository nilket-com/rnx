"""Durable job receipts: exact identity, immutable attempts, explicit resume."""
import fcntl
import hashlib
import json
import os
import tempfile
from pathlib import Path


def digest(path):
	h = hashlib.sha256()
	with Path(path).open('rb') as stream:
		for block in iter(lambda: stream.read(1024 * 1024), b''):
			h.update(block)
	return h.hexdigest()


def identity(value):
	return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def sync_directory(path):
	fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
	try:
		os.fsync(fd)
	finally:
		os.close(fd)


def atomic_json(path, value):
	path = Path(path)
	fd, name = tempfile.mkstemp(prefix='.' + path.name + '.', suffix='.pending', dir=path.parent)
	tmp = Path(name)
	try:
		with os.fdopen(fd, 'w') as stream:
			json.dump(value, stream, sort_keys=True, indent=2)
			stream.write('\n')
			stream.flush()
			os.fsync(stream.fileno())
		os.replace(tmp, path)
		sync_directory(path.parent)
	finally:
		tmp.unlink(missing_ok=True)


class Run:
	def __init__(self, directory, description, resume=False):
		self.directory = Path(directory)
		self.description = description
		self.key = identity(description)
		self.resume = resume
		self.lock = None

	def __enter__(self):
		self.directory.mkdir(parents=True, exist_ok=True)
		self.lock = (self.directory / 'LOCK').open('a+')
		try:
			fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
			path = self.directory / 'IDENTITY.json'
			if path.exists():
				if not self.resume:
					raise ValueError('existing run requires explicit resume')
				if json.loads(path.read_text()) != self.description:
					raise ValueError('run identity differs')
			else:
				if self.resume:
					raise ValueError('resume requires an existing identity')
				if any(p.name != 'LOCK' and not (p.is_file() and p.name.startswith('.IDENTITY.json.') and p.name.endswith('.pending')) for p in self.directory.iterdir()):
					raise ValueError('nonempty uninitialized run')
				atomic_json(path, self.description)
			return self
		except BaseException:
			self.lock.close()
			raise

	def __exit__(self, *_):
		self.lock.close()

	def begin(self, name):
		if not name or any(c not in 'abcdefghijklmnopqrstuvwxyz0123456789_-' for c in name):
			raise ValueError('invalid job name')
		job = self.directory / name
		job.mkdir(exist_ok=True)
		sync_directory(self.directory)
		attempts = sorted(job.glob('attempt-*'))
		for attempt in attempts:
			result = attempt / 'RESULT.json'
			if not result.exists():
				if not self.resume:
					raise ValueError('interrupted attempt requires explicit resume')
				continue
			row = json.loads(result.read_text())
			if row.get('identity') != self.key:
				raise ValueError('receipt identity differs')
			if row.get('status') != 'PASS':
				raise ValueError('completed failure cannot be retried')
			if not row.get('artifacts'):
				raise ValueError('receipt has no evidence')
			for relative, expected in row['artifacts'].items():
				path = Path(relative)
				if path.is_absolute() or '..' in path.parts:
					raise ValueError('artifact escapes attempt')
				artifact = attempt / path
				if not artifact.resolve().is_relative_to(attempt.resolve()) or digest(artifact) != expected:
					raise ValueError('artifact missing or corrupt')
			return None, row
		attempt = job / f'attempt-{len(attempts) + 1:04d}'
		attempt.mkdir()
		sync_directory(job)
		atomic_json(attempt / 'START.json', {'identity': self.key, 'job': name})
		return attempt, None

	def finish(self, attempt, status, artifacts, **details):
		if status not in ('PASS', 'FAIL'):
			raise ValueError('invalid terminal status')
		if (attempt / 'RESULT.json').exists():
			raise ValueError('terminal receipt is immutable')
		if not artifacts:
			raise ValueError('receipt requires evidence')
		row = dict(details, identity=self.key, status=status, artifacts={})
		directories = {attempt}
		for path in artifacts:
			path = Path(path)
			if not path.resolve().is_relative_to(attempt.resolve()):
				raise ValueError('artifact escapes attempt')
			with path.open('rb') as stream:
				os.fsync(stream.fileno())
			parent = path.parent
			while parent.is_relative_to(attempt):
				directories.add(parent)
				parent = parent.parent
			row['artifacts'][str(path.relative_to(attempt))] = digest(path)
		for directory in sorted(directories, key=lambda p: len(p.parts), reverse=True):
			sync_directory(directory)
		atomic_json(attempt / 'RESULT.json', row)
		return row
