"""Build paired production CLIs from hash exports with isolated target directories."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import traceback
from child_result import atomic_json, digest, environment

ROOT = Path(__file__).resolve().parents[2]


def fresh_target(out, index):
	# Equal-length role paths; never seed or share Cargo fingerprints/artifacts.
	path = out / f'cache-{index:02d}'
	path.mkdir(exist_ok=False)
	return path


def compiled_crates(text, required):
	compiled = set(re.findall(r'^\s*Compiling ([A-Za-z0-9_-]+) v', text, re.MULTILINE))
	missing = set(required) - compiled
	if missing:
		raise RuntimeError('missing fresh compilation: ' + ', '.join(sorted(missing)))
	return sorted(compiled)


def cleanup_target(target, binary, sha256, build_receipt):
	"""Remove only an invocation-owned cache after its copied artifact is durable."""
	target, binary, build_receipt = map(Path, (target, binary, build_receipt))
	assert re.fullmatch(r'cache-[0-9]{2}', target.name), 'unexpected cache name'
	assert target.is_dir() and not target.is_symlink(), 'not an owned cache directory'
	assert binary.parent.resolve() == target.parent.resolve(), 'binary must be outside cache'
	receipt = json.loads(build_receipt.read_text())
	assert receipt['status'] == 'PASS' and receipt['sha256'] == sha256
	assert Path(receipt['binary']).resolve() == binary.resolve()
	assert digest(binary) == sha256, 'copied binary does not verify'
	with binary.open('rb') as stream:
		os.fsync(stream.fileno())
	seen = set()
	logical = allocated = 0
	for directory, _, files in os.walk(target, followlinks=False):
		for name in files:
			stat = (Path(directory) / name).lstat()
			key = (stat.st_dev, stat.st_ino)
			if key not in seen:
				seen.add(key)
				logical += stat.st_size
				allocated += stat.st_blocks * 512
	journal = target.parent / (target.name + '-CLEANUP.json')
	row = dict(status='STARTED', removed_path=str(target), logical_file_bytes=logical,
		allocated_file_bytes=allocated, binary=str(binary), sha256=sha256,
		build_receipt=str(build_receipt), build_receipt_sha256=digest(build_receipt))
	atomic_json(journal, row)
	shutil.rmtree(target)
	assert digest(binary) == sha256, 'copied binary changed during cleanup'
	row.update(status='PASS', removed=not target.exists(), binary_verified=True)
	atomic_json(journal, row)
	return dict(receipt=str(journal), sha256=digest(journal))


def run(out, baseline, candidate, cargo):
	out.mkdir(parents=True, exist_ok=False)
	result = dict(status='FAIL', builds=[])
	try:
		subprocess.run([sys.executable, '-B', str(ROOT / 'ci/disk_space.py'),
			'--path', str(out)], check=True, capture_output=True, text=True)
		command = [str(cargo), '+1.98.1', 'build', '--locked', '--release',
			'--bin', 'rnx']
		for index, (role, revision) in enumerate((('baseline', baseline), ('candidate', candidate))):
			revision = subprocess.check_output(['git', '-c', 'color.ui=false', 'rev-parse', revision + '^{commit}'], cwd=ROOT, text=True).strip()
			archive = out / (role + '.tar')
			with archive.open('xb') as stream:
				subprocess.run(['git', '-c', 'color.ui=false', 'archive', revision], cwd=ROOT, stdout=stream, check=True)
			source = out / 'source'
			if index:
				shutil.rmtree(source)  # Only this invocation's preceding export.
			source.mkdir()
			subprocess.run(['tar', '-xf', str(archive), '-C', str(source)], check=True)
			target = fresh_target(out, index)
			env = dict(os.environ, CARGO_BUILD_JOBS='4', CARGO_TARGET_DIR=str(target),
				CARGO_INCREMENTAL='0', CARGO_TERM_COLOR='never')
			log = out / (role + '.log')
			with log.open('x') as stream:
				p = subprocess.run(command, cwd=source, env=env, stdout=stream,
					stderr=subprocess.STDOUT, timeout=14400)
			assert p.returncode == 0, (role, 'build failed', p.returncode)
			compiled = compiled_crates(log.read_text(), ('rune', 'rnx'))
			binary = out / ('rnx-' + role)
			shutil.copy2(target / 'release/rnx', binary)
			row = dict(role=role, source_commit=revision, rnx=revision, rune=revision,
				binary=str(binary), sha256=digest(binary), archive_sha256=digest(archive),
				command=command, environment=environment(env), target_directory=str(target),
				compiled_crates=compiled, log_sha256=digest(log))
			receipt = out / (role + '-BUILD.json')
			atomic_json(receipt, dict(status='PASS', **row))
			row['cleanup'] = cleanup_target(target, binary, row['sha256'], receipt)
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
	p.add_argument('--cargo', required=True, type=Path)
	p.add_argument('--out', required=True, type=Path)
	p.add_argument('--baseline', required=True)
	p.add_argument('--candidate', required=True)
	a = p.parse_args()
	return run(a.out.resolve(), a.baseline, a.candidate, a.cargo.absolute())


if __name__ == '__main__':
	raise SystemExit(main())
