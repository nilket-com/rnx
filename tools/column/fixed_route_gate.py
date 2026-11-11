"""Reusable source/identity/band gate with a directly measured cumulative anchor."""
import argparse
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import traceback

import child_result as child
import drift_band as band
from cross_program_gate import stage
from paired_build import build_environment, BUILD_KEYS, BUILD_PREFIXES

ROOT = Path(__file__).resolve().parents[2]


def valgrind_root(explicit=None):
	configured = explicit or os.environ.get('RNX_VALGRIND_ROOT')
	if configured:
		root = Path(configured).resolve()
	else:
		binary = shutil.which('valgrind')
		if not binary:
			raise ValueError('Valgrind missing: set RNX_VALGRIND_ROOT or put valgrind on PATH')
		root = Path(binary).resolve().parent.parent
	if not (root/'bin/valgrind').is_file() or not os.access(root/'bin/valgrind', os.X_OK):
		raise ValueError('Valgrind root must contain executable bin/valgrind')
	return root


def recipe(policy):
	from ir_pool import capture_environment
	cpu = Path('/proc/cpuinfo').read_text().split('\n\n')[0]
	cpu = [s for s in cpu.splitlines() if s.startswith(('model name', 'vendor_id', 'flags'))]
	tools = {}
	for name in ('rustc', 'cargo'):
		path = subprocess.check_output(['rustup', 'which', '--toolchain', '1.98.1', name], text=True).strip()
		tools[name] = child.digest(path)
	return dict(build=['cargo', '+1.98.1', 'build', '--locked', '--release', '--bin', 'rnx'],
		toolchain=tools, cpu=cpu, environment=child.environment(capture_environment()), run_environment=capture_environment(),
		build_environment=dict(sha256=child.identity(build_environment(os.environ)), keys=sorted(build_environment(os.environ)),
			allowlist=sorted(BUILD_KEYS), prefixes=list(BUILD_PREFIXES)),
		policy={k:policy[k] for k in ('schema','anchor','kernels','modes','sizes','per_record_per_mille','cumulative_per_mille')}, tooling={p: child.digest(ROOT / p) for p in (
			'tools/column/paired_build.py', 'tools/column/fixed_route_vectors.py', 'tools/column/old_path_child.py',
			'tools/column/ir_accounting.py', 'tools/column/old_path_programs.py')},
		fixtures={p.name: child.digest(p) for p in sorted((ROOT/'tools/routing-bench/kernels').glob('*.rn'))})


def build(out, baseline, candidate, cargo):
	return stage(out, 'build', [sys.executable, '-B', 'tools/column/paired_build.py', '--out', str(out/'pair'),
		'--baseline', baseline, '--candidate', candidate, '--cargo', str(cargo)], out/'pair/RESULT.json')


measurement_recipe = band.measurement_recipe


def capture_key(binary_sha256, current):
	return child.identity(dict(binary_sha256=binary_sha256, measurement=measurement_recipe(current)))


def captures(out, pair, valgrind, policy, current_recipe):
	baseline, candidate = [b['source_commit'] for b in pair['builds']]
	if baseline != candidate:
		command = [sys.executable, '-B', 'tools/column/executor_identity.py', '--baseline', pair['builds'][0]['binary'],
			'--candidate', pair['builds'][1]['binary'], '--out', str(out/'IDENTITY.json')]
		with (out/'identity.log').open('x') as log:
			status = subprocess.run(command, cwd=ROOT, stdout=log, stderr=subprocess.STDOUT).returncode
		ident = json.loads((out/'IDENTITY.json').read_text())
		if status not in (0, 1) or ident.get('error'): raise ValueError('identity tool failed, not a measured mismatch')
	vectors = stage(out, 'capture', [sys.executable, '-B', 'tools/column/fixed_route_vectors.py', '--capture-only',
		'--out', str(out/'vectors'), '--build-receipt', str(out/'pair/RESULT.json'),
		'--valgrind-root', str(valgrind)], out/'vectors/RESULT.json')
	rows = [json.loads(Path(p).read_text()) for p in vectors['records']]
	result = []
	for role, revision, binary in zip(('baseline', 'candidate'), (baseline, candidate), pair['builds']):
		selected = [r for r in rows if r['role'] == role]
		artifacts = {binary['binary']: binary['sha256']}
		for r in selected:
			artifacts[r['profile']] = r['profile_sha256']
			p = Path(r['profile']).parent
			artifacts[str(p/'program.rn')] = r['fixture_sha256']
			artifacts[str(p/'program.stdout')] = r['stdout_sha256']
		result.append(dict(status='PASS', revision=revision, recipe=current_recipe, policy_sha256=child.identity(policy),
			binary_sha256=binary['sha256'], capture_key=capture_key(binary['sha256'], current_recipe),
			records=selected, artifacts=artifacts))
	return result


def reuse_anchor(path, binary, current, policy):
	original = verified(json.loads(Path(path).read_text()))
	if original['revision'] != policy['anchor']: raise ValueError('wrong imported anchor revision')
	# Legacy receipts locate their binary in the preserved capture manifest.
	profile = Path(original['records'][0]['profile'])
	manifest_path = profile.parent.parent/'MANIFEST.json'
	manifest = json.loads(manifest_path.read_text())
	role = original['records'][0]['role']; measured = manifest['binaries'][role]
	if child.digest(measured['binary']) != measured['sha256'] or measured['sha256'] != binary['sha256']:
		raise ValueError('anchor binary differs; new capture required')
	measured_recipe = dict(original['recipe'], run_environment=manifest['environment'])
	if original['recipe'].get('run_environment', manifest['environment']) != manifest['environment']:
		raise ValueError('capture environment provenance mismatch')
	if child.environment(manifest['environment']) != original['recipe']['environment']:
		raise ValueError('legacy environment identity mismatch')
	if measurement_recipe(measured_recipe) != measurement_recipe(current):
		raise ValueError('anchor measurement inputs differ; new capture required')
	from ir_pool import capture_environment
	if manifest['environment'] != capture_environment(): raise ValueError('anchor run environment differs')
	if child.digest(binary['binary']) != binary['sha256']: raise ValueError('current build binary changed')
	artifacts = dict(original['artifacts'])
	artifacts.update({str(Path(path).resolve()): child.digest(path), str(manifest_path): child.digest(manifest_path), binary['binary']: binary['sha256']})
	return dict(original, recipe=measured_recipe, artifacts=artifacts, binary_sha256=binary['sha256'], capture_key=capture_key(binary['sha256'],current),
		reuse_proof=dict(original_receipt=str(Path(path).resolve()), original_sha256=child.digest(path),
			manifest_sha256=child.digest(manifest_path), binary_sha256=binary['sha256'],
			current_build=binary, current_recipe=current))


def verified(receipt):
	if receipt.get('status') != 'PASS' or not receipt.get('artifacts'): raise ValueError('invalid capture receipt')
	for path, sha in receipt['artifacts'].items():
		if child.digest(path) != sha: raise ValueError('capture artifact changed: ' + path)
	return receipt


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out', type=Path, required=True)
	p.add_argument('--cargo', type=Path, default=Path(shutil.which('cargo') or 'cargo'))
	p.add_argument('--valgrind-root', type=Path, default=None)
	p.add_argument('--anchor-only', action='store_true')
	p.add_argument('--import-anchor', type=Path, help='Verify and import an immutable prior capture under the binary identity rule')
	a = p.parse_args(); out = a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL')
	try:
		a.valgrind_root = valgrind_root(a.valgrind_root)
		import gate_policy
		info = gate_policy.record(ROOT); candidate = info['candidate']; source = child.source(ROOT)
		if subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT): raise ValueError('clean candidate required')
		policy = json.loads(band.POLICY.read_text()); band.validate_policy(policy); band.anchor_approval(ROOT, info['baseline'], candidate, policy)
		with (out/'guard-build.log').open('x') as log:
			subprocess.run([str(a.cargo), '+1.98.1', 'build', '--locked', '--manifest-path', 'ci/execute_guard/Cargo.toml'], cwd=ROOT, stdout=log, stderr=subprocess.STDOUT, check=True)
		guard = ROOT/'ci/execute_guard/target/debug/execute-guard'
		if not guard.exists():
			meta = json.loads(subprocess.check_output([str(a.cargo), '+1.98.1', 'metadata', '--no-deps', '--format-version=1', '--manifest-path', 'ci/execute_guard/Cargo.toml'], cwd=ROOT))
			guard = Path(meta['target_directory'])/'debug'/meta['packages'][0]['targets'][0]['name']
		r['source_condition'] = band.source_condition(ROOT, info['baseline'], candidate, policy, guard)
		with (out/'mirror.log').open('x') as log: subprocess.run([str(guard), str(ROOT)], stdout=log, stderr=subprocess.STDOUT, check=True)
		current = recipe(policy)
		# Valgrind's executable is an explicit recipe input, never merely a path.
		current['valgrind'] = child.digest(a.valgrind_root/'bin/valgrind')
		attempt = out/'anchor'; attempt.mkdir()
		anchor_pair = build(attempt, policy['anchor'], policy['anchor'], a.cargo)
		anchor_binary = anchor_pair['builds'][0]
		cache = ROOT/'target/fixed-route-anchors'/capture_key(anchor_binary['sha256'],current)
		cache.mkdir(parents=True, exist_ok=True)
		import fcntl
		with (cache/'LOCK').open('a+') as lock:
			fcntl.flock(lock, fcntl.LOCK_EX)
			anchor_path = cache/'ANCHOR.json'
			if anchor_path.exists():
				anchor = reuse_anchor(anchor_path, anchor_binary, current, policy)
			elif a.import_anchor:
				# A different binary is a cache miss, never a license to relabel it.
				try: anchor = reuse_anchor(a.import_anchor, anchor_binary, current, policy)
				except ValueError as error:
					if 'new capture required' not in str(error): raise
					r['import_miss'] = str(error)
					anchor = captures(attempt, anchor_pair, a.valgrind_root, policy, current)[0]
				child.atomic_json(anchor_path, anchor)
			else:
				anchor = captures(attempt, anchor_pair, a.valgrind_root, policy, current)[0]
				child.atomic_json(anchor_path, anchor)
		child.atomic_json(out/'ANCHOR-EVIDENCE.json', anchor)
		if a.anchor_only:
			r.update(status='PASS', anchor=str(out/'ANCHOR-EVIDENCE.json'), cache=str(anchor_path), recipe=current)
		else:
			pair = out/'record'; pair.mkdir()
			built = build(pair, info['baseline'], candidate, a.cargo)
			baseline, changed = captures(pair, built, a.valgrind_root, policy, current)
			r.update(band.score(verified(baseline), verified(changed), anchor, policy), baseline=baseline, candidate=changed, anchor=str(out/'ANCHOR-EVIDENCE.json'))
		if child.source(ROOT) != source or child.head(ROOT) != candidate: raise ValueError('source changed during gate')
	except Exception:
		r.update(status='FAIL', error=traceback.format_exc())
	finally:
		child.publish(out/'RESULT.json', r)
		print(r['status'], out/'RESULT.json', flush=True)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
