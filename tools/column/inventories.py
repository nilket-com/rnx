"""Prepare pinned Polars inventories from source, then check generated bindings."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import time
import traceback

ROOT = Path(__file__).resolve().parents[2]
CONFIGURATIONS = ('adapter-json', 'adapter', 'adapter-narrow')
NIGHTLY = 'nightly-2026-09-20'
MANIFESTS = ('probes/0072/extract/Cargo.toml', 'tools/polars-gen/Cargo.toml')


def validate_inventory(cfg, pins, inventory):
	assert pins['release'] == '0.55.2' and pins['cfg'] == cfg and pins['toolchain'] == NIGHTLY
	assert pins['crates'] and not pins['failed'], 'incomplete documentation'
	provenance = inventory['provenance']
	assert provenance['release'] == '0.55.2' and provenance['cfg'] == cfg
	assert sorted(provenance['features']) == sorted(pins['resolved_polars_features'])
	assert inventory['callables'], 'empty inventory'


def main():
	parser = argparse.ArgumentParser(description=__doc__)
	parser.add_argument('--out', type=Path, required=True)
	parser.add_argument('--preflight', action='store_true', help='check pinned toolchain and source prerequisites only')
	args = parser.parse_args()
	out = args.out.resolve()
	out.mkdir(parents=True, exist_ok=False)
	env = dict(os.environ, CARGO_BUILD_JOBS='4', PYTHONDONTWRITEBYTECODE='1', RUSTUP_AUTO_INSTALL='0')
	env.pop('CARGO_TARGET_DIR', None)
	offline = dict(env, CARGO_NET_OFFLINE='true')
	started = time.monotonic()
	result = {'status': 'FAIL', 'toolchain': NIGHTLY, 'checks': [], 'inventories': []}
	def run(name, command, environment=offline):
		with (out / (name + '.stdout')).open('x') as stdout, (out / (name + '.stderr')).open('x') as stderr:
			p = subprocess.run(command, cwd=ROOT, env=environment, stdout=stdout, stderr=stderr, timeout=10800)
		result['checks'].append({'name': name, 'command': command, 'exit': p.returncode})
		assert p.returncode == 0, (name, p.returncode)
	def sha(path):
		return hashlib.sha256(path.read_bytes()).hexdigest()
	try:
		head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
		assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
		result['source_commit'] = head
		locks = [ROOT / f'probes/0072/inventory/locks/0.55.2-{cfg}.lock' for cfg in CONFIGURATIONS]
		locks += [(ROOT / manifest).with_name('Cargo.lock') for manifest in MANIFESTS]
		before = {str(path.relative_to(ROOT)): sha(path) for path in locks}
		# Toolchain installation is an environment prerequisite, not an implicit download.
		run('nightly-preflight', ['rustup', 'run', NIGHTLY, 'rustc', '--version'])
		if args.preflight:
			result['locks_sha256'] = before
			result['status'] = 'PASS'
			return 0
		for i, manifest in enumerate(MANIFESTS):
			run('fetch-tool-' + str(i), ['cargo', '+1.98.1', 'fetch', '--locked', '--manifest-path', manifest], env)
		for cfg in CONFIGURATIONS:
			run('fetch-' + cfg, ['bash', 'probes/0072/inventory/doc.sh', '0.55.2', cfg, '--fetch-only'], env)
		target = ROOT / 'target/column-inventory-extract'
		run('build-extractor', ['cargo', '+1.98.1', 'build', '--locked', '--manifest-path', MANIFESTS[0]], dict(offline, CARGO_TARGET_DIR=str(target)))
		for cfg in CONFIGURATIONS:
			run('document-' + cfg, ['bash', 'probes/0072/inventory/doc.sh', '0.55.2', cfg])
			docs = ROOT / ('probes/0072/out/0.55.2-' + cfg)
			pins = json.loads((docs / 'pins.json').read_text())
			assert pins['crates'] and not pins['failed'], pins['failed']
			assert all((docs / (crate + '.json')).is_file() for crate in pins['crates'])
			run('extract-' + cfg, [str(target / 'debug/surface'), str(docs), 'polars', str(docs / 'result')])
			inventory = docs / 'result/inventory.json'
			validate_inventory(cfg, pins, json.loads(inventory.read_text()))
			result['inventories'].append({'cfg': cfg, 'sha256': sha(inventory), 'pins_sha256': sha(docs / 'pins.json')})
		# Use the same policy and bucket selection as the full-suite drift test.
		tests = (ROOT / 'adapters/polars/tests/generated.rs').read_text()
		constants = {}
		for name in ('BUCKETS', 'RELEASE_FILE'):
			matches = re.findall(r'const ' + name + r': &str = "([^"]+)";', tests)
			assert len(matches) == 1, ('generator test contract changed', name)
			constants[name] = matches[0]
		run('generated-bindings-check', ['cargo', '+1.98.1', 'run', '--locked', '--manifest-path', MANIFESTS[1], '--',
			str(ROOT / 'probes/0072/out/0.55.2-adapter-json/result/inventory.json'), str(ROOT / 'adapters/polars'), '--buckets', constants['BUCKETS'],
			'--release', str(ROOT / 'tools/polars-gen/releases' / constants['RELEASE_FILE']), '--check'])
		assert {name: sha(ROOT / name) for name in before} == before, 'locked dependencies changed'
		result['locks_sha256'] = before
		assert subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip() == head
		assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
		result['status'] = 'PASS'
	except BaseException as error:
		result['error'] = repr(error)
		(out / 'TRACEBACK.txt').write_text(traceback.format_exc())
	finally:
		result['seconds'] = time.monotonic() - started
		(out / 'RESULT.json').write_text(json.dumps(result, indent=2) + '\n')
	print(result['status'], str(out / 'RESULT.json'))
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
