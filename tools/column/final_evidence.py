"""Output roots a final assigns to its stages, as the stage programs read them."""
import os
from pathlib import Path


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
