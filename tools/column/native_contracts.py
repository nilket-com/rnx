"""Execute every native-manifest case, including ignored contracts, without a second list."""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import time
import traceback

import child_result as child
from owned_build import OwnedBuild, run_process

MANIFEST = 'rune/tools/xtask/manifests/native.json'


def manifest(root):
	m = json.loads((Path(root)/MANIFEST).read_text())
	cases = m['cases']
	if m['version'] != 1 or not cases: raise ValueError('empty or unknown native manifest')
	if len({c['name'] for c in cases}) != len(cases): raise ValueError('duplicate manifest case')
	if any(not c['selector'] or c['timeout'] <= 0 or Path(c['name']).name != c['name'] or c['name'] in {'.', '..', ''} for c in cases): raise ValueError('invalid manifest case')
	return m


def catalogue(m, rows):
	"""A successful outer suite cannot conceal a missing or differently selected case."""
	expected = {c['name']: c for c in m['cases']}
	actual = {r['name']: r for r in rows}
	if len(actual) != len(rows) or actual.keys() != expected.keys():
		raise ValueError('native contract catalogue differs: missing=' + repr(sorted(expected.keys()-actual.keys())) +
			' extra=' + repr(sorted(actual.keys()-expected.keys())))
	for name, spec in expected.items():
		r = actual[name]
		if r.get('specification') != child.identity(spec): raise ValueError(name + ': manifest case changed')
		if r.get('status') != 'PASS' or r.get('exit') != 0:
			raise ValueError(name + ': case failed\n' + r.get('error', 'missing successful execution'))
		if not r.get('selected') or r.get('passed') != len(r['selected']): raise ValueError(name + ': empty or partial execution')
		if spec['exact'] and r['selected'] != [spec['selector']]: raise ValueError(name + ': exact selector differs')
		if spec.get('rows') is not None and r.get('contract_rows') != spec['rows']:
			raise ValueError(name + ': contract row total differs')
	return dict(status='PASS', cases=len(rows), selectors=len({c['selector'] for c in m['cases']}),
		ignored_selectors=len({c['selector'] for c in m['cases'] if c['ignored']}))


def checked(command, cwd, out, *, env=None, timeout=1800):
	with out.open('x') as log:
		run_process(command, cwd=cwd, env=env, stdout=log, stderr=subprocess.STDOUT, timeout=timeout, check=True)
	return out.read_text()


def build(root, bulk, m):
	features = [l['features'] for l in m['libraries'] if {'execution', 'serde'} <= set(l['features'].split(',')) and not l.get('selector')]
	if len(features) != 1: raise ValueError('native contracts require one declared execution-serde library recipe')
	with OwnedBuild(bulk, 'build-target') as owner:
		from host_budget import build_jobs
		env = dict(os.environ, CARGO_TARGET_DIR=str(owner.path), CARGO_BUILD_JOBS=str(build_jobs()))
		command = ['cargo', '+1.98.1', 'test', '--locked', '-p', 'rune', '--profile', 'engine-dev',
			'--lib', '--features', features[0], '--no-run', '--message-format=json']
		text = checked(command, root, bulk/'build.log', env=env, timeout=3600)
		artifacts = [json.loads(line) for line in text.splitlines() if line.startswith('{')]
		found = [r['executable'] for r in artifacts if r.get('reason') == 'compiler-artifact' and
			r['target']['name'] == 'rune' and r['profile'].get('test') and r.get('executable')]
		if len(found) != 1: raise ValueError('ambiguous Rune test binary')
		binary = bulk/'rune-tests'; shutil.copy2(found[0], binary)
		units = compilation_units(binary, root, bulk/'dwarf.log')
		receipt = bulk/'BINARY.json'
		child.atomic_json(receipt, dict(status='PASS', expected_repo=str(root), binary_sha256=child.digest(binary),
			source=child.source(root), candidate=child.head(root), command=command, units=units,
			dwarf_sha256=child.digest(bulk/'dwarf.log')))
		owner.keep([binary, receipt])
	return binary, receipt


def compilation_units(binary, root, log):
	text = checked(['readelf', '--debug-dump=info', '--dwarf-depth=1', str(binary)], root, log, timeout=180)
	units = []
	for block in text.split('Compilation Unit @ offset'):
		names = [line for line in block.splitlines() if 'DW_AT_name' in line]
		if not names or 'crates/rune/src/lib.rs' not in names[0]: continue
		dirs = [line for line in block.splitlines() if 'DW_AT_comp_dir' in line]
		if len(dirs) != 1: raise ValueError('missing Rune compilation directory')
		cwd = Path(dirs[0].rsplit(': ', 1)[-1].strip())
		name = names[0].rsplit(': ', 1)[-1].strip()
		if cwd.resolve() != root.resolve() or (cwd/name.split('/@/')[0]).resolve() != (root/'rune/crates/rune/src/lib.rs').resolve():
			raise ValueError('Rune compilation source differs from checkout')
		units.append(dict(name=name, recorded_cwd=str(cwd)))
	if not units: raise ValueError('no Rune compilation units')
	return units


def run_case(c, binary, receipt, root, bulk):
	d = bulk/c['name']; d.mkdir()
	r = dict(name=c['name'], selector=c['selector'], specification=child.identity(c), status='FAIL')
	started = time.monotonic()
	try:
		if child.digest(binary) != json.loads(receipt.read_text())['binary_sha256']: raise ValueError('test binary changed')
		args = [str(binary), c['selector']]
		if c['exact']: args.append('--exact')
		if c['ignored']: args.append('--ignored')
		text = checked(args+['--list'], d, d/'discovery.log', timeout=60)
		r['selected'] = re.findall(r'^(.+): test$', text, re.M)
		if not r['selected'] or c['exact'] and r['selected'] != [c['selector']]: raise ValueError('selector discovery differs')
		env = dict(PATH='/usr/bin:/bin', LANG='C.UTF-8')
		# Only declared fixture inputs enter a test process, never the caller's environment.
		for key, folder in {'GAP_OUT':'gap-out', 'C10_REPLAY_OUT':'c10-out', 'C11_REPLAY_OUT':'c11-out',
			'C15B_STOCK_OUT':'c15b-out', 'C15C_STOCK_OUT':'c15c-out', 'C13_OUT':'c13-out', 'C15_OUT':'c15-out',
			'C15B_OUT':'c15b-fresh-out', 'C15C_OUT':'c15c-fresh-out', 'C12_OUT':'c12-out', 'C14_OUT':'c14-out', 'C14B_OUT':'c14b-out'}.items():
			env[key] = str(d/folder)
		env.update(c['env'])
		env.update(C15C_NATIVE_RECEIPT=str(receipt), C15C_NATIVE_RECEIPT_SHA256=child.digest(receipt),
			C15C_NATIVE_SOURCE=str(root), RNX_CONTRACT_DIFFS=str(d/'DIFFERENCES.json'))
		if os.environ.get('RNX_FINAL_GROUPS'): env['RNX_FINAL_GROUPS'] = os.environ['RNX_FINAL_GROUPS']
		command = args+['--nocapture', '--test-threads=1']; cwd = d
		if c['sandbox']:
			command = ['bwrap', '--ro-bind', '/', '/', '--bind', str(bulk), str(bulk), '--tmpfs', '/tmp',
				'--dev-bind', '/dev', '/dev', '--proc', '/proc', '--', *command]
			cwd = '/tmp'
		with (d/'stdout.log').open('x') as out, (d/'stderr.log').open('x') as err:
			done = run_process(command, cwd=cwd, env=env, stdout=out, stderr=err, timeout=c['timeout'])
		r['exit'] = done.returncode
		text = (d/'stdout.log').read_text()
		counts = re.findall(r'^test result: (?:ok|FAILED)\. (\d+) passed; (\d+) failed;', text, re.M)
		if len(counts) != 1 or tuple(map(int, counts[0])) != (len(r['selected']), 0) or done.returncode:
			raise ValueError('native contract execution failed: ' + c['name'])
		r['passed'] = int(counts[0][0])
		markers = re.findall(r'^CONTRACT ROWS (\d+)$', text, re.M)
		if c.get('rows') is not None:
			if markers != [str(c['rows'])]: raise ValueError('contract row total differs')
			r['contract_rows'] = int(markers[0])
		if c['empty_stderr'] and (d/'stderr.log').stat().st_size: raise ValueError('nonempty contract stderr')
		if (d/'DIFFERENCES.json').exists():
			differences = json.loads((d/'DIFFERENCES.json').read_text())
			if differences['differences']: raise ValueError('nonempty row differences')
			r['rows_checked'] = differences['rows_checked']
			if c.get('rows') is not None and r['rows_checked'] != c['rows']: raise ValueError('compared row total differs')
		r['status'] = 'PASS'
	except Exception:
		r['error'] = traceback.format_exc()
		if (d/'DIFFERENCES.json').is_file():
			try:
				differences = json.loads((d/'DIFFERENCES.json').read_text())['differences']
				r['row_differences'] = [{k: row[k] for k in ('row', 'difference')} for row in differences]
				r['error'] += '\n' + json.dumps(r['row_differences'], ensure_ascii=False)
			except (ValueError, KeyError, TypeError): pass  # Original failure remains the verdict.
	finally:
		r['seconds'] = time.monotonic()-started
		r['artifacts'] = {p.name: child.digest(p) for p in d.iterdir() if p.is_file()}
		child.atomic_json(d/'RESULT.json', r)
	return r


def run_cases(cases, binary, receipt, root, bulk):
	"""Run every case: each exclusive one alone, then the rest concurrently.

	Each concurrent case runs in a worker process, whose main thread owns the
	supervised wait and its signal handlers. Every case runs whatever another
	returns, so the catalogue reports every failure, as a serial run did."""
	import multiprocessing
	from concurrent.futures import ProcessPoolExecutor
	from host_budget import case_width
	from stage_scheduler import Client
	rows = {}
	client = Client.from_environment()
	for c in cases:
		# Under a final's jobserver an exclusive case holds every token: this
		# stage's own idle slot, and the rest taken here, so no build or case runs with it.
		if c.get('exclusive'): rows[c['name']] = with_tokens((client.capacity-client.held) if client else 0, c, binary, receipt, root, bulk)
	held = client.held if client else 1
	if client: client.close()
	shared = [c for c in cases if not c.get('exclusive')]
	if shared:
		width = min(case_width(), len(shared))
		# The stage's own slots, idle now that its builds are done, as a pipe of tokens.
		own = os.pipe(); os.write(own[1], b'+'*held); os.set_blocking(own[0], False)
		try:
			with ProcessPoolExecutor(max_workers=width, mp_context=multiprocessing.get_context('fork')) as pool:
				futures = {c['name']: pool.submit(with_slot, own, c, binary, receipt, root, bulk) for c in shared}
				for name, future in futures.items(): rows[name] = future.result()
		finally:
			for fd in own: os.close(fd)
	return [rows[c['name']] for c in cases]


def with_tokens(count, c, binary, receipt, root, bulk):
	"""Run one case holding count tokens of the final's jobserver, if there is one."""
	from stage_scheduler import Client
	client = Client.from_environment()
	if client is None or count == 0:
		if client: client.close()
		return run_case(c, binary, receipt, root, bulk)
	tokens = client.acquire(count)
	try: return run_case(c, binary, receipt, root, bulk)
	finally:
		client.release(tokens); client.close()


def with_slot(own, c, binary, receipt, root, bulk):
	"""Run one shared case on the stage's own slot or one pool token."""
	from stage_scheduler import Client
	client = Client.from_environment()
	if client is None: return run_case(c, binary, receipt, root, bulk)
	fd, token = client.acquire_one(own)
	try: return run_case(c, binary, receipt, root, bulk)
	finally:
		os.write(own[1] if fd == own[0] else client.fd, token); client.close()


def run(root, out):
	from final_evidence import assigned_root
	from ir_resources import resources
	root = Path(root).resolve()
	bulk = assigned_root('RNX_FINAL_CONTRACT_ROOT', out/'contract-payload')
	bulk.mkdir(exist_ok=True)
	m = manifest(root); source = child.source(root); started = time.monotonic()
	from host_budget import inherited_width
	# Inside a final the jobserver's tokens bound memory; a live reading would
	# count the neighbors' builds that those tokens already admit.
	available = resources()
	if inherited_width() is None and min(available['mem_available_bytes'], available.get('ancestor_headroom_bytes') or 2**63) < 4*1024**3:
		raise ValueError('native contract admission needs 4 GiB memory headroom')
	binary, receipt = build(root, bulk, m)
	rows = run_cases(m['cases'], binary, receipt, root, bulk)
	if child.source(root) != source: raise ValueError('source changed during native contracts')
	r = dict(status='FAIL', manifest_sha256=child.digest(root/MANIFEST), source=source,
		binary_sha256=child.digest(binary), cases=rows, seconds=time.monotonic()-started)
	try:
		r['catalogue'] = catalogue(m, rows); r['status'] = 'PASS'
	except ValueError as e: r['error'] = str(e)
	child.atomic_json(out/'CONTRACTS.json', r)
	return r
