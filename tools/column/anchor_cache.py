"""One owned anchor bundle; the consumer holds the lock through scoring."""
import copy
import fcntl
import json
import os
from pathlib import Path
import shutil
import stat
import uuid

import child_result as child
from final_evidence import canonical, sync_directory


class Cache:
	def __init__(self, root):
		self.root = canonical(root)
		self.root.mkdir(parents=True,exist_ok=True)
		self.journal = self.root/'OWNERS.json'
		self.lock = None

	def save(self): child.atomic_json(self.journal,self.state)

	def __enter__(self):
		canonical(self.root/'LOCK')
		self.lock = (self.root/'LOCK').open('a+')
		fcntl.flock(self.lock,fcntl.LOCK_EX)
		try:
			canonical(self.journal)
			self.state = json.loads(self.journal.read_text()) if self.journal.exists() else dict(schema=1,current=None,entries=[])
			if self.state.get('schema')!=1: raise ValueError('unknown cache ownership schema')
			for row in list(self.state['entries']):
				if row['id']!=self.state['current'] or row['state']!='complete': self.remove(row)
			return self
		except BaseException:
			self.lock.close(); raise

	def __exit__(self,*exc): self.lock.close()

	def path(self,row):
		p=canonical(row['path'])
		if p.parent!=self.root: raise ValueError('cache entry outside owner')
		ps=self.root.stat()
		if (ps.st_dev,ps.st_ino)!=(row['parent_device'],row['parent_inode']): raise ValueError('cache parent replaced')
		return p

	def check(self,row,p):
		canonical(p); s=p.lstat()
		if not stat.S_ISDIR(s.st_mode) or (s.st_dev,s.st_ino)!=(row['device'],row['inode']): raise ValueError('cache root replaced')

	def remove(self,row):
		p=self.path(row)
		if row['state']=='publishing':
			old=canonical(row['rename_from'])
			if old.parent!=self.root: raise ValueError('outside pending root')
			if old.exists() and p.exists(): raise ValueError('ambiguous cache publication')
			if old.exists(): p=old
		if row['state']=='creating':
			if p.exists(): p.rmdir()  # No recorded inode: empty-only, never recursive.
		else:
			if p.exists() or p.is_symlink():
				self.check(row,p)
				row.update(path=str(p),state='releasing');self.save()
				shutil.rmtree(p)
			elif row['state']!='releasing': raise ValueError('owned cache root disappeared')
		sync_directory(self.root)
		self.state['entries'].remove(row)
		if self.state['current']==row['id']:self.state['current']=None
		self.save()

	def lookup(self,key):
		row=next((r for r in self.state['entries'] if r['id']==self.state['current']),None)
		if row is None:return None,'absent'
		p=self.path(row)
		if not p.exists() and not p.is_symlink():
			self.state['entries'].remove(row);self.state['current']=None;self.save()
			return None,'verification failed'
		self.check(row,p)
		if row['key']!=key:return None,'key differs'
		try:
			if child.digest(p/'ANCHOR.json')!=row['receipt_sha256']:raise ValueError('anchor metadata differs')
			return self.verify(p,key),'hit'
		except (ValueError,OSError,KeyError,json.JSONDecodeError):
			# Delete only the known, identity-checked root; record the miss for caller.
			self.remove(row)
			return None,'verification failed'

	@staticmethod
	def verify(path,key):
		path=canonical(path)
		r=json.loads(canonical(path/'ANCHOR.json').read_text())
		if r.get('schema')!=2 or r.get('status')!='PASS' or r.get('capture_key')!=key or not r.get('artifacts'):raise ValueError('invalid anchor bundle')
		for name,sha in r['artifacts'].items():
			p=canonical(name)
			if not p.is_relative_to(path) or not p.is_file() or child.digest(p)!=sha:raise ValueError('cache artifact differs')
		if r['artifacts'].get(r['binary'])!=r['binary_sha256']:raise ValueError('unbound anchor binary')
		for record in r['records']:
			p=Path(record['profile'])
			for f,sha in ((p,record['profile_sha256']),(p.parent/'program.rn',record['fixture_sha256']),(p.parent/'program.stdout',record['stdout_sha256'])):
				if r['artifacts'].get(str(f))!=sha:raise ValueError('unbound anchor row')
		return r

	def publish(self,receipt,binary,provenance):
		# The source was verified by the caller; verify again while copying each file.
		ident=uuid.uuid4().hex; pending=self.root/('pending-'+ident); dest=self.root/('entry-'+ident)
		ps=self.root.stat()
		row=dict(id=ident,path=str(pending),key=receipt['capture_key'],state='creating',parent_device=ps.st_dev,parent_inode=ps.st_ino)
		self.state['entries'].append(row);self.save()
		pending.mkdir();sync_directory(self.root);s=pending.stat()
		row.update(state='building',device=s.st_dev,inode=s.st_ino);self.save()
		try:
			r=copy.deepcopy(receipt)
			r.pop('reuse_proof',None)
			r.update(schema=2,provenance=provenance,artifacts={},binary=str(dest/'rnx-anchor'))
			def copied(source,relative,sha):
				if child.digest(source)!=sha:raise ValueError('anchor source changed')
				p=pending/relative;p.parent.mkdir(parents=True,exist_ok=True)
				shutil.copyfile(source,p)
				if child.digest(p)!=sha:raise ValueError('anchor copy differs')
				with p.open('rb') as f:os.fsync(f.fileno())
				r['artifacts'][str(dest/relative)]=sha
			copied(binary,Path('rnx-anchor'),r['binary_sha256'])
			for i,record in enumerate(r['records']):
				profile=Path(record['profile']);base=Path('cases')/f'{i:02d}'
				for source,name,sha in ((profile,'profile',record['profile_sha256']),(profile.parent/'program.rn','program.rn',record['fixture_sha256']),(profile.parent/'program.stdout','program.stdout',record['stdout_sha256'])):
					copied(source,base/name,sha)
				record['profile']=str(dest/base/'profile')
			child.atomic_json(pending/'ANCHOR.json',r)
			for directory,_,_ in os.walk(pending,topdown=False):sync_directory(directory)
			row.update(state='publishing',path=str(dest),rename_from=str(pending));self.save()
			os.rename(pending,dest);sync_directory(self.root)
			self.verify(dest,r['capture_key'])
			row.update(state='complete',receipt_sha256=child.digest(dest/'ANCHOR.json'));row.pop('rename_from');self.state['current']=ident;self.save()
			for old in list(self.state['entries']):
				if old['id']!=ident:self.remove(old)
			return r
		except BaseException:
			# Journal survives a failed cleanup, so a later lock holder can recover.
			if row in self.state['entries'] and row['state']!='complete':self.remove(row)
			raise
