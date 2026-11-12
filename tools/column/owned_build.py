"""Invocation-owned disposable builds; never a sweep or a shared-cache cleaner."""
import json
import os
from pathlib import Path
import shutil
import signal
import select
import subprocess
import time
import threading
import uuid

from child_result import CONTEXT, atomic_json, digest


def drain_group(pgid):
	"""SIGKILL stops further forks; pidfds wait for exit, not just signal delivery."""
	try: os.killpg(pgid, signal.SIGKILL)
	except ProcessLookupError: return
	fds = []
	try:
		for entry in Path('/proc').iterdir():
			if not entry.name.isdigit(): continue
			try:
				pid = int(entry.name)
				fd = os.pidfd_open(pid)
				try:
					if os.getpgid(pid) == pgid: fds.append(fd)
					else: os.close(fd)
				except BaseException:
					os.close(fd); raise
			except ProcessLookupError: continue
		for fd in fds:
			wait = select.poll(); wait.register(fd, select.POLLIN)
			# A kernel exit notification, with no sampling or progress polling.
			wait.poll()
	finally:
		for fd in fds: os.close(fd)


def run_process(command, *, timeout=None, check=False, termination_grace=5, **kwargs):
	"""Forward launcher termination and drain the isolated producer group."""
	if threading.current_thread() is not threading.main_thread():
		raise RuntimeError('supervised wait requires the main thread')
	child = None
	pending = []
	def interrupted(signum, frame):
		# Popen may still be assigning its result. Defer until it is owned.
		pending.append(signum)
		if child is not None: raise RuntimeError('producer interrupted by signal ' + str(signum))
	previous = {sig: signal.getsignal(sig) for sig in (signal.SIGTERM, signal.SIGINT)}
	try:
		for sig in previous: signal.signal(sig, interrupted)
		child = subprocess.Popen(command, start_new_session=True, **kwargs)
		try:
			if pending: raise RuntimeError('producer interrupted by signal ' + str(pending[0]))
			code = child.wait(timeout=timeout)
		except BaseException:
			# A second ordinary signal must not interrupt descendant termination.
			for sig in previous: signal.signal(sig, signal.SIG_IGN)
			try: child.send_signal(signal.SIGTERM)
			except ProcessLookupError: pass
			try: child.wait(timeout=termination_grace)
			except subprocess.TimeoutExpired: pass
			# Kill remaining group members even if their leader already exited.
			drain_group(child.pid)
			child.wait()
			raise
	finally:
		for sig, handler in previous.items(): signal.signal(sig, handler)
	if check and code: raise subprocess.CalledProcessError(code, command)
	return subprocess.CompletedProcess(command, code)


class OwnedBuild:
	def __init__(self, parent, name):
		self.parent = Path(parent).absolute()
		if self.parent.resolve(strict=True) != self.parent or Path(name).name != name or name in {'.', '..', ''}:
			raise ValueError('noncanonical build owner or outside path')
		self.path = self.parent / name
		if self.path.exists() or self.path.is_symlink():
			raise ValueError('shared or preexisting build path')
		self.record = self.parent / (name + '-OWNER.json')
		if self.record.exists(): raise ValueError('ownership already recorded')
		stat = self.parent.stat()
		self.owner = dict(schema=1, path=str(self.path), parent=str(self.parent),
			parent_device=stat.st_dev, parent_inode=stat.st_ino, invocation=uuid.uuid4().hex)
		atomic_json(self.record, self.owner)
		self.retained = []
		self.result = None

	def __enter__(self): return self

	def keep(self, paths):
		self.retained.extend(Path(p) for p in paths)

	def release(self, original=None):
		start = time.monotonic()
		row = dict(status='FAIL', owner=self.owner, original_error=repr(original) if original else None)
		try:
			if json.loads(self.record.read_text()) != self.owner: raise ValueError('ownership changed')
			stat = self.parent.stat()
			if self.parent.resolve() != self.parent or (stat.st_dev, stat.st_ino) != (self.owner['parent_device'], self.owner['parent_inode']):
				raise ValueError('build owner replaced')
			if self.path.is_symlink() or self.path.resolve() != self.path:
				raise ValueError('symlink or outside build path')
			# Preserve top-level diagnostics too, including on failed compilation.
			paths = set(self.retained) | {p for p in self.parent.iterdir() if p.is_file() and not p.is_symlink()}
			hashes = {}
			for p in paths:
				p = p.resolve(strict=True)
				if p.is_relative_to(self.path): raise ValueError('build output is retained evidence')
				with p.open('rb') as stream: os.fsync(stream.fileno())
				hashes[str(p)] = digest(p)
			fd = os.open(self.parent, os.O_RDONLY | os.O_DIRECTORY)
			try: os.fsync(fd)
			finally: os.close(fd)
			seen = set(); logical = allocated = 0
			for directory, _, files in os.walk(self.path, followlinks=False):
				for name in files:
					s = (Path(directory)/name).lstat(); key = (s.st_dev, s.st_ino)
					if key not in seen:
						seen.add(key); logical += s.st_size; allocated += s.st_blocks * 512
			row.update(logical_file_bytes=logical, allocated_file_bytes=allocated, retained=hashes)
			if self.path.exists(): shutil.rmtree(self.path)
			if self.path.exists() or self.path.is_symlink(): raise ValueError('build output remains')
			if any(digest(p) != h for p, h in hashes.items()): raise ValueError('retained evidence changed')
			row.update(status='PASS', removed=True)
		except Exception as error:
			row['error'] = repr(error)
		finally:
			row['seconds'] = time.monotonic() - start
			self.result = row
			journal = self.parent / (self.path.name + '-RELEASE.json')
			atomic_json(journal, row)
			if os.environ.get(CONTEXT):
				context = Path(os.environ[CONTEXT])
				atomic_json(context.parent / ('CLEANUP-' + self.owner['invocation'] + '.json'),
					dict(path=str(journal), sha256=digest(journal), seconds=row['seconds'], status=row['status']))
		if row['status'] != 'PASS':
			if original is not None: original.add_note('build removal failed: ' + row['error'])
			else: raise RuntimeError('build removal failed: ' + row['error'])
		return row

	def __exit__(self, kind, error, tb):
		try: self.release(error)
		except Exception as cleanup_error:
			if error is None: raise
			error.add_note('cleanup receipt failure: ' + repr(cleanup_error))
		return False
