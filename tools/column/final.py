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
import stage_scheduler as scheduler
import gate_policy as gate
from final_evidence import Owners, compact, recover, revalidate, abandon_diagnostics
from stage_outputs import finish_stage
from failure_detail import copy_bundle, retain, value as failure_value, reason, sanitize, bounded, log_excerpt, ERROR_CAP, FINAL_CAP

ROOT = Path(__file__).resolve().parents[2]


# General diagnostics and focused failure bundles have one value/dump rule.
redact = sanitize


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
			data, truncated = log_excerpt(p, take)
			row = dict(stage=stage, name=redact(str(p),environment), bytes=size,
				sha256=child.digest(p), truncated=truncated,
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


class StageFailed(RuntimeError):
	"""The first failing stage: the final's cause, whatever ran beside it."""
	def __init__(self, message, row):
		super().__init__(message); self.row = row


# Tests replace this with stage_scheduler.Inline to run stages in-process.
STAGE_RUNNER = scheduler.Forked
DISK_FLOOR_BYTES = 20*1024**3


def stage_environment(root, out, name, command, owner, environment, pool, held=1):
	"""Owned roots and the environment of one stage, fixed before it starts."""
	env = dict(environment)
	env['RNX_FINAL_ROUTING_ROOT'] = str(owner.assign(root/'target/routing-cli',name))
	from accept import BULK_ROOTS
	for kind in BULK_ROOTS.get(name, ()):
		if kind == 'baseline':
			env['RNX_FINAL_BASELINE_ROOT'] = str(owner.assign(out,name+'-baseline'))
		elif kind == 'fixed':
			env['RNX_FINAL_FIXED_ROOT'] = str(owner.assign(out,name+'-payload'))
		elif kind == 'project':
			env['RNX_FINAL_PROJECT_ROOT'] = str(owner.assign(Path(tempfile.gettempdir()),name))
		elif kind == 'cross':
			parent = Path(command[command.index('--out')+1]).parent
			cross = owner.assign(root/'target/rnx-verify',name,name=parent.name)
			if cross != parent: raise ValueError('cross assignment differs from selection')
			env['RNX_FINAL_CROSS_ROOT'] = str(cross)
		elif kind == 'contracts':
			env['RNX_FINAL_CONTRACT_ROOT'] = str(owner.assign(out,name+'-contracts'))
		else: raise ValueError('unknown bulk root kind: '+kind)
	if name == 'polars-suite':
		from stage_outputs import owned_report
		env['RNX_ORACLE_RESULTS'] = str(owned_report(out))
	# The fixed-route screen keeps its pinned recipe outside the shared pool.
	if pool: env.update(scheduler.stage_environment(name, command, pool, held))
	return env


def stage_failure(root, out, name, done, owner, environment, row):
	"""Copy a failed stage's cause while invocation-owned bulk still exists."""
	failure = None
	stage_error = done.get('error') or 'child failed'
	roots = [out]
	try:
		roots = [out] + [Path(entry['path']) for entry in owner.row['roots']]
		validation = failure_value(done['validation']['path'], roots)
		producer = Path(done['validation']['path']).parent/'CHILD.json'
		semantic = failure_value(producer, roots)['receipt'] if producer.is_file() else done['validation']['path']
		cause = failure_value(semantic, roots)
		stage_error = reason(cause)
		if validation.get('failure') or cause.get('failure'):
			failure = copy_bundle(out, validation.get('failure') or cause['failure'], roots=roots, environment=environment)
		else:
			failure = retain(out, name, receipt=semantic, logs=[out/(name+'.log')], roots=roots,
				environment=environment, error=cause.get('error', validation.get('error', 'child failed')))
		if sum(p.stat().st_size for p in (out/'failure').iterdir()) + (out/'FAILURE.json').stat().st_size > FINAL_CAP:
			raise ValueError('final failure diagnostics exceed bound')
	except Exception as problem:
		# Diagnostics cannot replace the original failure or prevent release.
		row['failure_retention_error'] = type(problem).__name__
		if not (out/'failure').exists() and done.get('validation'):
			try:
				failure = retain(out, name, receipt=done['validation']['path'], logs=[out/(name+'.log')],
					roots=roots, environment=environment, error=stage_error)
			except Exception: pass  # The original stage error remains the verdict.
	if failure: row['failure'] = failure
	return 'stage failed: '+name + '; ' + (failure['error'] if failure else bounded(sanitize(str(stage_error), environment), ERROR_CAP)[0])


def run_stages(root, out, selection, owner, environment, evidence, rows, disk=None):
	"""Run the selection concurrently; rows come back in selection order."""
	stages = selection['stages']
	targets = scheduler.Targets(root)
	plan = [scheduler.resources(name, command, root, targets) for name, command in stages]
	needs = scheduler.producers(stages)
	bulk = [scheduler.declared_bulk(name) for name, _ in stages]
	tokens = int(environment.get('CARGO_BUILD_JOBS') or 1)
	weights = [scheduler.weight(name, command, root) for name, command in stages]
	# A stage runs its full test width; it is refused, not run on fewer tokens.
	wide = [(name, w) for (name, _), w in zip(stages, weights) if w > tokens]
	if wide: raise ValueError(f'budget of {tokens} tokens is below the test width of ' + ', '.join(f'{n} ({w})' for n, w in wide))
	growth = {r: scheduler.growth(r, root) for resources in plan for r in resources}
	def declared(indices):
		# Owned bulk per stage, and each target directory's growth once.
		return sum(bulk[i] for i in indices) + sum(growth[r] for r in set().union(*(plan[i] for i in indices), set()))
	pool = scheduler.Pool(out/'jobserver.fifo', tokens)
	disk = disk if disk is not None else {}
	disk.update(floor_bytes=DISK_FLOOR_BYTES, estimate_bytes=DISK_FLOOR_BYTES+declared(range(len(stages))),
		binding=str(root), lowest_free_bytes=None)
	pending, running, finished, succeeded, done_rows = list(range(len(stages))), {}, set(), set(), {}
	failed = None
	def complete(index):
		nonlocal failed
		handle, token = running.pop(index)
		done = handle.result()
		pool.give(token)
		finished.add(index)
		name = stages[index][0]
		row = dict(name=name, exit=done['returncode'], validation=done.get('validation'))
		if done.get('declared_output') is not None:
			row['declared_output'] = done['declared_output']
		done_rows[index] = row
		if done.get('validation'): evidence[name] = done['validation']
		if not done['returncode']: succeeded.add(index)
		if done['returncode'] and failed is None:
			failed = StageFailed(stage_failure(root, out, name, done, owner, environment, row), row)
	def harvest():
		for index in [i for i, (h, _) in running.items() if h.ready()]:
			complete(index)
			# Repeat the clean-tree and pinned HEAD/selection check after EVERY stage.
			gate.check_selection(selection,root,final=True)
	try:
		while pending or running:
			if failed is None:
				for index in list(pending):
					# A stage that already failed stops admission before the next start.
					harvest()
					if failed is not None: break
					if not scheduler.ready(index, plan, finished, running, needs[index], succeeded): continue
					free = shutil.disk_usage(root).free
					if disk['lowest_free_bytes'] is None or free < disk['lowest_free_bytes']: disk['lowest_free_bytes'] = free
					needed = DISK_FLOOR_BYTES + declared(list(pending) + list(running))
					if free < needed:
						if running: break
						raise ValueError(f'disk admission before {stages[index][0]}: {free} bytes free, {needed} needed')
					token = pool.take(weights[index])
					if token is None: break
					name, command = stages[index]
					gate.check_selection(selection,root,final=True)
					env = stage_environment(root, out, name, command, owner, environment, pool, weights[index])
					def execute(name=name, command=command, env=env):
						with (out/(name+'.log')).open('x') as log:
							return child.run(command,cwd=root,root=root,env=env,stdout=log,stderr=subprocess.STDOUT,
								timeout=10800,directory=out/(name+'-child'),after=lambda:finish_stage(root,name,out))
					pending.remove(index)
					try: running[index] = (STAGE_RUNNER(execute), token)
					except BaseException:
						pool.give(token); raise
					if scheduler.ALONE in plan[index]: break
			if not running:
				if failed is not None or not pending: break
				# Nothing runs and nothing could start: no later pass would differ.
				raise ValueError('no selected stage can start: ' + stages[pending[0]][0])
			scheduler.wait([h for h, _ in running.values()], timeout=1.0)
			harvest()
	except BaseException:
		# A signal or controller error: stop every running stage, keep their receipts.
		previous = {sig:signal.signal(sig,signal.SIG_IGN) for sig in (signal.SIGTERM,signal.SIGINT)}
		try:
			for handle, _ in running.values(): handle.terminate()
			for index in list(running): complete(index)
		finally:
			for sig, handler in previous.items(): signal.signal(sig, handler)
			rows.extend(done_rows[i] for i in sorted(done_rows))
			pool.close()
		raise
	rows.extend(done_rows[i] for i in sorted(done_rows))
	pool.close()
	if failed is not None: raise failed


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
	if release['status']=='PASS' and (out/'fixed-route/RESULT.json').is_file():
		child.atomic_json(out/'fixed-route/PAYLOAD-RELEASE.json',dict(status='PASS',
			payload='released; artifact paths in measurement receipts are historical',
			manifest_sha256=child.digest(out/'MANIFEST.json'),release_sha256=child.digest(out/'RELEASE.json')))
	return release


# People wait on the complete final. Past an hour it fails, so a slow stage is
# fixed when it appears rather than after the loop has doubled.
WALL_BUDGET_SECONDS = 3600


def wall_budget_error(scope, seconds):
	if scope == 'complete' and seconds > WALL_BUDGET_SECONDS:
		return f'complete final took {seconds/60:.1f} minutes; its budget is {WALL_BUDGET_SECONDS//60}'
	return None


def run(root, out, names=None, remote_context=None, fresh=False, scope=None):
	scope = scope or ('explicit' if names is not None else 'required')
	gate.publication_eligible(scope)
	root = Path(root).resolve()
	out = Path(out).absolute()
	# Inspect fresh targets before this invocation creates registry/output directories.
	violation = fresh_violation(root,fresh)
	out.mkdir(parents=True,exist_ok=False)
	registry = root/'target/final-owners'
	registry.mkdir(parents=True,exist_ok=True)
	started = time.monotonic()
	free_before = shutil.disk_usage(root).free
	selection = owner = failed_row = None
	evidence, tools, rows, disk = {}, {}, [], {}
	# Stages run beside each other, and some fingerprint the checkout's .git: no
	# git command of the controller or a stage may refresh the index while they do.
	optional_locks = os.environ.get('GIT_OPTIONAL_LOCKS')
	os.environ['GIT_OPTIONAL_LOCKS'] = '0'
	from host_budget import final_jobs, memory_events
	events_before = memory_events()
	environment = dict(os.environ,CARGO_BUILD_JOBS=str(final_jobs()),PYTHONDONTWRITEBYTECODE='1',RNX_FINAL='1')
	acceptance = dict(status='FAIL',candidate=child.head(root),
		acceptance_scope=scope,
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
		selection = gate.select(root,out,names=names,scope=scope)
		gate.check_selection(selection,root,final=True)
		child.atomic_json(out/'SELECTION.json',selection)
		tools = tools_identity()
		run_stages(root,out,selection,owner,environment,evidence,rows,disk)
		over = wall_budget_error(scope,time.monotonic()-started)
		if over: raise ValueError(over)
		acceptance.update(status='PASS',finalization=gate.finalize(selection,evidence,root),recovered=recovered)
	except BaseException as error:
		failed_row = getattr(error,'row',None)
		acceptance['error'] = failed_row['failure']['error'] if failed_row and failed_row.get('failure') else traceback.format_exc()
	finally:
		# No second ordinary signal may interrupt the terminal drain/release sequence.
		previous = {sig:signal.signal(sig,signal.SIG_IGN) for sig in (signal.SIGTERM,signal.SIGINT)}
		try:
			acceptance.update(checks=rows,seconds=time.monotonic()-started)
			if disk: acceptance['disk'] = disk
			# Throttling at the control group's high limit is evidence about a failed deadline.
			acceptance['memory'] = dict(tokens=environment.get('CARGO_BUILD_JOBS'), events_before=events_before, events_after=memory_events())
			if failed_row:
				acceptance['failed_stage'] = failed_row['name']
				if failed_row.get('failure'): acceptance['failure'] = failed_row['failure']
			release = terminal(out,owner,selection,evidence,tools,acceptance,environment)
			r = dict(status='PASS' if acceptance['status']==release['status']=='PASS' else 'FAIL',
				candidate=acceptance['candidate'],seconds=time.monotonic()-started,
				checks=[{'name':row['name'],'exit':row['exit']} for row in rows],
				error=acceptance.get('error'),release_error=release.get('error'),
				manifest_error=acceptance.get('manifest_error'),diagnostics_error=acceptance.get('diagnostics_error'),
				finalization=acceptance.get('finalization'),remote_context=remote_context,
				acceptance_scope=acceptance['acceptance_scope'],complete_final=scope=='complete' and acceptance['status']==release['status']=='PASS',
				publication_eligible=gate.publication_eligible(scope) and acceptance['status']==release['status']=='PASS',
				free_bytes_before=free_before,free_bytes_after=shutil.disk_usage(root).free,
				records={n:child.digest(out/(n+'.json')) for n in ('ACCEPTANCE','MANIFEST','RELEASE') if (out/(n+'.json')).exists()})
			if failed_row:
				r['failed_stage'] = failed_row['name']
				if failed_row.get('failure'): r['failure'] = failed_row['failure']
			if owner and release['status']=='PASS':
				try: owner.retire()
				except Exception: r.update(status='FAIL',complete_final=False,publication_eligible=False,retirement_error=traceback.format_exc())
			# The budget covers the whole run: stages, finalization and release.
			r['seconds'] = time.monotonic()-started
			over = wall_budget_error(scope,r['seconds'])
			if over:
				r.update(status='FAIL',complete_final=False,publication_eligible=False,wall_budget_error=over)
				# A stage failure stays the reported cause; otherwise the budget is.
				r['error'] = r.get('error') or over
			child.publish(out/'RESULT.json',r)
		finally:
			if owner:
				owner.close()
			lock.close()
			for sig, handler in previous.items():
				signal.signal(sig,handler)
			if optional_locks is None: os.environ.pop('GIT_OPTIONAL_LOCKS', None)
			else: os.environ['GIT_OPTIONAL_LOCKS'] = optional_locks
	return r


READS_PLANS = gate.READS_PLANS


def docs_change(root, prior_candidate, candidate):
	"""The paths an amend changed, when it changed nothing a stage reads but the plans."""
	git = lambda *a: subprocess.check_output(['git', '-c', 'color.ui=false', *a], cwd=root)
	if git('rev-parse', prior_candidate + '^') != git('rev-parse', candidate + '^'):
		raise ValueError('docs successor must amend the passed commit onto the same parent')
	# Without rename folding, a file moved into plans/ also lists its source.
	changed = [p.decode() for p in git('diff', '--no-renames', '--name-only', '-z', prior_candidate, candidate).split(b'\0') if p]
	if not changed or any(not p.startswith('plans/') for p in changed):
		raise ValueError('docs successor allows changes under plans/ only: ' + repr(changed))
	return changed


def successor(root, out, prior):
	"""Extend a passed complete final to a docs-only amend of its commit."""
	prior = Path(prior).resolve()
	earlier = revalidate(prior)
	if not earlier['complete_final'] or not earlier['publication_eligible']:
		raise ValueError('docs successor requires a passed complete final')
	candidate = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip()
	if subprocess.check_output(['git', 'status', '--porcelain'], cwd=root): raise ValueError('docs successor requires a clean tree')
	changed = docs_change(root, earlier['candidate'], candidate)
	out.mkdir(parents=True, exist_ok=False)
	docs = run(root, out/'plans-final', scope='docs')
	r = dict(status='FAIL', candidate=candidate, prior=str(prior), prior_candidate=earlier['candidate'],
		prior_result_sha256=child.digest(prior/'RESULT.json'), changed=changed, reran=list(READS_PLANS),
		plans_final=str(out/'plans-final'))
	if docs['status'] == 'PASS':
		r['plans_final_result_sha256'] = child.digest(out/'plans-final/RESULT.json'); r['status'] = 'PASS'
	child.atomic_json(out/'SUCCESSOR.json', r)
	return dict(r, acceptance_scope='complete' if r['status'] == 'PASS' else None)


def revalidate_successor(out, root=ROOT):
	r = json.loads((out/'SUCCESSOR.json').read_text())
	if r['status'] != 'PASS': raise ValueError('docs successor did not pass')
	prior = Path(r['prior'])
	if child.digest(prior/'RESULT.json') != r['prior_result_sha256']: raise ValueError('prior final changed')
	earlier = revalidate(prior)
	if earlier['candidate'] != r['prior_candidate'] or not earlier['complete_final']: raise ValueError('prior final differs')
	if child.digest(Path(r['plans_final'])/'RESULT.json') != r['plans_final_result_sha256']: raise ValueError('plans final changed')
	plans = revalidate(Path(r['plans_final']))
	if plans['candidate'] != r['candidate']: raise ValueError('plans final is for another commit')
	if plans['acceptance_scope'] != 'docs': raise ValueError('plans final ran another selection')
	if docs_change(root, r['prior_candidate'], r['candidate']) != r['changed']: raise ValueError('docs change differs')
	return dict(status='PASS', candidate=r['candidate'], acceptance_scope='complete', complete_final=True,
		publication_eligible=True, extends=r['prior'], changed=r['changed'])


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out',type=Path,required=True)
	modes=p.add_mutually_exclusive_group()
	modes.add_argument('--required',action='store_true',help='exactly policy-required stages (default)')
	modes.add_argument('--complete',action='store_true',help='all catalogue stages')
	modes.add_argument('--stages',help='explicit selection; never publication eligible')
	p.add_argument('--revalidate',action='store_true')
	p.add_argument('--docs-successor',type=Path,metavar='PRIOR',help='extend a passed complete final to a docs-only amend')
	p.add_argument('--abandon-diagnostics',action='store_true')
	a = p.parse_args()
	if a.abandon_diagnostics:
		abandon_diagnostics(a.out)
		r = {'status':'PASS'}
	elif a.revalidate:
		r = revalidate_successor(a.out) if (a.out/'SUCCESSOR.json').exists() else revalidate(a.out)
	else:
		def stop(signum,frame):
			raise RuntimeError('final interrupted by signal '+str(signum))
		for sig in (signal.SIGTERM,signal.SIGINT):
			signal.signal(sig,stop)
		if a.docs_successor:
			r = successor(ROOT,a.out.resolve(),a.docs_successor)
		else: r = run(ROOT,a.out,a.stages.split(',') if a.stages is not None else None,
			scope='explicit' if a.stages is not None else ('complete' if a.complete else 'required'))
	successor_receipt = a.docs_successor or (a.revalidate and (a.out/'SUCCESSOR.json').exists())
	print(r['status'],r.get('acceptance_scope',''),a.out/('SUCCESSOR.json' if successor_receipt else 'RESULT.json'),flush=True)
	return r['status']!='PASS'


if __name__=='__main__':
	raise SystemExit(main())
