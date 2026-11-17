"""The Python stage programs' side of the final's jobserver: native contract
cases take their stage's tokens and the pool's, and host budgets read its width."""
import multiprocessing
import os
from pathlib import Path
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch


class Pool:
	"""A fifo of tokens, as the Rust controller's pool is."""
	def __init__(self, path, tokens):
		self.path = Path(path); self.tokens = tokens
		os.mkfifo(self.path, 0o600)
		self.fd = os.open(self.path, os.O_RDWR | os.O_NONBLOCK)
		os.write(self.fd, b'+' * tokens)

	def environment(self):
		flags = f'-j{self.tokens} --jobserver-auth=fifo:{self.path}'
		return {'MAKEFLAGS': flags, 'CARGO_MAKEFLAGS': flags}

	def take(self, count=1):
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


def _work(send, execute):
	try:
		done = execute()
		value = dict(returncode=done.returncode, validation=done.validation)
	except BaseException as error:
		value = dict(returncode=1, error=repr(error))
	send.send(value); send.close()


class Forked:
	"""A function in a forked worker, so a wait that would hang can be observed."""
	def __init__(self, execute):
		context = multiprocessing.get_context('fork')
		self.receive, send = context.Pipe(duplex=False)
		self.process = context.Process(target=_work, args=(send, execute), daemon=False)
		self.process.start(); send.close()

	def result(self):
		try: value = self.receive.recv()
		except EOFError: value = dict(returncode=1, error='worker exited without a result')
		self.process.join(); self.receive.close()
		return value


class Jobserver(unittest.TestCase):
	def test_an_exclusive_case_takes_what_its_stage_does_not_hold(self):
		import native_contracts
		with tempfile.TemporaryDirectory() as tmp:
			pool = Pool(Path(tmp)/'jobserver.fifo', 4)
			try:
				stage = pool.take(2)  # a stage of weight two
				seen = []
				def case(c, *a): seen.append(pool.take()); return {'name': c['name'], 'status': 'PASS'}
				env = dict(pool.environment(), RNX_FINAL_STAGE_TOKENS='2')
				with patch.dict(os.environ, env), patch.object(native_contracts, 'run_case', side_effect=case), \
						patch('host_budget.case_width', return_value=1):
					# In a worker, so waiting on the stage's own tokens fails instead of hanging.
					worker = Forked(lambda: SimpleNamespace(returncode=0, validation=(
						native_contracts.run_cases([{'name': 'exclusive', 'exclusive': True}], None, None, None, None), seen)[1]))
				finished = worker.receive.poll(30)
				if not finished: pool.give(b'++')
				value = worker.result()
				pool.give(stage)
				self.assertTrue(finished, 'the exclusive case waited on tokens its stage holds')
				self.assertEqual(value['validation'], [None])
			finally: pool.close()

	def test_fake_case_tests_ignore_an_ambient_final_pool(self):
		import test_native_contracts
		with tempfile.TemporaryDirectory() as tmp:
			pool = Pool(Path(tmp)/'jobserver.fifo', 1)
			held = pool.take()  # a running stage holds the only token
			with patch.dict(os.environ, pool.environment()):
				suite = unittest.defaultTestLoader.loadTestsFromTestCase(test_native_contracts.Concurrency)
				worker = Forked(lambda: SimpleNamespace(returncode=0, validation=unittest.TextTestRunner(stream=open(os.devnull, 'w')).run(suite).wasSuccessful()))
			finished = worker.receive.poll(60)
			if not finished:
				for _ in range(16): pool.give(b'+')
			value = worker.result()
			pool.give(held); pool.close()
			self.assertTrue(finished, 'fake cases waited on the ambient pool')
			self.assertTrue(value['validation'])

	def test_inside_a_final_stages_use_its_budget(self):
		import host_budget
		with patch.dict(os.environ, {'MAKEFLAGS': '-j7 --jobserver-auth=fifo:/nonexistent'}):
			self.assertEqual(host_budget.build_jobs(), 7)
			self.assertEqual(host_budget.case_width(), 3)
		with patch.dict(os.environ, {'MAKEFLAGS': '-j7'}):
			self.assertIsNone(host_budget.inherited_width())


if __name__ == '__main__': unittest.main()
