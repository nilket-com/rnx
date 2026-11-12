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
from owned_build import OwnedBuild, run_process

ROOT = Path(__file__).resolve().parents[2]


BUILD_KEYS = frozenset({'HOME', 'PATH', 'TMPDIR', 'CARGO_HOME', 'RUSTUP_HOME',
	'RUSTFLAGS', 'RUSTDOCFLAGS', 'RUSTUP_TOOLCHAIN', 'RUSTC', 'RUSTDOC',
	'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'CC', 'CXX', 'CFLAGS', 'CXXFLAGS',
	'LDFLAGS', 'AR', 'LANG', 'LC_ALL', 'TZ', 'PKG_CONFIG_PATH', 'PKG_CONFIG_LIBDIR',
	'PKG_CONFIG_SYSROOT_DIR', 'SOURCE_DATE_EPOCH'})
BUILD_PREFIXES = ('CARGO_', 'LC_')


def build_environment(environ):
	# Unknown compiler overrides fail rather than entering an unkeyed build.
	for key in environ:
		if key.startswith(('RUST', 'CC_', 'CXX_', 'CFLAGS_', 'LDFLAGS_', 'PKG_CONFIG_')) and key not in BUILD_KEYS:
			raise ValueError('undeclared build variable: ' + key)
	env = {k:v for k,v in environ.items() if k in BUILD_KEYS or k.startswith(BUILD_PREFIXES)}
	rustup = shutil.which('rustup')
	if rustup is None: raise ValueError('rustup unavailable')
	compiler = subprocess.check_output([rustup, 'which', '--toolchain', '1.98.1', 'rustc'], text=True).strip()
	paths = [str(Path(rustup).absolute().parent), str(Path(compiler).parent), '/usr/local/bin', '/usr/bin', '/bin']
	env = {k:v for k,v in env.items() if not k.startswith('LC_')}
	env.update(PATH=os.pathsep.join(dict.fromkeys(paths)), LANG='C', LC_ALL='C', TZ='UTC',
		CARGO_BUILD_JOBS='4', CARGO_INCREMENTAL='0', CARGO_TERM_COLOR='never')
	# Target is owned by the caller, never inherited from another job.
	env.pop('CARGO_TARGET_DIR', None)
	return env


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


def verify_copy(target, binary, sha256, build_receipt):
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
	return dict(binary=str(binary), sha256=sha256, build_receipt=str(build_receipt))


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
			with OwnedBuild(out, f'cache-{index:02d}') as owned:
				target = fresh_target(out, index)
				env = dict(build_environment(os.environ), CARGO_TARGET_DIR=str(target))
				log = out / (role + '.log')
				with log.open('x') as stream:
					p = run_process(command, cwd=source, env=env, stdout=stream,
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
				row['retained_copy'] = verify_copy(target, binary, row['sha256'], receipt)
				owned.keep([binary, receipt])
			row['cleanup'] = dict(receipt=str(out/(target.name+'-RELEASE.json')), sha256=digest(out/(target.name+'-RELEASE.json')))
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
