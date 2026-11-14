"""Content-keyed prepared targets and one exact Miri test per durable job."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.util
import json
from pathlib import Path
import sys
import traceback
import uuid

from owned_build import OwnedBuild
import child_result as child
from miri_cache import Cache, description, input_files

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT/'rune/tools/xtask/supplemental'
sys.path.insert(0, str(HERE))
spec = importlib.util.spec_from_file_location('exact_miri_backend', HERE/'run.py')
backend = importlib.util.module_from_spec(spec); spec.loader.exec_module(backend)


def manifest(package, inventory):
	if inventory.get('schema') != 1 or not inventory.get('tests'): raise ValueError('empty/invalid inventory')
	names = [row['name'] for row in inventory['tests']]
	if len(set(names)) != len(names) or not all(isinstance(n, str) and n for n in names): raise ValueError('duplicate/invalid test')
	m = dict(package=package, stable='1.98.1', nightly=inventory['toolchain'], features={'selected': inventory['features']}, subsets=[])
	for i, row in enumerate(inventory['tests']):
		if type(row['ignored']) is not bool or type(row['timeout_seconds']) is not int or row['timeout_seconds'] <= 0:
			raise ValueError('invalid ignored/timeout declaration')
		m['subsets'].append(dict(name=f'exact-{i:04d}', feature='selected', selector=row['name'], exact=True,
			inventory=[row['name']], shards=[[row['name']]], ignored=[row['name']] if row['ignored'] else [], timeout_seconds=row['timeout_seconds']))
	return m


def run(root, package, inventory, cache_root, workers=None):
	root = Path(root).resolve(); m = manifest(package, inventory)
	env = backend.environment(); env['MIRIFLAGS'] = inventory['miriflags']
	before = backend.source_identity(root, m, env)
	inputs = input_files(root, package, inventory['features'], inventory['toolchain'])
	recipe = description(root, inputs, inventory['toolchain'], env['MIRIFLAGS'], inventory['features'],
		dict(before['tools'], backend=before['runner'], runner=child.digest(__file__), cache=child.digest(Path(__file__).with_name('miri_cache.py')), cleanup=child.digest(Path(__file__).with_name('owned_build.py'))), child.environment(env), package)
	cache = Cache(cache_root, recipe)
	limits = backend.observed_admission(backend.resources(), 4*1024**3, requested=workers if workers is not None else 6)
	prepare = lambda path: backend.preparation(path, root, m, env, 'selected')
	prepared = cache.stage('prepare', '', prepare)
	jobs = cache.order(backend.selection(m, {'selected': prepared}))
	if not jobs: raise ValueError('empty exact selection')
	def job(spec):
		# Ignored state and execution timeout belong to this test's recipe too.
		test_cache = Cache(cache_root, dict(recipe, test_contract={'ignored': spec['ignored'], 'timeout': spec['timeout']}))
		def action(path):
			with OwnedBuild(path, 'target') as owned:
				details, artifacts = backend.semantic(path, root, m, env, spec, prepared)
				owned.keep(artifacts)
			details['cleanup'] = owned.result
			return details, [p for p in path.iterdir() if p.is_file()]
		return test_cache.stage('test', spec['names'][0], action)
	with ThreadPoolExecutor(max_workers=limits['workers']) as pool: rows = list(pool.map(job, jobs))
	cache.stage('prepare', '', prepare)
	if backend.source_identity(root, m, env) != before or input_files(root, package, inventory['features'], inventory['toolchain']) != inputs:
		raise ValueError('source/toolchain changed during Miri')
	return dict(resources=limits, recipe=recipe, prepared=prepared, jobs=rows,
		covered_names=sorted(n for r in rows for n in r['names']), schedule=[s['names'][0] for s in jobs])


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--package', required=True); p.add_argument('--inventory', type=Path, required=True)
	p.add_argument('--root', type=Path, default=ROOT); p.add_argument('--cache', type=Path)
	p.add_argument('--out', type=Path, required=True); p.add_argument('--workers', type=int)
	p.add_argument('--invalidate-key'); p.add_argument('--reason')
	a = p.parse_args(); a.out.mkdir(parents=True, exist_ok=True); r = dict(status='FAIL')
	try:
		if a.invalidate_key:
			if not a.reason: raise ValueError('invalidation reason required')
			r['invalidated'] = str(Cache(a.cache or a.root/'target/miri-exact-cache', {}).invalidate(a.invalidate_key, a.reason))
		r.update(run(a.root, a.package, json.loads(a.inventory.read_text()), a.cache or a.root/'target/miri-exact-cache', a.workers), status='PASS')
	except Exception as error:
		r['error'] = traceback.format_exc()
		if hasattr(error, 'numbers'): r['admission'] = error.numbers
	finally:
		path = a.out/('INVOCATION-'+uuid.uuid4().hex+'.json'); child.publish(path, r)
		print(r['status'], path, flush=True)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
