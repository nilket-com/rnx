"""Atomic durable artifacts and identity-exact resume, with no failure retries."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import tempfile


def digest(path):
	h=hashlib.sha256()
	with Path(path).open('rb') as f:
		while b:=f.read(1024*1024): h.update(b)
	return h.hexdigest()


def atomic(path, value):
	path=Path(path); path.parent.mkdir(parents=True,exist_ok=True)
	fd,tmp=tempfile.mkstemp(prefix='.pending-',dir=path.parent)
	try:
		with os.fdopen(fd,'w') as f:
			json.dump(value,f,sort_keys=True,indent=2); f.write('\n'); f.flush(); os.fsync(f.fileno())
		os.replace(tmp,path)
		fd=os.open(path.parent,os.O_RDONLY|os.O_DIRECTORY)
		try: os.fsync(fd)
		finally: os.close(fd)
	finally:
		if os.path.exists(tmp): os.unlink(tmp)


def lock(path):
	path=Path(path); path.parent.mkdir(parents=True,exist_ok=True)
	f=path.open('a'); fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB)
	return f


def reuse(directory, identity):
	path=Path(directory)/'RESULT.json'
	if not path.exists(): return None
	r=json.loads(path.read_text())
	if r['identity']!=identity: raise RuntimeError('resume identity mismatch')
	if r['status']!='PASS': raise RuntimeError('failed case retained; no automatic retry')
	for name,sha in r['artifacts'].items():
		if digest(Path(directory)/name)!=sha: raise RuntimeError('artifact mismatch')
	return r
