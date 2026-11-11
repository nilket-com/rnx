"""Conservative path coverage, including both dependency graphs on graph edits."""
import json
from pathlib import Path
import subprocess
import tarfile
import tempfile


def graph_change(paths):
	return any(Path(p).name in {'Cargo.toml', 'Cargo.lock'} for p in paths)


def direct(paths):
	stages = {'commit-discipline', 'controller-controls'}
	for p in paths:
		if p.startswith(('src/', 'tests/')): stages.update(('root-tests', 'inventory', 'cross'))
		if p.startswith('rune/crates/'):
			stages.update(('rune-suite', 'rune-clippy', 'cross'))
			if '/src/' in p: stages.add('fixed-route')
		if p.startswith('adapters/'):
			name = p.split('/')[1]
			if name not in {'candle', 'polars', 'postgres'}: raise ValueError('unregistered adapter: ' + name)
			stages.update(('suite-adapter-' + name, 'clippy-adapter-' + name))
		if p.startswith('jupyter/'): stages.update(('jupyter-format', 'jupyter-clippy', 'jupyter-tests', 'jupyter-kernel'))
		if p.startswith('tools/project/'): stages.update(('project-suite', 'project-clippy', 'cross'))
		if p.startswith('tools/verify/') or p == 'tools/verify.py': stages.add('cross')
	return stages


def graph(root, revision):
	"""Cargo resolves manifests, including optional path dependencies, at each side."""
	with tempfile.TemporaryDirectory(prefix='required-graph-') as tmp:
		directory = Path(tmp)
		archive = directory/'source.tar'
		with archive.open('wb') as f: subprocess.run(['git', '-C', str(root), 'archive', revision], stdout=f, check=True)
		with tarfile.open(archive) as t: t.extractall(directory, filter='data')
		nodes = {}
		for manifest in sorted(directory.rglob('Cargo.toml')):
			relative = manifest.relative_to(directory).as_posix()
			if relative.startswith('probes/'): continue  # Archived experiments, never active packages.
			if relative in nodes: continue  # Already resolved as a workspace member.
			# Cargo metadata does not build or run tests. --no-deps still exposes
			# every declared dependency (incl optional/dev/build) with path roots.
			data = json.loads(subprocess.check_output(['cargo', '+1.98.1', 'metadata', '--offline', '--no-deps', '--format-version=1', '--manifest-path', str(manifest)], cwd=directory, stderr=subprocess.PIPE))
			for package in data['packages']:
				path = Path(package['manifest_path']).resolve().relative_to(directory).as_posix()
				deps = []
				for dep in package['dependencies']:
					if dep.get('path'):
						p = (Path(dep['path'])/'Cargo.toml').resolve()
						if not p.is_relative_to(directory): raise ValueError('external graph input')
						deps.append(p.relative_to(directory).as_posix())
				nodes[path] = sorted(set(deps))
		return nodes


def affected(paths, graphs):
	selected = set()
	for nodes in graphs:
		seeds = set()
		for manifest in nodes:
			parent = Path(manifest).parent
			for path in paths:
				if Path(path).name in {'Cargo.toml', 'Cargo.lock'}:
					# Ancestor manifests/locks can alter every enclosed package.
					if parent.is_relative_to(Path(path).parent): seeds.add(manifest)
				elif Path(path).is_relative_to(parent): seeds.add(manifest)
		while True:
			next_seeds = seeds | {p for p, deps in nodes.items() if set(deps) & seeds}
			if next_seeds == seeds: break
			seeds = next_seeds
		selected.update(seeds)
	return selected


def required(paths, graphs=None):
	stages = direct(paths)
	if not graph_change(paths): return sorted(stages)
	stages.add('cross')
	if graphs is None: raise ValueError('manifest/lock change requires before and after Cargo graphs')
	for manifest in affected(paths, graphs):
		if manifest == 'Cargo.toml': stages.update(('root-tests', 'inventory'))
		elif manifest.startswith('rune/crates/'):
			stages.update(('rune-suite', 'rune-clippy', 'fixed-route'))
		elif manifest.startswith('adapters/'):
			stages.update(direct([str(Path(manifest).parent/'src/lib.rs')]))
		elif manifest == 'jupyter/Cargo.toml': stages.update(('jupyter-format', 'jupyter-clippy', 'jupyter-tests', 'jupyter-kernel'))
		elif manifest == 'tools/project/Cargo.toml': stages.update(('project-suite', 'project-clippy'))
		elif manifest.startswith(('ci/', 'rune/tools/')):
			stages.add('controller-controls')
		else: raise ValueError('affected package needs a registered suite: ' + manifest)
	return sorted(stages)
