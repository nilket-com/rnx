"""Final-owned bulk roots and compact, explicitly post-release revalidation."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import time
import uuid

from child_result import atomic_json, digest, identity

CAP = 256 * 1024


def sync_directory(path):
	fd=os.open(path,os.O_RDONLY|os.O_DIRECTORY)
	try:os.fsync(fd)
	finally:os.close(fd)


def canonical(path):
	path = Path(path).absolute()
	if path.resolve() != path: raise ValueError('symlink or noncanonical owner path')
	return path


def assigned_root(variable, standalone):
	"""Standalone recipes keep their paths; a final must supply an owned root."""
	if not os.environ.get('RNX_FINAL'): return Path(standalone)
	value = os.environ.get(variable)
	if not value: raise ValueError('final output root not assigned: '+variable)
	p = canonical(value)
	if not p.is_dir(): raise ValueError('final owner must create root')
	return p


def tree(path, known=None):
	"""Hash link text, never its target; stream file contents, sorted by relative name."""
	started = time.monotonic(); rows = []; logical = allocated = reused = 0; known = known or {}
	for directory, dirs, files in os.walk(path, followlinks=False):
		for name in sorted(dirs + files):
			p = Path(directory)/name; s = p.lstat()
			kind = stat.S_IFMT(s.st_mode)
			if stat.S_ISLNK(s.st_mode): value = hashlib.sha256(os.fsencode(os.readlink(p))).hexdigest()
			elif stat.S_ISREG(s.st_mode):
				value = known.get(str(p))
				if value is None:value = digest(p)
				else:reused += 1
			elif stat.S_ISDIR(s.st_mode): value = None
			else: raise ValueError('unsupported payload file type: ' + str(p))
			rows.append([str(p.relative_to(path)), kind, s.st_size if value else 0, value])
			logical += s.st_size; allocated += s.st_blocks * 512
	return dict(sha256=identity(sorted(rows)), entries=len(rows), logical_bytes=logical,
		allocated_bytes=allocated, reused_file_digests=reused, seconds=time.monotonic()-started)


class Owners:
	def __init__(self, registry, out):
		self.registry = canonical(registry); self.registry.mkdir(parents=True, exist_ok=True)
		self.out = canonical(out); self.out.mkdir(parents=True, exist_ok=True)
		self.id = uuid.uuid4().hex
		self.path = self.registry/(self.id+'.owner.json')
		self.groups = self.registry/(self.id+'.groups')
		self.groups.mkdir()
		self.lock = self.path.with_suffix('.lock').open('x')
		fcntl.flock(self.lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
		self.row = dict(schema=1, invocation=self.id, out=str(self.out), groups=str(self.groups), roots=[], status='active')
		atomic_json(self.path, self.row)

	def assign(self, parent, label, *, name=None):
		parent = canonical(parent); parent.mkdir(parents=True, exist_ok=True)
		name = name or (self.id+'-'+label)
		if Path(name).name != name or name in {'', '.', '..'}: raise ValueError('outside owner name')
		p = parent/name
		if p.exists() or p.is_symlink(): raise ValueError('preexisting evidence root')
		# Parent identity is durable even if creation is interrupted.
		ps = parent.stat()
		entry = dict(path=str(p), parent=str(parent), stage=label, state='creating',
			parent_device=ps.st_dev, parent_inode=ps.st_ino)
		self.row['roots'].append(entry); atomic_json(self.path, self.row)
		p.mkdir(); sync_directory(parent); s = p.stat(); ps = parent.stat()
		entry.update(state='owned', device=s.st_dev, inode=s.st_ino,
			parent_device=ps.st_dev, parent_inode=ps.st_ino)
		atomic_json(self.path, self.row)
		return p

	def release(self):
		result = release(self.path)
		self.row = json.loads(self.path.read_text())
		return result

	def retire(self): retire(self.path)

	def close(self): self.lock.close()


def release(ledger):
	ledger = Path(ledger); row = json.loads(ledger.read_text()); results = []
	if row.get('schema') != 1: raise ValueError('unknown ownership schema')
	for entry in row['roots']:
		p = Path(entry['path']); parent = canonical(entry['parent'])
		if p.parent != parent or p.name in {'', '.', '..'}: raise ValueError('outside owned parent')
		ps = parent.stat()
		if (ps.st_dev, ps.st_ino) != (entry.get('parent_device'), entry.get('parent_inode')):
			raise ValueError('parent identity mismatch')
		if entry['state'] == 'released':
			if p.exists() or p.is_symlink(): raise ValueError('released root reappeared')
			results.append(entry); continue
		if entry['state'] == 'creating':
			if p.exists() or p.is_symlink():
				canonical(p)
				if not stat.S_ISDIR(p.lstat().st_mode): raise ValueError('incomplete root is not directory: '+str(p))
				# Never adopt or recursively remove a root without a recorded inode.
				try: p.rmdir()
				except OSError as error: raise ValueError('incomplete nonempty root refused: '+str(p)) from error
				sync_directory(parent)
			entry.update(state='released', creation='resolved without adoption')
			atomic_json(ledger,row); results.append(entry); continue
		if entry['state'] not in {'owned', 'releasing'}: raise ValueError('invalid root state: '+str(p))
		if p.exists() or p.is_symlink():
			canonical(p); s = p.lstat()
			if not stat.S_ISDIR(s.st_mode) or (s.st_dev,s.st_ino) != (entry['device'],entry['inode']):
				raise ValueError('root identity mismatch')
			if entry['state'] == 'owned':
				entry['state'] = 'releasing'; atomic_json(ledger,row)
			shutil.rmtree(p)  # fd-based implementation; interior symlinks are unlinked.
			sync_directory(parent)
		elif entry['state'] != 'releasing': raise ValueError('owned root disappeared before release')
		entry['state']='released'; atomic_json(ledger,row); results.append(entry)
	row['status']='released'; atomic_json(ledger,row)
	return dict(status='PASS', ledger_sha256=digest(ledger), roots=results)


def group_directory(ledger, row):
	path = canonical(row['groups'])
	expected = ledger.parent/(row['invocation']+'.groups')
	if path != expected or not path.is_dir(): raise ValueError('missing or invalid producer registry: '+str(path))
	return path


def retire(ledger):
	ledger = Path(ledger)
	row = json.loads(ledger.read_text())
	if row.get('status') not in {'released','retiring'}: raise ValueError('cannot retire unreleased owner')
	groups = canonical(row['groups'])
	if groups != ledger.parent/(row['invocation']+'.groups'): raise ValueError('invalid group registry')
	row['status']='retiring'; atomic_json(ledger,row)
	# Caller already made the release record durable. Recovery no longer needs groups.
	if groups.exists(): shutil.rmtree(groups)
	ledger.with_suffix('.lock').unlink(missing_ok=True)
	ledger.unlink()
	sync_directory(ledger.parent)


def recover(registry):
	"""Recovery needs only the registry, never a deleted final directory."""
	registry = canonical(registry)
	registry.mkdir(parents=True,exist_ok=True)
	rows = []
	for ledger in sorted(registry.glob('*.owner.json')):
		canonical(ledger)
		initial=json.loads(ledger.read_text())
		with ledger.with_suffix('.lock').open('a+' if initial.get('status')=='retiring' else 'r+') as lock:
			try: fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
			except BlockingIOError: raise ValueError('live final owner: '+str(ledger))
			row = json.loads(ledger.read_text())
			if row.get('status')=='retiring':
				retire(ledger); continue
			from owned_build import drain_recorded_groups
			groups = group_directory(ledger, row)
			drain_recorded_groups(groups)
			result = release(ledger)
			result.update(interrupted=True, producer_groups=[json.loads(p.read_text()) for p in sorted(groups.glob('GROUP-*.json'))])
			receipt = registry/(row['invocation']+'-RECOVERY.json')
			atomic_json(receipt,result)
			retire(ledger)
			rows.append(str(receipt))
	return rows


def compact(selection, evidence, owners, tools):
	stages=[]; known={}
	for name, command in selection['stages']:
		ref=evidence.get(name)
		if not ref: continue
		r=json.loads(Path(ref['path']).read_text())
		if r['status']=='PASS':known.update(r['artifacts'])
		stages.append(dict(name=name, command_template=command, status=r['status'], exit=r['process_exit'],
			seconds=r['seconds'], cleanup_seconds=r['cleanup_seconds'], environment_names=sorted(r['context']['environment']['keys']),
			receipt_sha256=ref['sha256'], artifact_set_sha256=identity(r['artifacts'])))
	roots=[dict(owner=e, payload=tree(e['path'],known)) for e in owners.row['roots']]
	m=dict(schema=1,candidate=selection['record']['candidate'],source=selection['source'],policy=selection['policy'],
		acceptance_scope=selection.get('scope'),selection_sha256=identity(selection),tools=tools,stages=stages,roots=roots)
	if len(json.dumps(m,indent=2).encode())>CAP: raise ValueError('compact manifest exceeds 256 KiB')
	return m


def revalidate(out):
	out=Path(out); result=json.loads((out/'RESULT.json').read_text())
	for name in ('ACCEPTANCE','MANIFEST','RELEASE'):
		if digest(out/(name+'.json')) != result['records'][name]: raise ValueError('terminal record changed: '+name)
	a=json.loads((out/'ACCEPTANCE.json').read_text()); m=json.loads((out/'MANIFEST.json').read_text()); r=json.loads((out/'RELEASE.json').read_text())
	if result['status']!='PASS' or a['status']!='PASS' or r['status']!='PASS': raise ValueError('final not complete')
	if result['candidate']!=m['candidate'] or a['candidate']!=m['candidate']: raise ValueError('candidate differs')
	if r['acceptance_sha256']!=result['records']['ACCEPTANCE'] or r['manifest_sha256']!=result['records']['MANIFEST']: raise ValueError('release not bound')
	if [x['path'] for x in r['roots']] != [x['owner']['path'] for x in m['roots']] or any(x['state']!='released' for x in r['roots']): raise ValueError('incomplete release')
	if 'publication_eligible' in result:
		from gate_policy import publication_eligible
		if not a['acceptance_scope']==m['acceptance_scope']==result['acceptance_scope']: raise ValueError('scope differs')
		if result['publication_eligible']!=publication_eligible(result['acceptance_scope']) or result['complete_final']!=(result['acceptance_scope']=='complete'): raise ValueError('scope eligibility differs')
	return dict(status='PASS',candidate=m['candidate'],bulk='released; not rehashed',release=result['records']['RELEASE'],
		acceptance_scope=result['acceptance_scope'],complete_final=result['complete_final'],publication_eligible=result.get('publication_eligible',result['complete_final']))


def abandon_diagnostics(out):
	out=Path(out);r=json.loads((out/'RESULT.json').read_text())
	if r.get('status')!='FAIL':raise ValueError('only failed final diagnostics can be abandoned')
	p=out/'DIAGNOSTICS.json'
	if p.is_symlink():raise ValueError('diagnostic symlink')
	sha=digest(p)
	atomic_json(out/'DIAGNOSTICS-RELEASE-INTENT.json',dict(sha256=sha,reason='explicit abandon'))
	p.unlink();sync_directory(out)
	atomic_json(out/'DIAGNOSTICS-RELEASE.json',dict(status='PASS',sha256=sha,reason='explicit abandon'))
