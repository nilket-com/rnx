"""Build the identical portable host probe in same-path fresh exports by hash."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import traceback
from child_result import atomic_json, digest, environment

ROOT = Path(__file__).resolve().parents[2]


def run(out, baseline, candidate, cargo):
	out.mkdir(parents=True, exist_ok=False)
	result = dict(status='FAIL', builds=[])
	try:
		probe = Path(__file__).with_name('portable_probe.rs').read_bytes()
		locks = [subprocess.check_output(['git', 'show', rev + ':Cargo.lock'], cwd=ROOT)
			for rev in (baseline, candidate)]
		assert locks[0] == locks[1], 'dependency lock changed'
		command = [str(cargo), '+1.98.1', 'build', '--locked', '--release', '--no-default-features', '--example', 'column_force']
		for role, revision in (('baseline', baseline), ('candidate', candidate)):
			revision = subprocess.check_output(['git', 'rev-parse', revision + '^{commit}'], cwd=ROOT, text=True).strip()
			archive = out / (role + '.tar')
			with archive.open('xb') as stream:
				subprocess.run(['git', 'archive', revision], cwd=ROOT, stdout=stream, check=True)
			source = out / 'source'
			if source.exists():
				shutil.rmtree(source)  # Only this invocation's prior fresh export.
			source.mkdir()
			subprocess.run(['tar', '-xf', str(archive), '-C', str(source)], check=True)
			(source / 'tools/column/portable_probe.rs').write_bytes(probe)
			with (source / 'Cargo.toml').open('a') as stream:
				stream.write('\n[[example]]\nname = "column_force"\npath = "tools/column/portable_probe.rs"\n')
			# Archive mtimes predate builds: never share Cargo fingerprints across revisions.
			env = dict(os.environ, CARGO_BUILD_JOBS='2', CARGO_TARGET_DIR=str(out / (role + '-cache')), CARGO_INCREMENTAL='0')
			with (out / (role + '.log')).open('x') as stream:
				p = subprocess.run(command, cwd=source, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=14400)
			assert p.returncode == 0, (role, 'build failed', p.returncode)
			binary = out / (role + '-portable')
			shutil.copy2(out / (role + '-cache') / 'release/examples/column_force', binary)
			row = dict(role=role, source_commit=revision, binary=str(binary), sha256=digest(binary),
				archive_sha256=digest(archive), probe_sha256=hashlib.sha256(probe).hexdigest(),
				lock_sha256=hashlib.sha256(locks[0]).hexdigest(), command=command, environment=environment(env))
			atomic_json(out / (role + '-BUILD.json'), dict(status='PASS', **row))
			result['builds'].append(row)
		result['status'] = 'PASS'
	except Exception:
		result['error'] = traceback.format_exc()
	finally:
		atomic_json(out / 'RESULT.json', result)
		print(result['status'], out / 'RESULT.json', flush=True)
	return result['status'] != 'PASS'


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--cargo', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	p.add_argument('--baseline', required=True)
	p.add_argument('--candidate', required=True)
	a = p.parse_args()
	return run(a.out.resolve(), a.baseline, a.candidate, a.cargo.absolute())


if __name__ == '__main__':
	raise SystemExit(main())
