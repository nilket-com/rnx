"""ADR 0211 canonical engine provenance checks; no dependency fetches."""
from pathlib import Path
import tomllib


def engine_name(name):
	return name == 'rune' or name.startswith('rune-')


def dependency_specs(document):
	for section in ('dependencies', 'dev-dependencies', 'build-dependencies'):
		for alias, spec in document.get(section, {}).items():
			if engine_name(spec.get('package', alias) if isinstance(spec, dict) else alias):
				yield alias, spec
	for target in document.get('target', {}).values():
		yield from dependency_specs(target)
	for table in document.get('patch', {}).values():
		yield from dependency_specs({'dependencies': table})
	if 'workspace' in document:
		yield from dependency_specs({'dependencies': document['workspace'].get('dependencies', {})})


def canonical_manifest(root, name):
	engine = (Path(root) / 'rune').resolve(strict=True)
	path = engine / 'crates' / name / 'Cargo.toml'
	resolved = path.resolve(strict=True)
	if not resolved.is_relative_to(engine):
		raise ValueError('canonical engine path escapes imported source')
	return resolved


def check_manifest(root, path):
	path = Path(path)
	doc = tomllib.loads(path.read_text())
	checked = []
	for alias, spec in dependency_specs(doc):
		if not isinstance(spec, dict) or 'git' in spec or 'registry' in spec or 'registry-index' in spec:
			raise ValueError(f'{path}: external engine dependency {alias}')
		name = spec.get('package', alias)
		if spec.get('workspace'):
			raise ValueError(f'{path}: engine dependencies require explicit canonical paths')
		if 'path' not in spec:
			raise ValueError(f'{path}: missing canonical engine path')
		selected = (path.parent / spec['path'] / 'Cargo.toml').resolve(strict=True)
		if selected != canonical_manifest(root, name):
			raise ValueError(f'{path}: engine dependency path differs from canonical source')
		checked.append(name)
	return checked


def check_lock(path):
	seen = set()
	for package in tomllib.loads(Path(path).read_text()).get('package', []):
		if not engine_name(package['name']):
			continue
		if package.get('source') is not None:
			raise ValueError(f'{path}: external engine source in lock')
		if package['name'] in seen:
			raise ValueError(f'{path}: duplicate engine package')
		seen.add(package['name'])
	return sorted(seen)


def check_metadata(root, document):
	seen = set()
	for package in document['packages']:
		name = package['name']
		if not engine_name(name):
			continue
		if package['source'] is not None or name in seen:
			raise ValueError('external or duplicate resolved engine source')
		manifest = Path(package['manifest_path']).resolve(strict=True)
		# Active engine tools may have Rune-prefixed names outside crates/.
		engine = (Path(root) / 'rune').resolve(strict=True)
		if not manifest.is_relative_to(engine):
			raise ValueError('resolved engine manifest escapes canonical subtree')
		if (engine / 'crates' / name).exists() and manifest != canonical_manifest(root, name):
			raise ValueError('resolved Rune manifest is not canonical')
		seen.add(name)
	if 'rune' not in seen:
		raise ValueError('resolved graph has no canonical Rune')
	return sorted(seen)


def check_tree(root):
	import os
	root = Path(root).resolve(strict=True)
	checked = {'mode': 'canonical-path', 'manifests': [], 'locks': []}
	for directory, folders, files in os.walk(root, followlinks=False):
		here = Path(directory)
		folders[:] = sorted(name for name in folders if name not in {'.git', 'target', '__pycache__'}
			and not (here == root and name in {'probes', 'provenance'})
			and not (here / name).is_symlink())
		for name in sorted(set(files) & {'Cargo.toml', 'Cargo.lock'}):
			path = here / name
			found = check_manifest(root, path) if name == 'Cargo.toml' else check_lock(path)
			if found:
				checked['manifests' if name == 'Cargo.toml' else 'locks'].append(str(path.relative_to(root)))
	for required in ['Cargo.lock', 'tools/project/Cargo.lock']:
		if required not in checked['locks']:
			raise ValueError('missing canonical engine lock graph: ' + required)
	return checked
