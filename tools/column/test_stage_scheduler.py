"""Concurrent stages: declared resources, order, failure, signals, jobserver, disk."""
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import time
from types import SimpleNamespace
import unittest
from unittest.mock import patch

import child_result as child
import final
from final_evidence import Owners
import stage_scheduler as scheduler

ROOT = Path(__file__).resolve().parents[2]


def selection(stages):
	return {'record': {'candidate': 'c'}, 'source': 's', 'policy': 'p', 'stages': [[n, [n]] for n in stages]}


class Fake:
	"""Each fake stage logs its start and end times and sleeps or fails as told."""
	def __init__(self, root, behavior):
		self.log = root/'events'; self.log.mkdir(); self.behavior = behavior

	def __call__(self, command, **kw):
		name = command[0]; seconds, code = self.behavior.get(name, (0.3, 0))
		(self.log/(name+'.start')).write_text(repr(time.monotonic()))
		time.sleep(seconds)
		(self.log/(name+'.end')).write_text(repr(time.monotonic()))
		d = kw['directory']; d.mkdir()
		child.atomic_json(d/'RESULT.json', dict(status='FAIL' if code else 'PASS', process_exit=code, error='planted failure' if code else None))
		return SimpleNamespace(returncode=code, validation={'path': str(d/'RESULT.json'), 'sha256': child.digest(d/'RESULT.json')})

	def span(self, name):
		read = lambda suffix: float((self.log/(name+suffix)).read_text())
		return read('.start'), read('.end')

	def ran(self, name): return (self.log/(name+'.start')).exists()


class Scheduling(unittest.TestCase):
	def run_fake(self, plan, behavior=None, tokens=4, rows=None):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); out = root/'out'; out.mkdir()
			fake = Fake(root, behavior or {}); rows = [] if rows is None else rows
			owner = Owners(root/'registry', out)
			try:
				with patch.object(final.child, 'run', side_effect=fake), patch.object(final.gate, 'check_selection'), \
						patch.object(final.scheduler, 'resources', lambda name, *a: frozenset(plan[name])):
					try:
						final.run_stages(root, out, selection(list(plan)), owner, {'CARGO_BUILD_JOBS': str(tokens)}, {}, rows)
						error = None
					except final.StageFailed as failed: error = failed
				return fake, rows, error, {n: fake.span(n) for n in plan if fake.ran(n)}
			finally: owner.close()

	@staticmethod
	def overlap(a, b): return a[0] < b[1] and b[0] < a[1]

	def test_disjoint_overlap_shared_keep_order_and_alone_overlaps_nothing(self):
		_, rows, error, spans = self.run_fake({'a': {'x'}, 'b': {'y'}, 'c': {'x'}, 'fixed-route': {scheduler.ALONE}, 'd': {'z'}})
		self.assertIsNone(error)
		self.assertTrue(self.overlap(spans['a'], spans['b']))
		self.assertFalse(self.overlap(spans['a'], spans['c'])); self.assertLess(spans['a'][1], spans['c'][0])
		for name in ('a', 'b', 'c', 'd'): self.assertFalse(self.overlap(spans['fixed-route'], spans[name]), name)
		self.assertEqual([r['name'] for r in rows], ['a', 'b', 'c', 'fixed-route', 'd'])

	def test_first_stages_run_before_everything(self):
		_, _, _, spans = self.run_fake({'commit-discipline': {scheduler.FIRST}, 'a': {'x'}, 'b': {'y'}})
		self.assertLess(spans['commit-discipline'][1], min(spans['a'][0], spans['b'][0]))

	def test_failure_stops_admission_and_running_stages_keep_receipts(self):
		_, rows, error, spans = self.run_fake({'slow': {'y'}, 'broken': {'x'}, 'after': {'x'}, 'other': {'z'}},
			{'slow': (1.0, 0), 'broken': (0.1, 1), 'other': (0.1, 0)}, tokens=2)
		self.assertIsInstance(error, final.StageFailed)
		self.assertEqual(error.row['name'], 'broken')
		self.assertNotIn('after', spans); self.assertNotIn('other', spans)
		self.assertEqual({r['name']: r['exit'] for r in rows}, {'slow': 0, 'broken': 1})
		self.assertIn('slow', [r['name'] for r in rows])

	def test_a_declared_producer_finishes_before_its_consumer(self):
		import accept
		with patch.dict(accept.PREREQUISITES, {'consumer': ('producer',)}):
			_, _, error, spans = self.run_fake({'producer': {'x'}, 'consumer': {'y'}}, {'producer': (1.0, 0), 'consumer': (0.1, 0)})
		self.assertIsNone(error)
		self.assertLess(spans['producer'][1], spans['consumer'][0])

	def test_a_failure_that_already_returned_stops_the_next_admission(self):
		def slow_check(*a, **k): time.sleep(0.5)  # each admission rechecks the selection
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); out = root/'out'; out.mkdir(); rows = []
			fake = Fake(root, {'broken': (0.0, 1), 'b': (0.1, 0), 'c': (0.1, 0)})
			owner = Owners(root/'registry', out); plan = {'broken': {'x'}, 'b': {'y'}, 'c': {'z'}}
			try:
				with patch.object(final.child, 'run', side_effect=fake), patch.object(final.gate, 'check_selection', side_effect=slow_check), \
						patch.object(final.scheduler, 'resources', lambda name, *a: frozenset(plan[name])):
					with self.assertRaises(final.StageFailed):
						final.run_stages(root, out, selection(list(plan)), owner, {'CARGO_BUILD_JOBS': '8'}, {}, rows)
			finally: owner.close()
			self.assertFalse(fake.ran('c'))

	def test_a_test_width_above_the_budget_is_refused_before_any_stage(self):
		for tokens in (1, 2):
			with self.subTest(tokens=tokens), patch.object(final.scheduler, 'weight', return_value=4):
				with self.assertRaisesRegex(ValueError, f'budget of {tokens} tokens is below the test width of a \\(4\\)'):
					self.run_fake({'a': {'x'}}, tokens=tokens)
		fake, _, error, _ = self.run_fake({'a': {'x'}}, tokens=4)
		self.assertIsNone(error)

	def test_tokens_bound_running_stages(self):
		_, _, _, spans = self.run_fake({'a': {'x'}, 'b': {'y'}, 'c': {'z'}}, tokens=1)
		ordered = sorted(spans.values())
		self.assertTrue(all(ordered[i][1] <= ordered[i+1][0] for i in range(2)))

	def test_signal_drains_every_running_stage(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); out = root/'out'; out.mkdir(); rows = []
			fake = Fake(root, {'a': (30, 0), 'b': (30, 0)})
			owner = Owners(root/'registry', out)
			def stop(signum, frame): raise RuntimeError('final interrupted by signal ' + str(signum))
			previous = signal.signal(signal.SIGTERM, stop)
			# A timer signal, not a thread: the workers fork from a single-threaded controller.
			alarm = signal.signal(signal.SIGALRM, lambda *a: os.kill(os.getpid(), signal.SIGTERM))
			try:
				with patch.object(final.child, 'run', side_effect=fake), patch.object(final.gate, 'check_selection'), \
						patch.object(final.scheduler, 'resources', lambda name, *a: frozenset({name})):
					signal.setitimer(signal.ITIMER_REAL, 1.0); started = time.monotonic()
					with self.assertRaisesRegex(RuntimeError, 'interrupted'):
						final.run_stages(root, out, selection(['a', 'b']), owner, {'CARGO_BUILD_JOBS': '4'}, {}, rows)
				self.assertLess(time.monotonic() - started, 20)
				self.assertEqual(sorted((r['name'], r['exit']) for r in rows), [('a', 1), ('b', 1)])
				self.assertFalse((out/'jobserver.fifo').exists())
			finally:
				signal.setitimer(signal.ITIMER_REAL, 0); signal.signal(signal.SIGALRM, alarm)
				signal.signal(signal.SIGTERM, previous); owner.close()


class Admission(unittest.TestCase):
	def test_disk_short_of_estimate_or_remaining_estimate(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); out = root/'out'; out.mkdir()
			owner = Owners(root/'registry', out); fake = Fake(root, {})
			plan = {'a': {'x'}, 'clippy': {'x'}}
			try:
				with patch.object(final.child, 'run', side_effect=fake), patch.object(final.gate, 'check_selection'), \
						patch.object(final.scheduler, 'resources', lambda name, *a: frozenset(plan[name])):
					floor = final.DISK_FLOOR_BYTES; bulk = scheduler.declared_bulk('clippy')
					short = SimpleNamespace(free=floor + bulk - 1)
					with patch.object(final.shutil, 'disk_usage', return_value=short):
						with self.assertRaisesRegex(ValueError, 'disk admission before a'):
							final.run_stages(root, out, selection(['a', 'clippy']), owner, {'CARGO_BUILD_JOBS': '2'}, {}, [])
					self.assertFalse(fake.ran('a'))
					# Enough for both at first; the second stage then finds too little.
					frees = iter([floor + bulk, floor + bulk - 1])
					with patch.object(final.shutil, 'disk_usage', side_effect=lambda p: SimpleNamespace(free=next(frees))):
						with self.assertRaisesRegex(ValueError, 'disk admission before clippy'):
							(out/'again').mkdir(); final.run_stages(root, out/'again', selection(['a', 'clippy']), owner, {'CARGO_BUILD_JOBS': '2'}, {}, [], disk := {})
					self.assertEqual(disk['estimate_bytes'], floor + bulk)
					self.assertEqual(disk['lowest_free_bytes'], floor + bulk - 1)
			finally: owner.close()

	def test_a_cold_target_directory_counts_its_growth(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); out = root/'out'; out.mkdir()
			owner = Owners(root/'registry', out); fake = Fake(root, {})
			cold = str(root/'adapters/large/target/debug')
			try:
				with patch.object(final.child, 'run', side_effect=fake), patch.object(final.gate, 'check_selection'), \
						patch.object(final.scheduler, 'resources', lambda name, *a: frozenset({cold})):
					# Above the floor and owned bulk alone, below the cold directory's growth.
					short = SimpleNamespace(free=final.DISK_FLOOR_BYTES + scheduler.COLD_BYTES - 1)
					with patch.object(final.shutil, 'disk_usage', return_value=short):
						with self.assertRaisesRegex(ValueError, 'disk admission before suite'):
							final.run_stages(root, out, selection(['suite']), owner, {'CARGO_BUILD_JOBS': '2'}, {}, [], disk := {})
				self.assertFalse(fake.ran('suite'))
				self.assertEqual(disk['estimate_bytes'], final.DISK_FLOOR_BYTES + scheduler.COLD_BYTES)
			finally: owner.close()

	def test_every_selected_stage_is_declared_and_resolved(self):
		import accept
		targets = scheduler.Targets(ROOT)
		plan = {name: scheduler.resources(name, command, ROOT, targets) for name, command in accept.commands(ROOT/'target/unused', root=ROOT)}
		target = lambda *parts: str(ROOT.joinpath(*parts))
		self.assertEqual(plan['rune-suite'], {target('target', 'engine-dev')})
		self.assertEqual(plan['polars-suite'], {target('adapters/polars/target', 'debug'), target('target/0073')})
		self.assertIn(target('target/0072'), plan['polars-inventories'])
		self.assertEqual(plan['production-cli-build'], plan['production-cli-controls'])
		self.assertEqual(plan['production-cli-build'], {target('adapters/polars/target', 'release')})
		self.assertIn(target('rune/hegel_tests/target', 'debug'), plan['package-hegel'])
		self.assertEqual(plan['format'], frozenset())
		self.assertEqual(plan['fixed-route'], {scheduler.ALONE})
		self.assertEqual(plan['fetch-Cargo.toml'], {scheduler.FIRST})
		self.assertEqual(plan['clippy'], {target('target/column-clippy-published', 'debug'), target('target/column-clippy-candidate', 'debug')})
		with self.assertRaisesRegex(ValueError, 'not declared'):
			scheduler.declared('new-stage', ['python3', '-B', 'tools/column/new_stage.py'])
		commands = dict(accept.commands(ROOT/'target/unused', root=ROOT))
		weights = {name: scheduler.weight(name, commands[name], ROOT) for name in
			('rune-suite', 'root-tests', 'generated-projects', 'clippy', 'cross', 'package-hegel', 'format', 'column-host', 'fixed-route')}
		self.assertEqual(weights, {'rune-suite': 4, 'root-tests': 4, 'generated-projects': 1, 'clippy': 1, 'cross': 4,
			'package-hegel': 4, 'format': 1, 'column-host': scheduler.TEST_THREADS, 'fixed-route': 1})

	def test_python_suites_and_the_screen_get_no_jobserver(self):
		import accept
		commands = dict(accept.commands(ROOT/'target/unused', root=ROOT))
		with tempfile.TemporaryDirectory() as tmp:
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 4)
			try:
				for name in ('controller-controls', 'benchmark-protocol', 'pin-controls', 'fixed-route'):
					self.assertEqual(scheduler.stage_environment(name, commands[name], pool, 1), {}, name)
				env = scheduler.stage_environment('rune-suite', commands['rune-suite'], pool, 4)
				self.assertIn('--jobserver-auth=fifo:', env['MAKEFLAGS'])
				self.assertEqual((env['RNX_FINAL_STAGE_TOKENS'], env['RUST_TEST_THREADS']), ('4', str(scheduler.TEST_THREADS)))
			finally: pool.close()

	def test_target_growth_is_declared_warm_or_cold(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); warm = root/'warm/debug'; warm.mkdir(parents=True)
			(warm/'artifact').write_bytes(b'x'*1000)
			self.assertEqual(scheduler.growth(str(warm), root), scheduler.WARM_GROWTH_BYTES)
			self.assertEqual(scheduler.growth(str(root/'missing/debug'), root), scheduler.COLD_BYTES)
			self.assertEqual(scheduler.growth(scheduler.ALONE, root), 0)


class Jobserver(unittest.TestCase):
	def workspace(self, root):
		(root/'Cargo.toml').write_text('[workspace]\nmembers=["a","b","c","d"]\nresolver="2"\n')
		for n in 'abcd':
			(root/n/'src').mkdir(parents=True)
			(root/n/'Cargo.toml').write_text(f'[package]\nname="{n}"\nversion="0.1.0"\nedition="2021"\n')
			# Each build script records when it ran, so overlap is measured, not timed.
			(root/n/'build.rs').write_text('use std::time::*;fn now()->u128{SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_micros()}'
				'fn main(){let s=now();std::thread::sleep(Duration::from_millis(500));'
				f'std::fs::write(std::path::Path::new(&std::env::var("STAMPS").unwrap()).join("{n}"),format!("{{}} {{}}",s,now())).unwrap();}}\n')
			(root/n/'src/lib.rs').write_text('')

	def spans(self, root, pool, name):
		stamps = root/('stamps-'+name); stamps.mkdir()
		env = dict(os.environ, CARGO_TARGET_DIR=str(root/name), CARGO_BUILD_JOBS='16', STAMPS=str(stamps), **pool.environment())
		subprocess.run(['cargo', '+1.98.1', 'build', '-q', '--offline'], cwd=root, env=env, check=True, timeout=600)
		return sorted(tuple(map(int, p.read_text().split())) for p in stamps.iterdir())

	def test_cargo_joins_the_final_pool(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp); self.workspace(root)
			pool = scheduler.Pool(root/'jobserver.fifo', 4)
			try:
				held = pool.take(4)
				self.assertIsNone(pool.take())
				serial = self.spans(root, pool, 'held')
				pool.give(held)
				free = self.spans(root, pool, 'free')
			finally: pool.close()
			overlaps = lambda spans: any(a[1] > b[0] for a, b in zip(spans, spans[1:]))
			# With every token held Cargo has only its implicit job: no two scripts overlap.
			self.assertEqual(len(serial), 4); self.assertFalse(overlaps(serial))
			self.assertTrue(overlaps(free))

	def test_exclusive_case_holds_every_token(self):
		import native_contracts
		with tempfile.TemporaryDirectory() as tmp:
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 3)
			try:
				stage = pool.take()  # the controller's token for the running stage
				seen = []
				def case(*a): seen.append(pool.take()); return {'status': 'PASS'}
				with patch.dict(os.environ, pool.environment()), patch.object(native_contracts, 'run_case', side_effect=case):
					native_contracts.with_tokens(2, {'name': 'exclusive'}, None, None, None, None)
					self.assertEqual(seen, [None])
					native_contracts.with_tokens(1, {'name': 'shared'}, None, None, None, None)
					self.assertIsNotNone(seen[1]); pool.give(seen[1])
				pool.give(stage)
				self.assertEqual(len([t for t in iter(pool.take, None)]), 3)
			finally: pool.close()

	def test_one_token_pool_still_finishes_the_cases(self):
		import native_contracts
		with tempfile.TemporaryDirectory() as tmp:
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 1)
			stage = pool.take()  # the controller holds the only token for the running stage
			cases = [{'name': 'exclusive', 'exclusive': True}] + [{'name': f'shared-{i}'} for i in range(3)]
			def case(c, *a): time.sleep(0.2); return {'name': c['name'], 'status': 'PASS'}
			with patch.dict(os.environ, pool.environment()), patch.object(native_contracts, 'run_case', side_effect=case), \
					patch('host_budget.case_width', return_value=3):
				# In a worker, so a deadlock is a failure rather than a hung test.
				worker = scheduler.Forked(lambda: SimpleNamespace(returncode=0, validation=native_contracts.run_cases(cases, None, None, None, None)))
			finished = worker.receive.poll(30)
			if not finished:
				for _ in range(8): pool.give(b'+')  # release the blocked case workers
			value = worker.result()
			pool.give(stage)
			self.assertTrue(finished, 'cases waited on a token the stage holds')
			rows = value['validation']
			self.assertEqual([r['name'] for r in rows], [c['name'] for c in cases])
			self.assertEqual(len([t for t in iter(pool.take, None)]), 1)
			pool.close()

	def test_case_workers_share_a_nonblocking_slot_pipe(self):
		with tempfile.TemporaryDirectory() as tmp:
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 2)
			try:
				stage = pool.take()  # one pool token left; the stage's own slot is a pipe
				own = os.pipe(); os.write(own[1], b'+')
				with self.assertRaisesRegex(ValueError, 'nonblocking'):
					scheduler.Client(pool.path, 2).acquire_one(own)
				os.set_blocking(own[0], False)
				# Two forked workers race for one own slot and one pool token: both get one.
				def take():
					client = scheduler.Client(pool.path, 2)
					fd, token = client.acquire_one(own)
					return SimpleNamespace(returncode=0, validation=(fd == own[0], os.get_blocking(own[0])))
				workers = [scheduler.Forked(take) for _ in range(2)]
				finished = all(w.receive.poll(10) for w in workers)
				if not finished: os.write(own[1], b'++'); pool.give(b'++')
				values = [w.result()['validation'] for w in workers]
				self.assertTrue(finished, 'a worker blocked on the drained slot pipe')
				self.assertEqual(sorted(v[0] for v in values), [False, True])
				self.assertFalse(any(v[1] for v in values))
				pool.give(stage)
				for fd in own: os.close(fd)
			finally: pool.close()

	def test_the_controller_takes_a_stage_weight_whole_or_not_at_all(self):
		with tempfile.TemporaryDirectory() as tmp:
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 3)
			try:
				one = pool.take()
				self.assertIsNone(pool.take(3))
				self.assertEqual(len(pool.take(2)), 2)  # the refused take gave back what it read
			finally: pool.close()

	def test_an_exclusive_case_takes_what_its_stage_does_not_hold(self):
		import native_contracts
		with tempfile.TemporaryDirectory() as tmp:
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 4)
			try:
				stage = pool.take(2)  # a stage of weight two
				seen = []
				def case(c, *a): seen.append(pool.take()); return {'name': c['name'], 'status': 'PASS'}
				env = dict(pool.environment(), RNX_FINAL_STAGE_TOKENS='2')
				with patch.dict(os.environ, env), patch.object(native_contracts, 'run_case', side_effect=case), \
						patch('host_budget.case_width', return_value=1):
					# In a worker, so waiting on the stage's own tokens fails instead of hanging.
					worker = scheduler.Forked(lambda: SimpleNamespace(returncode=0, validation=(
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
			pool = scheduler.Pool(Path(tmp)/'jobserver.fifo', 1)
			held = pool.take()  # a running stage holds the only token
			with patch.dict(os.environ, pool.environment()):
				suite = unittest.defaultTestLoader.loadTestsFromTestCase(test_native_contracts.Concurrency)
				worker = scheduler.Forked(lambda: SimpleNamespace(returncode=0, validation=unittest.TextTestRunner(stream=open(os.devnull, 'w')).run(suite).wasSuccessful()))
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


class SourceWrites(unittest.TestCase):
	def test_no_git_command_in_a_final_refreshes_the_index(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp)
			git = lambda *a, **k: subprocess.run(['git', *a], cwd=root, check=True, capture_output=True, **k)
			git('init'); (root/'tracked.txt').write_text('same')
			git('add', '.'); git('-c', 'user.name=T', '-c', 'user.email=t@invalid', '-c', 'commit.gpgsign=false', 'commit', '-m', 'f')
			stamp = lambda: (root/'.git/index').stat().st_mtime_ns
			os.utime(root/'tracked.txt', ns=(1, 1)); os.utime(root/'tracked.txt')  # stale stat data, same content
			before = stamp()
			git('status', '--porcelain', env=dict(os.environ, GIT_OPTIONAL_LOCKS='0'))
			self.assertEqual(stamp(), before)
			time.sleep(0.01); git('status', '--porcelain', env={k: v for k, v in os.environ.items() if k != 'GIT_OPTIONAL_LOCKS'})
			self.assertNotEqual(stamp(), before)  # the hazard: a default status rewrites it

	def test_the_final_and_its_stages_run_git_without_optional_locks(self):
		seen = []
		def execute(command, **kw):
			seen.append((os.environ.get('GIT_OPTIONAL_LOCKS'), kw['env'].get('GIT_OPTIONAL_LOCKS')))
			d = kw['directory']; d.mkdir(); child.atomic_json(d/'RESULT.json', dict(status='PASS'))
			return SimpleNamespace(returncode=0, validation={'path': str(d/'RESULT.json'), 'sha256': child.digest(d/'RESULT.json')})
		import stage_fakes
		with tempfile.TemporaryDirectory() as tmp, patch.dict(os.environ, {}, clear=False):
			os.environ.pop('GIT_OPTIONAL_LOCKS', None)
			root = Path(tmp)
			with patch.object(final, 'disk_check'), patch.object(final, 'tools_identity', return_value={}), \
					patch.object(final.child, 'head', return_value='c'), patch.object(final.gate, 'select', return_value=selection(['a'])), \
					patch.object(final.gate, 'check_selection'), patch.object(final.gate, 'finalize', return_value={'status': 'PASS'}), \
					patch.object(final.child, 'run', side_effect=execute), patch.object(final, 'compact', return_value={}), stage_fakes.inline(final):
				final.run(root, root/'final', names=['a'])
			self.assertEqual(seen, [('0', '0')])
			self.assertNotIn('GIT_OPTIONAL_LOCKS', os.environ)

	def test_a_stage_writing_tracked_source_fails_its_neighbor(self):
		with tempfile.TemporaryDirectory() as tmp:
			root = Path(tmp)
			git = lambda *a: subprocess.run(['git', *a], cwd=root, check=True, capture_output=True)
			git('init'); (root/'tracked.txt').write_text('original'); (root/'.gitignore').write_text('out/\n')
			git('add', '.'); git('-c', 'user.name=T', '-c', 'user.email=t@invalid', '-c', 'commit.gpgsign=false', 'commit', '-m', 'f')
			out = root/'out'; out.mkdir()
			def stage(name, code):
				# An ignored test script: a registered leaf child of this checkout.
				(out/('test_'+name+'.py')).write_text(code)
				def execute():
					with (out/(name+'.log')).open('x') as log:
						return child.run([sys.executable, str(out/('test_'+name+'.py'))], cwd=root, root=root, stdout=log,
							stderr=subprocess.STDOUT, directory=out/(name+'-child'), timeout=60)
				return execute
			neighbor = scheduler.Forked(stage('neighbor', 'import time; time.sleep(1.5)'))
			time.sleep(0.5)
			writer = scheduler.Forked(stage('writer', "open('tracked.txt','w').write('changed')"))
			writer.result()
			value = neighbor.result()
			self.assertEqual(value['returncode'], 1)
			self.assertIn('source changed during child', json.loads(Path(value['validation']['path']).read_text())['error'])


if __name__ == '__main__': unittest.main()
