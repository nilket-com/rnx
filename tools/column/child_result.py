"""Bound child-controller results. Exit zero alone never validates a producer."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import time
import uuid

ROOT = Path(__file__).resolve().parents[2]
_spec = importlib.util.spec_from_file_location('gate_durable_receipts', ROOT / 'rune/tools/xtask/supplemental/receipts.py')
_durable = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(_durable)
atomic_json = _durable.atomic_json
digest = _durable.digest
identity = _durable.identity
CONTEXT = 'RNX_VALIDATION_CONTEXT'
ENV_KEYS = frozenset({'RUSTFLAGS', 'RUSTDOCFLAGS', 'RUSTUP_TOOLCHAIN', 'RUSTC',
	'RUSTDOC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'MIRIFLAGS',
	'GLIBC_TUNABLES', 'MALLOC_CONF', 'PYTHONHASHSEED', 'PYTHONDONTWRITEBYTECODE',
	'CC', 'CXX', 'CFLAGS', 'CXXFLAGS', 'LDFLAGS', 'LANG', 'LC_ALL', 'TZ'})
ENV_PREFIXES = ('CARGO_', 'RNX_')
PRODUCERS = frozenset(('tools/column/' + n + '.py') for n in (
	'accept', 'final', 'verify', 'clippy', 'inventories', 'cli', 'resume', 'recover_cross', 'chain', 'borrowed_controls', 'ir_pool', 'old_path', 'boolean_capture', 'f64_plan_capture',
	'f64_plan_large', 'f64_output_capture', 'accept_remote', 'cross_program_controls', 'cross_program_gate', 'fixed_route_gate', 'suite_stage', 'miri_exact', 'repl_session_controls', 'jupyter_controls')) | {'tools/verify.py',
	'rune/tools/xtask/column/ck1.py', 'rune/tools/xtask/column/measure.py',
	'rune/tools/xtask/column/cli_capture.py', 'rune/tools/xtask/supplemental/run.py'}
LEAVES = {'ci/preflight.py', 'ci/disk_space.py', 'ci/hygiene.py', 'ci/commit_subjects.py',
	'tools/test_verify_pins.py', 'rune/tools/xtask/column/records.py', 'rune/tools/xtask/column/dependencies.py'}


def source(root):
	root = Path(root).resolve()
	paths = subprocess.check_output(['git', '-C', str(root), 'ls-files', '-z', '--cached', '--others', '--exclude-standard']).split(b'\0')
	rows = {}
	for raw in filter(None, paths):
		name = os.fsdecode(raw); p = root / name
		if p.is_symlink():
			rows[name] = identity(['symlink', os.readlink(p)])
		elif p.is_file():
			rows[name] = [digest(p), bool(p.stat().st_mode & 0o111)]
		else:
			rows[name] = 'deleted'
	return identity(rows)


def head(root):
	try:
		return subprocess.check_output(["git", "-C", str(root), "rev-parse", "--verify", "HEAD"], stderr=subprocess.DEVNULL).decode().strip()
	except subprocess.CalledProcessError:
		return None  # Uncommitted temporary repositories support development controls only.


def environment(env):
	# Bind declared recipe inputs only. Never persist values, including secrets.
	declared = {k: v for k, v in env.items() if k != CONTEXT and
		(k in ENV_KEYS or k.startswith(ENV_PREFIXES))}
	return {'keys': sorted(declared), 'sha256': identity(declared)}


def script(command, cwd):
	if Path(command[0]).name.startswith('python'):
		for part in command[1:]:
			if part == '-m':
				return None
			if not part.startswith('-'):
				return (Path(cwd) / part).resolve()
	return None


def contract(command, cwd, root=ROOT):
	if '-m' in command and Path(command[0]).name.startswith('python'):
		if command[command.index('-m') + 1] != 'unittest': raise ValueError('unregistered Python module child')
	path = script(command, cwd)
	if path is not None:
		try: name = path.relative_to(Path(root).resolve()).as_posix()
		except ValueError: raise ValueError('unregistered external Python child')
		if name in PRODUCERS: return 'producer'
		if name in LEAVES or path.name.startswith('test_'): return 'leaf'
		raise ValueError('unregistered Python child: ' + name)
	if Path(command[0]).name in {'cargo', 'rustc', 'git', 'ssh', 'rustup', 'python3', 'python'}:
		return 'leaf'
	raise ValueError('unregistered raw child: ' + command[0])


def publish(path, value):
	"""Producer writes its own semantic result before its bound terminal envelope."""
	path = Path(path)
	if os.environ.get(CONTEXT):
		context_path = Path(os.environ[CONTEXT]); ctx = json.loads(context_path.read_text())
		resolved = str(path.resolve())
		if resolved in ctx['existing_results']:
			if not ctx['resume'] or json.loads(path.read_text()).get('status') != 'PASS':
				raise ValueError('cannot overwrite a completed failure or implicitly resume')
			if digest(path) != ctx['existing_results'][resolved]: raise ValueError('prior result changed')
			archive = context_path.with_name('PRIOR-' + digest(path) + '.json')
			with archive.open('xb') as f:
				f.write(path.read_bytes()); f.flush(); os.fsync(f.fileno())
			_durable.sync_directory(archive.parent)
		value = dict(value, validation_context=identity(ctx))
	atomic_json(path, value)
	bind(path)


def bind(path):
	"""Bind an existing semantic receipt (e.g. the Rust cross controller's result)."""
	if not os.environ.get(CONTEXT): return
	context_path = Path(os.environ[CONTEXT])
	ctx = json.loads(context_path.read_text())
	terminal = context_path.with_name('CHILD.json')
	if terminal.exists(): raise ValueError('child terminal receipt already exists')
	path = Path(path).resolve(strict=True)
	value = json.loads(path.read_text())
	if not isinstance(value, dict) or value.get('status') not in {'PASS', 'FAIL'}:
		raise ValueError('invalid producer status')
	if head(ctx['root']) != ctx['candidate'] or source(ctx['root']) != ctx['source'] or environment(os.environ) != ctx['environment']:
		raise ValueError('producer source/environment changed')
	# fsync the actual producer receipt before committing its digest envelope.
	with path.open('rb') as f: os.fsync(f.fileno())
	_durable.sync_directory(path.parent)
	atomic_json(terminal, {'schema': 1, 'context': identity(ctx), 'invocation': ctx['invocation'],
		'source': ctx['source'], 'status': value['status'], 'receipt': str(path), 'sha256': digest(path)})


def validate(directory, expected, exit_code):
	directory = Path(directory)
	if exit_code != 0: raise ValueError('child exit is nonzero')
	if json.loads((directory / 'CONTEXT.json').read_text()) != expected:
		raise ValueError('invocation context changed')
	row = json.loads((directory / 'CHILD.json').read_text())
	if not isinstance(row, dict) or row.get('schema') != 1 or row.get('status') != 'PASS':
		raise ValueError('child receipt is not PASS')
	for key, value in [('context', identity(expected)), ('invocation', expected['invocation']), ('source', expected['source'])]:
		if row.get(key) != value: raise ValueError('child binding differs: ' + key)
	path = Path(row['receipt'])
	if str(path.parent) != expected['result_directory'] :
		raise ValueError('producer result is outside this fresh invocation')
	value = json.loads(path.read_text())
	if str(path) in expected['existing_results']:
		if not expected['resume'] or value.get('validation_context') != identity(expected):
			raise ValueError('stale or unbound resumed producer result')
		prior = directory / ('PRIOR-' + expected['existing_results'][str(path)] + '.json')
		if digest(prior) != expected['existing_results'][str(path)] or json.loads(prior.read_text()).get('status') != 'PASS': raise ValueError('cannot reuse failed result')
	if digest(path) != row['sha256'] or value.get('status') != 'PASS':
		raise ValueError('producer result missing, changed or failed')
	return row


def run(command, *, cwd, env=None, stdout=None, stderr=None, timeout=None, directory=None, root=ROOT, after=None):
	"""Run once and persist parent validation; callers retain their semantic checks."""
	from owned_build import run_process
	started = time.monotonic()
	command = [str(x) for x in command]; cwd = Path(cwd).resolve(); root = Path(root).resolve()
	if directory is None:
		if not stdout or not isinstance(getattr(stdout, 'name', None), str):
			raise ValueError('explicit attempt directory or named stdout required')
		directory = Path(stdout.name + '.validation')
	directory = Path(directory); directory.mkdir(parents=True, exist_ok=False)
	row = {'schema': 1, 'status': 'FAIL', 'command': command, 'process_exit': None}
	child = subprocess.CompletedProcess(command, 1)
	try:
		kind = contract(command, cwd, root)
		clean_env = dict(os.environ if env is None else env); clean_env.pop(CONTEXT, None)
		ctx = {'schema': 1, 'invocation': uuid.uuid4().hex, 'root': str(root), 'cwd': str(cwd),
			'command': command, 'candidate': head(root), 'source': source(root), 'environment': environment(clean_env), 'kind': kind}
		if kind == 'producer':
			if command.count('--out') != 1: raise ValueError('producer needs one explicit output directory')
			result_dir = (cwd / command[command.index('--out') + 1]).resolve()
			ctx['result_directory'] = str(result_dir)
			ctx['existing_results'] = {str(p.resolve()): digest(p) for p in result_dir.glob('*.json')}
			ctx['resume'] = '--resume' in command
		atomic_json(directory / 'CONTEXT.json', ctx)
		row['context'] = ctx
		if kind == 'producer': clean_env[CONTEXT] = str((directory / 'CONTEXT.json').resolve())
		test_command = Path(command[0]).name == 'cargo' and 'test' in command and '--list' not in command
		if test_command:
			from suite_stage import discovered
			discovery = command[:command.index('--')] if '--' in command else command[:]
			# Keep filters and ignored policy, remove execution-thread settings.
			tail = command[command.index('--')+1:] if '--' in command else []
			discovery += ['--'] + [v for v in tail if not v.startswith('--test-threads')] + ['--list']
			with (directory/'discovery.log').open('x') as log:
				done = run_process(discovery, termination_grace=30, cwd=cwd, env=clean_env, stdout=log, stderr=subprocess.STDOUT, timeout=timeout)
			if done.returncode: raise ValueError('test discovery failed')
			row['discovery'] = discovered((directory/'discovery.log').read_text())
		child = run_process(command, termination_grace=30, cwd=cwd, env=clean_env, stdout=stdout, stderr=stderr, timeout=timeout)
		row['process_exit'] = child.returncode
		if after is not None: row['declared_output'] = after()
		if child.returncode != 0: raise ValueError('child exit is nonzero')
		if test_command:
			from suite_stage import executed
			if stdout is None or not isinstance(getattr(stdout, 'name', None), str): raise ValueError('test execution requires a named log')
			stdout.flush()
			row['test_count'] = executed(Path(stdout.name).read_text())

		if head(root) != ctx['candidate'] or source(root) != ctx['source']: raise ValueError('source changed during child')
		if kind == 'producer': row['producer'] = validate(directory, ctx, child.returncode)
		row['status'] = 'PASS'
	except Exception as error:
		row['error'] = repr(error)
		row['error_type'] = type(error).__name__
		child = subprocess.CompletedProcess(command, child.returncode or 1)
	finally:
		artifacts = {}
		for stream in (stdout, stderr):
			if stream is not None and hasattr(stream, 'flush'):
				stream.flush(); os.fsync(stream.fileno())
				artifacts[str(Path(stream.name).resolve())] = digest(stream.name)
		if row.get('declared_output'):
			archive = Path(row['declared_output']['archive'])
			with archive.open('rb') as f: os.fsync(f.fileno())
			artifacts[str(archive.resolve())] = digest(archive)
		if not artifacts and row['status'] == 'PASS':
			row.update(status='FAIL', error='stage has no log evidence')
			child = subprocess.CompletedProcess(command, 1)
		if (directory/'discovery.log').exists(): artifacts[str((directory/'discovery.log').resolve())] = digest(directory/'discovery.log')
		cleanup = []
		for p in sorted(directory.glob('CLEANUP-*.json')):
			event = json.loads(p.read_text())
			if digest(event['path']) != event['sha256']: raise ValueError('cleanup evidence changed')
			cleanup.append(event); artifacts[str(p.resolve())] = digest(p)
			artifacts[event['path']] = event['sha256']
			if event['status'] != 'PASS':
				row.update(status='FAIL', cleanup_error='owned build removal failed')
				child = subprocess.CompletedProcess(command, child.returncode or 1)
		row.update(seconds=time.monotonic()-started, cleanup_seconds=sum(e['seconds'] for e in cleanup), cleanup=cleanup)
		row['artifacts'] = artifacts
		atomic_json(directory / 'RESULT.json', row)
	child.declared_output = row.get('declared_output')
	child.validation = {'path': str((directory / 'RESULT.json').resolve()), 'sha256': digest(directory / 'RESULT.json')}
	return child


def evidence(reference, expected_source):
	path = Path(reference['path'])
	if digest(path) != reference['sha256']: raise ValueError('stage evidence changed')
	row = json.loads(path.read_text())
	if row.get('status') != 'PASS' or row.get('process_exit') != 0 or row['context']['source'] != expected_source:
		raise ValueError('stage did not pass for expected source')
	if row['context']['kind'] == 'producer': validate(path.parent, row['context'], row['process_exit'])
	for name, sha in row['artifacts'].items():
		if digest(name) != sha: raise ValueError('stage artifact changed')
	return row


def validate_remote(exit_code, result, expected):
	"""Remote transport binds the fresh invocation and bootstrap recipe explicitly."""
	if exit_code != 0 or result.get('status') != 'PASS':
		raise ValueError('remote child failed')
	if result.get('publication_eligible', result.get('complete_final', True)) is False: raise ValueError('remote selected stages are not publication eligible')
	if result.get('remote_context') != expected or result.get('candidate') != expected['candidate']:
		raise ValueError('remote invocation, source or recipe differs')
	final = result.get('finalization', {})
	if final.get('status') != 'PASS' or final.get('candidate') != expected['candidate'] or final.get('source') != expected['source']:
		raise ValueError('remote finalization missing or stale')
	return {'status': 'PASS', 'context': identity(expected), 'receipt': identity(result)}
