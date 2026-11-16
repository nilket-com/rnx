"""Concurrent final stages: declared resources, one jobserver, a worker per stage.

A stage writes Cargo target directories, one per profile it builds. A stage
starts once every earlier stage that shares one of them has finished, so
builds into one directory keep selection order and never wait on Cargo's
lock. Every stage but the fixed-route screen joins one fifo jobserver; the
controller holds a token for each running stage, which covers the first job
of the Cargo it starts."""
import json
import multiprocessing
from multiprocessing import connection
import os
from pathlib import Path
import re
import signal
import subprocess
import traceback

GIB = 1024 ** 3
# Before anything else: the commit checks and the dependency fetches.
FIRST = 'first'
# Nothing else runs while it does: its screen needs a quiet machine.
ALONE = 'alone'
ALONE_STAGES = ('fixed-route',)
# Bytes each owned bulk root may reach, rounded up from complete finals.
# The native contracts' owned build target is removed before the manifest
# measures its root, so its estimate is a full engine-dev test build's.
BULK_BYTES = {'baseline': 2*GIB, 'cross': GIB, 'fixed': GIB, 'project': GIB, 'contracts': 6*GIB}
# Wrapper stages, by script: what each builds beyond the manifests it names.
WRAPPERS = {
	'tools/column/clippy.py': None,  # resolved by its --adapter flag
	'tools/column/suite_stage.py': None,  # resolved from its recipes
	'tools/column/inventories.py': (('target/column-inventory-extract', 'debug'), ('tools/polars-gen/Cargo.toml', 'debug'),
		('target/0072', '')),  # the inventory documentation's own Cargo targets
	'tools/column/jupyter_controls.py': (('Cargo.toml', 'debug'), ('jupyter/Cargo.toml', 'debug')),
	'tools/column/cli.py': (('adapters/polars/Cargo.toml', 'release'),),
	'tools/verify.py': (),  # cross builds under its own owned output
	'tools/column/fixed_route_gate.py': (),
	'ci/preflight.py': (),
	'tools/test_verify_pins.py': (),
}
UNITTEST = ('-m', 'unittest')
# Builds a stage's tests start themselves, beyond its own commands.
NESTED = {'polars-suite': (('target/0073', ''),)}
# Test threads where a command names none (RUST_TEST_THREADS in every stage).
TEST_THREADS = 4
# A Cargo target directory keeps every artifact it has built, so a warm one
# grows only by what a candidate rebuilds; a missing one grows to its full size.
WARM_GROWTH_BYTES = GIB
WARM_GROWTH_SHARE = 0.05
COLD_BYTES = 4*GIB


def profile(command):
	if '--release' in command: return 'release'
	if '--profile' in command:
		name = command[command.index('--profile')+1]
		return {'dev': 'debug', 'test': 'debug', 'bench': 'release'}.get(name, name)
	return 'debug'


def cargo_verb(command):
	return next(arg for arg in command[1:] if not arg.startswith('+'))


def manifest(command):
	return command[command.index('--manifest-path')+1] if '--manifest-path' in command else 'Cargo.toml'


def script(command):
	return next((arg for arg in command[1:] if arg.endswith('.py')), None)


def declared(name, command):
	"""Refuse a stage whose writes the scheduler cannot name."""
	if name in ALONE_STAGES or name == 'commit-discipline' or name.startswith('fetch-'): return
	if Path(command[0]).name == 'cargo': return
	if command[0] == 'python3' and (script(command) in WRAPPERS or tuple(command[2:4]) == UNITTEST): return
	raise ValueError('stage writes are not declared: ' + name)


class Targets:
	"""Each manifest's Cargo target directory, as Cargo resolves it."""
	def __init__(self, root):
		self.root = Path(root); self.known = {}

	def __call__(self, path):
		if not path.endswith('Cargo.toml'): return self.root/path
		if path not in self.known:
			meta = subprocess.check_output(['cargo', '+1.98.1', 'metadata', '--format-version', '1', '--no-deps',
				'--offline', '--manifest-path', path], cwd=self.root)
			self.known[path] = Path(json.loads(meta)['target_directory'])
		return self.known[path]


def resources(name, command, root, targets=None):
	"""The set of (target directory/profile) a stage writes, or FIRST/ALONE."""
	declared(name, command)
	if name in ALONE_STAGES: return frozenset({ALONE})
	if name == 'commit-discipline' or name.startswith('fetch-'): return frozenset({FIRST})
	targets = targets or Targets(root)
	def cargo(argv):
		if cargo_verb(argv) in {'fmt', 'fetch', 'metadata'}: return set()
		return {str(targets(manifest(argv))/profile(argv))}
	nested = {str(targets(where)/kind) for where, kind in NESTED.get(name, ())}
	if Path(command[0]).name == 'cargo': return frozenset(cargo(command) | nested)
	path = script(command)
	if path == 'tools/column/suite_stage.py':
		from suite_stage import recipes
		stage = command[command.index('--stage')+1]
		return frozenset(set().union(*(cargo(r) for r in recipes(stage, root))))
	if path == 'tools/column/clippy.py':
		prefix = 'column-clippy-' + ('adapter-' if '--adapter' in command else '')
		return frozenset(str(targets('target/'+prefix+role)/'debug') for role in ('published', 'candidate'))
	return frozenset(str(targets(where)/kind) for where, kind in WRAPPERS.get(path, ()))


def test_threads(command):
	"""Threads a test command runs: its --test-threads, else the final's default."""
	for arg in command:
		if arg.startswith('--test-threads='): return int(arg.split('=', 1)[1])
	return TEST_THREADS


def weight(name, command, root):
	"""Tokens a stage holds while it runs: its widest test run, else one.

	A test run's threads and their children use CPUs and memory as builds do,
	so they hold tokens for them rather than borrowing the pool's."""
	if Path(command[0]).name == 'cargo':
		return test_threads(command) if cargo_verb(command) == 'test' else 1
	path = script(command)
	if path == 'tools/column/suite_stage.py':
		from suite_stage import recipes
		stage = command[command.index('--stage')+1]
		# suite_stage runs every test recipe with --test-threads=4.
		return 4 if any('test' in r for r in recipes(stage, root)) else 1
	if path == 'tools/verify.py': return TEST_THREADS
	return 1


def stage_environment(name, command, pool, held=1):
	"""The jobserver for every stage but the fixed-route screen and Python test suites.

	Python suites run fake stages and cases of their own; a test that needs a
	pool makes a private one."""
	if name in ALONE_STAGES or tuple(command[2:4]) == UNITTEST or script(command) == 'tools/test_verify_pins.py':
		return {}
	return dict(pool.environment(), RUST_TEST_THREADS=str(TEST_THREADS), RNX_FINAL_STAGE_TOKENS=str(held))


def growth(resource, root):
	"""Bytes a stage's Cargo target directory may add: a share of a warm one, all of a missing one."""
	path = Path(resource)
	if resource in (FIRST, ALONE) or not str(path).startswith(str(root)): return 0
	try: size = sum(p.stat().st_size for p in path.rglob('*') if p.is_file() and not p.is_symlink())
	except FileNotFoundError: size = 0
	return max(WARM_GROWTH_BYTES, int(size*WARM_GROWTH_SHARE)) if size else COLD_BYTES


def conflict(a, b):
	return bool(a & b) or FIRST in a or FIRST in b or ALONE in a or ALONE in b


def ready(index, plan, finished, running, producers=(), succeeded=None):
	"""Its declared producers have passed and every earlier stage sharing a
	resource has finished; ALONE waits for quiet."""
	if ALONE in plan[index] and running: return False
	if not all(j in (finished if succeeded is None else succeeded) for j in producers): return False
	return all(j in finished for j in range(index) if conflict(plan[index], plan[j]))


def producers(stages):
	"""Selection indices of each stage's declared prerequisites (accept.PREREQUISITES)."""
	from accept import PREREQUISITES
	order = {name: i for i, (name, _) in enumerate(stages)}
	return [tuple(order[p] for p in PREREQUISITES.get(name, ()) if p in order) for name, _ in stages]


def declared_bulk(name):
	from accept import BULK_ROOTS
	return sum(BULK_BYTES[kind] for kind in BULK_ROOTS.get(name, ()))


class Pool:
	"""The final's jobserver: a fifo of tokens shared by every Cargo, rustc and case."""
	def __init__(self, path, tokens):
		if tokens < 1: raise ValueError('a jobserver needs one token')
		self.path = Path(path); self.tokens = tokens
		os.mkfifo(self.path, 0o600)
		self.fd = os.open(self.path, os.O_RDWR | os.O_NONBLOCK)
		os.write(self.fd, b'+' * tokens)

	def environment(self):
		flags = f'-j{self.tokens} --jobserver-auth=fifo:{self.path}'
		return {'MAKEFLAGS': flags, 'CARGO_MAKEFLAGS': flags}

	def take(self, count=1):
		"""count tokens at once or none: the controller never waits holding a part."""
		taken = b''
		while len(taken) < count:
			try: token = os.read(self.fd, count-len(taken))
			except BlockingIOError: token = b''
			if not token:
				if taken: self.give(taken)
				return None
			taken += token
		return taken

	def give(self, token): os.write(self.fd, token)

	def close(self):
		os.close(self.fd); self.path.unlink(missing_ok=True)


class Client:
	"""A stage's side of the final's jobserver, found in MAKEFLAGS."""
	def __init__(self, path, capacity, held=1):
		self.capacity = capacity; self.held = held; self.path = path; self.fd = os.open(path, os.O_RDWR)

	@classmethod
	def from_environment(cls, environment=None):
		environment = os.environ if environment is None else environment
		flags = environment.get('MAKEFLAGS', '')
		auth = re.search(r'--jobserver-auth=fifo:(\S+)', flags); width = re.search(r'(?:^|\s)-j(\d+)', flags)
		# The tokens the controller holds for this stage, idle while its builds are done.
		held = int(environment.get('RNX_FINAL_STAGE_TOKENS', '1'))
		return cls(auth.group(1), int(width.group(1)), held) if auth and width else None

	def acquire(self, count):
		tokens = b''
		while len(tokens) < count: tokens += os.read(self.fd, count-len(tokens))
		return tokens

	def acquire_one(self, own):
		"""One token from the stage's own idle slot (a pipe) or the pool, whichever frees first.

		The controller already holds a token for this stage; while no Cargo of the
		stage runs, that slot carries one case, so a one-token pool still finishes."""
		import select
		# Both descriptors stay nonblocking: the own pipe is set so once before the
		# case workers fork, and the pool is read through a private nonblocking open.
		if os.get_blocking(own[0]): raise ValueError('the stage slot pipe must be nonblocking')
		pool = os.open(self.path, os.O_RDWR | os.O_NONBLOCK)
		try:
			while True:
				readable, _, _ = select.select([own[0], pool], [], [])
				for fd in readable:
					try: token = os.read(fd, 1)
					except BlockingIOError: continue
					if token: return (own[0] if fd == own[0] else self.fd), token
		finally: os.close(pool)

	def release(self, tokens): os.write(self.fd, tokens)

	def close(self): os.close(self.fd)


def _work(send, execute):
	try:
		done = execute()
		value = dict(returncode=done.returncode, validation=done.validation,
			declared_output=getattr(done, 'declared_output', None))
	except BaseException:
		value = dict(returncode=1, error=traceback.format_exc())
	finally:
		# The receipt is written; a late signal may not lose its delivery.
		for sig in (signal.SIGTERM, signal.SIGINT): signal.signal(sig, signal.SIG_IGN)
	send.send(value); send.close()


class Forked:
	"""A stage in a forked worker, whose main thread supervises its child."""
	def __init__(self, execute):
		context = multiprocessing.get_context('fork')
		self.receive, send = context.Pipe(duplex=False)
		self.process = context.Process(target=_work, args=(send, execute), daemon=False)
		self.process.start(); send.close()

	def waitable(self): return self.receive

	def ready(self): return self.receive.poll()

	def result(self):
		try: value = self.receive.recv()
		except EOFError: value = dict(returncode=1, error='stage worker exited without a result')
		self.process.join(); self.receive.close()
		return value

	def terminate(self):
		if self.process.is_alive(): self.process.terminate()


class Inline:
	"""The same stage run in the controller's process, for controller tests."""
	def __init__(self, execute):
		done = execute()
		self.value = dict(returncode=done.returncode, validation=done.validation,
			declared_output=getattr(done, 'declared_output', None))

	def waitable(self): return None

	def ready(self): return True

	def result(self): return self.value

	def terminate(self): pass


def wait(handles, timeout):
	objects = [h.waitable() for h in handles if h.waitable() is not None]
	if any(h.ready() for h in handles): return
	if objects: connection.wait(objects, timeout=timeout)
