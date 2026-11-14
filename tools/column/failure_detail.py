"""Bounded diagnostic copies of a failure, never substitute acceptance evidence."""
import json
import os
from pathlib import Path
import re

from child_result import atomic_json, digest
from final_evidence import sync_directory

FILE_CAP = 64*1024
ERROR_CAP = 16*1024
BUNDLE_CAP = 256*1024
FINAL_CAP = 1024*1024
INDEX_RESERVE = 32*1024
NON_SECRET_ENV = frozenset({'HOME', 'PWD', 'OLDPWD', 'USER', 'LOGNAME', 'PATH', 'SHELL',
	'LANG', 'LANGUAGE', 'TERM', 'COLORTERM', 'CARGO_HOME', 'RUSTUP_HOME', 'SHLVL',
	'TMPDIR', 'TMP', 'TEMP', 'TZ', 'XDG_RUNTIME_DIR', 'XDG_CACHE_HOME', 'XDG_CONFIG_HOME',
	'XDG_DATA_HOME', 'XDG_STATE_HOME'})
INVOCATION_DUMP = re.compile(r'''(?ix)^\s*(?:[\w.]+:\s*)?
	(?: (?:argv|command|execve|environment|environ)\s*[:=(\[]
	  | command\s+['"]?\[ ) | ^\s*\[['"]''')
CREDENTIAL_FLAG = re.compile(r'''(?ix)
	(?P<flag>--(?:(?:\w+[-_])*(?:token|key|secret|password|passwd|auth|authorization|credentials?)(?:[-_]\w+)*
	|api[-_]?key|access[-_]?token|client[-_]?secret))
	(?P<separator>=|[\t ]+)
	(?P<value>"(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'|[^\s,;]+)''')


def sanitize(text, environment):
	# Keep diagnostic numbers, locations and option prose. Only recognizable
	# invocation dumps are dropped; credential options lose just their value.
	lines = []
	for line in text.splitlines(keepends=True):
		if INVOCATION_DUMP.search(line):
			lines.append('<redacted invocation detail>\n')
		else:
			lines.append(CREDENTIAL_FLAG.sub(lambda match: match['flag']+match['separator']+'<redacted>', line))
	text = ''.join(lines)
	paths = set(str(environment.get('PATH', '')).split(os.pathsep))
	secrets = {str(v) for k, v in environment.items() if v and k not in NON_SECRET_ENV and
		not k.startswith('LC_') and len(str(v)) >= 8 and not str(v).isnumeric() and str(v) not in paths}
	for secret in sorted(secrets, key=len, reverse=True): text = text.replace(secret, '<redacted>')
	return text


def bounded(text, cap):
	data = text.encode('utf-8')
	return data[:cap].decode('utf-8', 'ignore'), len(data) > cap


def checked(path, roots):
	path = Path(path).absolute()
	if path.resolve() != path or not any(path.is_relative_to(Path(root).absolute()) for root in roots):
		raise ValueError('failure artifact is linked or outside invocation')
	if not path.is_file(): raise FileNotFoundError('failure artifact missing: ' + path.name)
	return path


def value(path, roots):
	path = checked(path, roots)
	if path.stat().st_size > 8*1024*1024: raise ValueError('failure receipt exceeds read bound')
	row = json.loads(path.read_text())
	if not isinstance(row, dict): raise ValueError('failure receipt is not an object')
	return row


def log_excerpt(path, cap=FILE_CAP):
	# For a large log discard partial boundary lines, so a split secret is not
	# retained as a prefix/suffix that would evade whole-value redaction.
	size = path.stat().st_size
	with path.open('rb') as stream:
		if size <= cap: return stream.read(), False
		marker = b'\n<truncated>\n'
		take = (cap-len(marker))//2
		head = stream.read(take)
		stream.seek(size-take)
		tail = stream.read()
	head = head.rpartition(b'\n')[0] if b'\n' in head else b''
	tail = tail.partition(b'\n')[2] if b'\n' in tail else b''
	return head+marker+tail, True


def reason(row):
	if row.get('error'): return str(row['error'])
	# Retain failed rule identities/booleans, never the measurement vectors.
	failed = []
	for section in ('per_record', 'cumulative'):
		for check in row.get(section, []):
			if check.get('pass_all') is False:
				failed.append(section+': '+', '.join(f'{key}={check[key]}' for key in
					('kind', 'mode', 'metric', 'level_pass', 'growth_pass') if key in check))
	return '\n'.join(failed[:16]) or 'producer returned FAIL without an error message'


def projection(row, environment):
	result = {key: row[key] for key in ('status', 'kind', 'policy', 'n', 'role', 'error_type')
		if key in row and isinstance(row[key], (str, int, bool, type(None)))}
	if row.get('status') == 'FAIL' or 'error' in row:
		result['error'], result['error_truncated'] = bounded(sanitize(reason(row), environment), ERROR_CAP)
	# Only the numeric admission fields and binding label are diagnostic data.
	if isinstance(row.get('admission'), dict):
		result['admission'] = {k: v for k, v in row['admission'].items() if
			k in {'host_available_bytes', 'cgroup_headroom_bytes', 'binding_cgroup', 'binding_limit', 'reserve_bytes', 'per_worker_bytes', 'workers'}
			and isinstance(v, (int, str))}
	for key, item in result.items():
		if isinstance(item, str): result[key] = sanitize(item, environment)
		elif isinstance(item, dict): result[key] = {k: sanitize(v, environment) if isinstance(v, str) else v for k, v in item.items()}
	return result


class Bundle:
	def __init__(self, out, stage, environment, error):
		self.out = Path(out)
		self.directory = self.out/'failure'
		self.directory.mkdir(exist_ok=False)
		self.environment = environment
		message, cut = bounded(sanitize(str(error), environment), ERROR_CAP)
		self.row = dict(schema=1, diagnostic_only=True, stage=bounded(sanitize(stage, environment), 256)[0], files=[], omitted=[],
			error=message, error_truncated=cut)
		self.remaining = BUNDLE_CAP-INDEX_RESERVE

	def add(self, path, kind, roots):
		if len(self.row['files']) >= 8 or self.remaining < 1024:
			self.row['omitted'].append('budget exhausted'); return
		path = checked(path, roots)
		if kind == 'receipt':
			data = (json.dumps(projection(value(path, roots), self.environment), sort_keys=True, indent=2)+'\n').encode()
			cut = False
		else:
			raw, cut = log_excerpt(path)
			data = sanitize(raw.decode('utf-8', 'replace'), self.environment).encode()
		cap = min(FILE_CAP, self.remaining)
		if kind == 'receipt' and len(data) > cap:
			data = (json.dumps(dict(status='FAIL', error='receipt projection omitted: diagnostic size bound', truncated=True))+'\n').encode()
			cut = True
		else:
			cut |= len(data) > cap
			data = data[:cap].decode('utf-8', 'ignore').encode()
		name = f'{len(self.row["files"]):02d}-' + ('receipt.json' if kind == 'receipt' else 'child.log')
		dest = self.directory/name
		with dest.open('xb') as stream:
			stream.write(data); stream.flush(); os.fsync(stream.fileno())
		self.remaining -= len(data)
		self.row['files'].append(dict(path='failure/'+name, kind=kind, bytes=len(data), sha256=digest(dest),
			source_name=bounded(sanitize(path.name, self.environment), 256)[0],
			source_bytes=path.stat().st_size, source_sha256=digest(path), truncated=cut, projected=kind=='receipt'))

	def finish(self):
		self.row['omitted'] = self.row['omitted'][:16]
		encoded = json.dumps(self.row, sort_keys=True, indent=2).encode()+b'\n'
		if len(encoded) > INDEX_RESERVE: raise ValueError('failure index exceeds bound')
		sync_directory(self.directory)
		atomic_json(self.out/'FAILURE.json', self.row)
		if len(encoded) + sum(f['bytes'] for f in self.row['files']) > BUNDLE_CAP: raise ValueError('failure bundle exceeds bound')
		return dict(path=str(self.out/'FAILURE.json'), sha256=digest(self.out/'FAILURE.json'),
			stage=self.row['stage'], error=self.row['error'])


def retain(out, stage, *, receipt=None, logs=(), roots, environment=None, error='stage failed'):
	environment = dict(os.environ) if environment is None else environment
	bundle = Bundle(out, stage, environment, error)
	try:
		if receipt is not None:
			parent = value(receipt, roots)
			leaf = Path(receipt)
			# Only the capture's explicit case receipt list is followed, never a tree.
			for name in parent.get('records', [])[:512]:
				if not isinstance(name, str): continue
				row = value(name, roots)
				if row.get('status') == 'FAIL':
					leaf = Path(name); break
			row = value(leaf, roots)
			if row.get('status') == 'FAIL' or row.get('error'):
				bundle.row['error'], bundle.row['error_truncated'] = bounded(sanitize(reason(row), environment), ERROR_CAP)
			bundle.add(leaf, 'receipt', roots)
			for name in ('child.log', 'direct.stderr'):
				p = leaf.parent/name
				if p.is_file(): bundle.add(p, 'log', roots)
			if leaf != Path(receipt): bundle.add(receipt, 'receipt', roots)
	except Exception as problem:
		bundle.row['omitted'].append(bounded(sanitize(str(problem), environment), 1024)[0])
	for p in logs:
		try:
			if Path(p).is_file():
				bundle.add(p, 'log', roots)
				if receipt is None or not Path(receipt).is_file() or 'child exit is nonzero' in bundle.row['error']:
					raw, _ = log_excerpt(checked(p, roots))
					tail = sanitize(raw.decode('utf-8', 'replace'), environment)[-8192:]
					bundle.row['error'], cut = bounded(bundle.row['error']+'\nchild log:\n'+tail, ERROR_CAP)
					bundle.row['error_truncated'] |= cut
		except Exception as problem:
			bundle.row['omitted'].append(bounded(sanitize(str(problem), environment), 1024)[0])
	return bundle.finish()


def copy_bundle(out, reference, *, roots, environment=None):
	"""Copy only verified diagnostic files, bounded again at each release owner."""
	environment = dict(os.environ) if environment is None else environment
	path = checked(reference['path'], roots)
	if digest(path) != reference['sha256']: raise ValueError('failure index changed')
	row = value(path, roots)
	if row.get('schema') != 1 or row.get('diagnostic_only') is not True or len(row['files']) > 8 or path.stat().st_size > INDEX_RESERVE:
		raise ValueError('not a bounded diagnostic bundle')
	bundle = Bundle(out, row['stage'], environment, row['error'])
	for item in row['files']:
		p = checked(path.parent/item['path'], [path.parent])
		if p.stat().st_size != item['bytes'] or item['bytes'] > FILE_CAP or digest(p) != item['sha256']:
			raise ValueError('failure artifact changed or exceeds bound')
		count = len(bundle.row['files'])
		bundle.add(p, item['kind'], [path.parent])
		if len(bundle.row['files']) == count: break
		bundle.row['files'][-1].update(source_name=item['source_name'], source_bytes=item['source_bytes'],
			source_sha256=item['source_sha256'], truncated=item['truncated'] or bundle.row['files'][-1]['truncated'], projected=item['projected'])
	bundle.row['omitted'] += row.get('omitted', [])
	bundle.row['error_truncated'] |= row.get('error_truncated', False)
	return bundle.finish()


class StageFailure(RuntimeError):
	def __init__(self, stage, failure):
		self.failure = failure
		super().__init__(stage + ' failed: STOP; ' + failure['error'])
