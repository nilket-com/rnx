"""A stage's side of the final's fifo jobserver.

The Rust controller (tools/final) owns the pool; Python stage programs
join it through the MAKEFLAGS it sets."""
import os
import re


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
