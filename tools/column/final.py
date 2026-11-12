"""Canonical final: live validation, durable acceptance, then named-root release."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import signal
import shutil
import subprocess
import sys
import tempfile
import time
import traceback

import child_result as child
import gate_policy as gate
from final_evidence import Owners, compact, recover, revalidate, abandon_diagnostics
from stage_outputs import finish_stage

ROOT = Path(__file__).resolve().parents[2]


def redact(text, environment):
	# Streams may echo any inherited variable, not just the declared recipe set.
	values = sorted({str(v) for v in environment.values() if v}, key=len, reverse=True)
	for value in values:
		text = text.replace(value, '<redacted>')
	return text


def diagnostics(out, entries, error, environment):
	budget = 4*1024*1024
	files = []
	stage_budgets = {}
	for entry in entries:
		stage = entry['stage']
		stage_budget = stage_budgets.setdefault(stage, 256*1024)
		for p in sorted(Path(entry['path']).rglob('*')):
			if not p.is_file() or p.is_symlink() or p.suffix not in {'.log','.stderr','.stdout'}:
				continue
			if min(stage_budget, budget) < 1024:
				break
			size = p.stat().st_size
			take = min(stage_budget, budget, 32768)
			with p.open('rb') as stream:
				data = stream.read(take//2)
				if size > take//2:
					stream.seek(max(take//2, size-take//2))
					data += stream.read(take//2)
			row = dict(stage=stage, name=redact(str(p),environment), bytes=size,
				sha256=child.digest(p), truncated=size>len(data),
				text=redact(data.decode('utf-8','replace'),environment))
			cost = len(json.dumps(row).encode())
			if cost > min(stage_budget, budget):
				continue
			files.append(row)
			stage_budget -= cost
			budget -= cost
		stage_budgets[stage] = stage_budget
		if budget < 1024:
			break
	r = dict(error=redact(str(error)[:16384],environment), files=files,
		release_point='explicit abandon; no automatic retention sweep')
	while len(json.dumps(r).encode()) > 4*1024*1024:
		r['files'].pop()
	child.atomic_json(out/'DIAGNOSTICS.json',r)


def tools_identity():
	return dict(
		rustc=subprocess.check_output(['rustc','+1.98.1','-vV'],text=True),
		driver=child.digest(__file__),
		owner=child.digest(Path(__file__).with_name('final_evidence.py')),
		executables={name:child.digest(Path(shutil.which(name)).resolve())
			for name in ('cargo','rustc','git','python3','bwrap') if shutil.which(name)})


def disk_check(root, out):
	with (out/'disk-admission.json').open('x') as log:
		subprocess.run([sys.executable,'-B',str(root/'ci/disk_space.py'),
			'--path',str(root),'--path',str(out)], stdout=log, check=True)


def fresh_violation(root, fresh):
	if os.environ.get('CARGO_TARGET_DIR'):
		return 'final refuses CARGO_TARGET_DIR override'
	if fresh:
		from accept import MANIFESTS
		for manifest in MANIFESTS:
			if (root/manifest).parent.joinpath('target').exists():
				return 'fresh acceptance refuses preexisting native target: '+manifest
	return None


def run_stages(root, out, selection, owner, environment, evidence, rows):
	for name, command in selection['stages']:
		gate.check_selection(selection,root,final=True)
		env = dict(environment)
		env['RNX_FINAL_ROUTING_ROOT'] = str(owner.assign(root/'target/routing-cli',name))
		if name == 'generated-projects':
			env['RNX_FINAL_PROJECT_ROOT'] = str(owner.assign(Path(tempfile.gettempdir()),name))
		if name == 'cross':
			parent = Path(command[command.index('--out')+1]).parent
			cross = owner.assign(root/'target/rnx-verify',name,name=parent.name)
			if cross != parent:
				raise ValueError('cross assignment differs from selection')
			env['RNX_FINAL_CROSS_ROOT'] = str(cross)
		with (out/(name+'.log')).open('x') as log:
			done = child.run(command,cwd=root,root=root,env=env,stdout=log,stderr=subprocess.STDOUT,
				timeout=10800,directory=out/(name+'-child'),after=lambda:finish_stage(root,name,out))
		row = dict(name=name,exit=done.returncode,validation=done.validation)
		if getattr(done,'declared_output',None) is not None:
			row['declared_output'] = done.declared_output
		rows.append(row)
		evidence[name] = done.validation
		# Repeat the clean-tree and pinned HEAD/selection check after EVERY stage.
		gate.check_selection(selection,root,final=True)
		if done.returncode:
			raise RuntimeError('stage failed: '+name)


def terminal(out, owner, selection, evidence, tools, acceptance, environment):
	"""Snapshot errors cannot prevent release after failed acceptance."""
	release = dict(status='FAIL')
	manifest = dict(schema=1,candidate=acceptance['candidate'],roots=[])
	try:
		if owner:
			from owned_build import drain_recorded_groups
			drain_recorded_groups(owner.groups)
			try:
				if selection:
					manifest = compact(selection,evidence,owner,tools)
			except Exception:
				acceptance['manifest_error'] = traceback.format_exc()
				if acceptance['status'] == 'PASS':
					raise  # Passing acceptance may not release without its full manifest.
				manifest.update(incomplete=True,roots=[dict(owner=e) for e in owner.row['roots']])
			if acceptance['status'] != 'PASS':
				try:
					diagnostics(out,owner.row['roots'],acceptance.get('error'),environment)
				except Exception:
					acceptance['diagnostics_error'] = traceback.format_exc()
		child.atomic_json(out/'ACCEPTANCE.json',acceptance)
		child.atomic_json(out/'MANIFEST.json',manifest)
		if owner:
			release = owner.release()
			release.update(acceptance_sha256=child.digest(out/'ACCEPTANCE.json'),
				manifest_sha256=child.digest(out/'MANIFEST.json'))
	except BaseException:
		release.update(status='FAIL',error=traceback.format_exc())
		# Persist the original outcome even when passing-manifest construction fails.
		if not (out/'ACCEPTANCE.json').exists():
			child.atomic_json(out/'ACCEPTANCE.json',acceptance)
	child.atomic_json(out/'RELEASE.json',release)
	return release


def run(root, out, names=None, remote_context=None, fresh=False):
	root = Path(root).resolve()
	out = Path(out).absolute()
	# Inspect fresh targets before this invocation creates registry/output directories.
	violation = fresh_violation(root,fresh)
	out.mkdir(parents=True,exist_ok=False)
	registry = root/'target/final-owners'
	registry.mkdir(parents=True,exist_ok=True)
	started = time.monotonic()
	free_before = shutil.disk_usage(root).free
	selection = owner = None
	evidence, tools, rows = {}, {}, []
	environment = dict(os.environ,CARGO_BUILD_JOBS='4',PYTHONDONTWRITEBYTECODE='1',RNX_FINAL='1')
	acceptance = dict(status='FAIL',candidate=child.head(root),
		acceptance_scope='complete-final' if names is None else 'selected-stages',
		mode='fresh-clone' if fresh else 'ordinary')
	lock = (registry/'DRIVER.lock').open('a+')
	try:
		fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
		recovered = recover(registry)
		owner = Owners(registry,out)
		environment['RNX_FINAL_GROUPS'] = str(owner.groups)
		if violation:
			raise ValueError(violation)
		disk_check(root,out)
		selection = gate.select(root,out,names=names)
		gate.check_selection(selection,root,final=True)
		child.atomic_json(out/'SELECTION.json',selection)
		tools = tools_identity()
		run_stages(root,out,selection,owner,environment,evidence,rows)
		acceptance.update(status='PASS',finalization=gate.finalize(selection,evidence,root),recovered=recovered)
	except BaseException:
		acceptance['error'] = traceback.format_exc()
	finally:
		# No second ordinary signal may interrupt the terminal drain/release sequence.
		previous = {sig:signal.signal(sig,signal.SIG_IGN) for sig in (signal.SIGTERM,signal.SIGINT)}
		try:
			acceptance.update(checks=rows,seconds=time.monotonic()-started)
			release = terminal(out,owner,selection,evidence,tools,acceptance,environment)
			r = dict(status='PASS' if acceptance['status']==release['status']=='PASS' else 'FAIL',
				candidate=acceptance['candidate'],seconds=time.monotonic()-started,
				checks=[{'name':row['name'],'exit':row['exit']} for row in rows],
				error=acceptance.get('error'),release_error=release.get('error'),
				manifest_error=acceptance.get('manifest_error'),diagnostics_error=acceptance.get('diagnostics_error'),
				finalization=acceptance.get('finalization'),remote_context=remote_context,
				acceptance_scope=acceptance['acceptance_scope'],complete_final=names is None and acceptance['status']==release['status']=='PASS',
				free_bytes_before=free_before,free_bytes_after=shutil.disk_usage(root).free,
				records={n:child.digest(out/(n+'.json')) for n in ('ACCEPTANCE','MANIFEST','RELEASE') if (out/(n+'.json')).exists()})
			if owner and release['status']=='PASS':
				try: owner.retire()
				except Exception: r.update(status='FAIL',complete_final=False,retirement_error=traceback.format_exc())
			child.publish(out/'RESULT.json',r)
		finally:
			if owner:
				owner.close()
			lock.close()
			for sig, handler in previous.items():
				signal.signal(sig,handler)
	return r


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out',type=Path,required=True)
	p.add_argument('--stages',help='explicit selected-stage check, not a complete final')
	p.add_argument('--revalidate',action='store_true')
	p.add_argument('--abandon-diagnostics',action='store_true')
	a = p.parse_args()
	if a.abandon_diagnostics:
		abandon_diagnostics(a.out)
		r = {'status':'PASS'}
	elif a.revalidate:
		r = revalidate(a.out)
	else:
		def stop(signum,frame):
			raise RuntimeError('final interrupted by signal '+str(signum))
		for sig in (signal.SIGTERM,signal.SIGINT):
			signal.signal(sig,stop)
		r = run(ROOT,a.out,a.stages.split(',') if a.stages else None)
	print(r['status'],r.get('acceptance_scope',''),a.out/'RESULT.json',flush=True)
	return r['status']!='PASS'


if __name__=='__main__':
	raise SystemExit(main())
