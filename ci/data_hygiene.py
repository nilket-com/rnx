"""Explicit, reviewed data consumers; never infer deletion from text search."""
import ast
import hashlib
import json
from pathlib import Path
import re
import subprocess

DATA = {'.json', '.jsonl', '.gz', '.xz', '.tsv', '.csv', '.npy', '.npz', '.bin', '.der', '.pem', '.txt', '.log', '.stdout', '.stderr', '.out', '.elf', '.nm', '.exec', '.data', '.trace', '.pty', '.ansi', '.bundle', '.png', '.woff2', '.ttf', '.ipynb'}
SOURCE = {'.rs', '.py', '.sh', '.rn', '.js', '.ts', '.c', '.h', '.cpp', '.rb', '.pl', '.lua', '.jl', '.R', '.java', '.in', '.html', '.css', '.scss', '.svg', '.md', '.rst', '.toml', '.lock', '.yaml', '.yml'}


def is_data(path):
	p = Path(path)
	if p.name.startswith(('LICENSE', 'COPYING', 'NOTICE')) or '/licenses/' in path:
		return False
	return p.suffix in DATA or p.suffix not in SOURCE and p.name not in {'.gitignore', '.gitattributes', '.editorconfig', 'Makefile', 'Justfile'}


def check(files, manifest, baseline_sizes):
	failures = []
	entries = manifest['entries']; groups = {}
	for path, data in files.items():
		if not is_data(path): continue
		entry = entries.get(path)
		if entry:
			if not entry.get('reason', '').strip() or len(data) > entry.get('max_bytes', -1):
				failures.append((path, 'missing reason or exceeded reviewed size'))
			if entry.get('consumer'):
				reader = entry['consumer']
				# A reviewed loader expression, not an arbitrary mention of a path.
				text = files.get(reader['path'], b'').decode('utf8', 'replace')
				expression = reader['expression']
				live = Path(reader['path']).suffix in SOURCE - {'.md', '.rst', '.toml', '.lock', '.yaml', '.yml'}
				if Path(reader['path']).suffix == '.py':
					try: live = live and any(isinstance(n, ast.Call) and ast.get_source_segment(text, n) == expression for n in ast.walk(ast.parse(text)))
					except SyntaxError: live = False
				else:
					live = live and any(expression in line and not line.lstrip().startswith(('//', '#', '*')) for line in text.splitlines())
				if not live or expression not in text or not re.search(r'include_(?:str|bytes)!|read_to_string\(|read_bytes\(|read_text\(|open\(|glob\(', expression):
					failures.append((path, 'consumer loader missing'))
			elif entry.get('class') not in {'source-configuration', 'reviewed-retention'}:
				failures.append((path, 'live fixture needs concrete loader or reviewed retention'))
		if len(data) > 16 * 1024 and not entry:
			failures.append((path, 'data lacks consumer or reasoned retention'))
		if len(data) >= 1024 * 1024 and baseline_sizes.get(path, 0) < 1024 * 1024 and not (entry and entry.get('allow_large')):
			failures.append((path, 'new data reaches 1 MiB'))
		if len(data) >= 4096:
			groups.setdefault(hashlib.sha256(data).hexdigest(), []).append(path)
	for sha, paths in groups.items():
		if len(paths) < 2: continue
		exemption = manifest.get('duplicate_exemptions', {}).get(sha, {})
		if sorted(exemption.get('paths', [])) != sorted(paths) or not exemption.get('reason', '').strip():
			failures.append((paths, 'duplicate data reaches 4 KiB'))
	return failures


def scan(root, revision=None, indexed=False):
	root = Path(root)
	def git(*args): return subprocess.check_output(['git', '-C', str(root), *args])
	rev = revision or 'HEAD'
	paths = git('ls-tree', '-r', '--name-only', '-z', revision).split(b'\0') if revision else git('ls-files', '-z').split(b'\0')
	files = {}
	for raw in filter(None, paths):
		path = raw.decode('utf8', 'surrogateescape')
		if revision or indexed:
			files[path] = git('show', (revision or '') + ':' + path)
		elif (root/path).is_file(): files[path] = (root/path).read_bytes()
	manifest = json.loads(files['ci/data_consumers.json'])
	# A record is measured from the parent of its own most recent plan.
	plan = next((line.split('\t', 1)[0] for line in git('log', '--first-parent', '--format=%H%x09%s', rev).decode().splitlines() if re.match(r'[^\t]+\tplan: adr [0-9]{4} ', line)), None)
	if plan is None: raise ValueError('data hygiene requires a record plan baseline')
	baseline = {}
	for row in filter(None, git('ls-tree', '-r', '-l', '-z', plan+'^').decode().split('\0')):
		metadata, path = row.split('\t', 1)
		size = metadata.split()[-1]
		if size != '-': baseline[path] = int(size)
	return check(files, manifest, baseline)
