"""Serial resumable column comparison; validation must precede timed matrix."""
import argparse
from concurrent.futures import ThreadPoolExecutor
import importlib.metadata
import itertools
import json
import os
from pathlib import Path
import platform
import resource
import subprocess
import sys
import time
import traceback
from durable import atomic, digest, lock, reuse
from fixtures import SPECS, NATIVE, LAYOUTS, SOURCE

HERE=Path(__file__).resolve().parent


def snapshot():
	def read(p):
		try: return p.read_text().strip()
		except OSError: return None
	return dict(load=os.getloadavg(),thermal={str(p):read(p) for p in Path('/sys/class/thermal').glob('thermal_zone*/temp')},
		frequency={str(p):read(p) for p in Path('/sys/devices/system/cpu').glob('cpu*/cpufreq/scaling_cur_freq')},time_ns=time.time_ns())


def cells(rows, threads, kinds, layouts):
	for kind,layout in itertools.product(kinds,layouts):
		for engine in ('rnx','polars','arrow','duckdb'):
			if threads=='default' and engine in ('rnx','arrow'): continue
			for panel in (('equivalent','native') if engine in ('polars','duckdb') and kind in NATIVE else ('equivalent',)):
				yield dict(kind=kind,layout=layout,rows=rows,threads=threads,engine=engine,panel=panel)


# Reviewed secondary scope; primary retains both sizes and five process samples.
POLICY = {'primary_samples': 5, 'secondary_samples': 3, 'secondary_rows': [100000000]}


def selected(cell, replicate):
	return cell['panel']=='equivalent' or (cell['rows'] in POLICY['secondary_rows'] and replicate<POLICY['secondary_samples'])


def validation_identity_matches(old, new):
	"""Reviewed policy-only reuse: workers, expressions, fixtures and oracle exact."""
	before=dict(old); after=dict(new)
	before_sources=before.pop('sources'); after_sources=after.pop('sources')
	if set(before_sources)!=set(after_sources): return False
	allowed={'matrix.py','render.py','test_compare.py'}
	if any(before_sources[k]!=after_sources[k] for k in before_sources if k not in allowed): return False
	before.pop('candidate_commit',None); after.pop('candidate_commit',None)
	before.pop('policy',None); after.pop('policy',None)
	return before==after


def signed_source(args):
	root=HERE.parents[2]
	commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=root,text=True).strip()
	subprocess.run(['git','verify-commit',commit],cwd=root,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
	assert subprocess.check_output(['git','show','-s','--format=%s'],cwd=root,text=True).startswith('impl: adr 0224 ')
	assert not subprocess.check_output(['git','status','--porcelain','--untracked-files=no'],cwd=root), 'tracked tree changed'
	build=json.loads(args.build_receipt.read_text()); assert build['status']=='PASS'
	assert digest(args.rnx)==build['sha256']
	for name,sha in build['source_hashes'].items(): assert digest(root/name)==sha
	assert not subprocess.check_output(['git','diff',build['column_source_baseline'],'--',*build['column_paths']],cwd=root)
	# Every product source change since the measured baseline must be in the clock receipt.
	changed=subprocess.check_output(['git','diff','--name-only',build['column_source_baseline'],'--','src','adapters','rune/crates','Cargo.toml','Cargo.lock'],cwd=root,text=True).splitlines()
	assert set(changed)<=set(build['source_hashes']), changed
	return commit


def identity(args):
	return dict(schema=1,rnx_sha256=digest(args.rnx),python=sys.version,
		install_receipt_sha256=digest(args.install_receipt),
		packages={n:importlib.metadata.version(n) for n in ('polars','polars-runtime-32','pyarrow','duckdb')},
		host_class='Intel i7-14700 measurement host',kernel=platform.release(),
		sources={p.name:digest(p) for p in HERE.glob('*.py')},fixture_source=digest(SOURCE),
		rows=args.rows,kinds=args.kinds,layouts=args.layouts,machine=platform.machine(),
		cooldown=120,samples=5,calls=6,rnx_fuel=100000000000,thread_modes={'rnx':['single'],'arrow':['single'],'polars':['single','default'],'duckdb':['single','default']},
		environment={k:v for k,v in os.environ.items() if k in ('GLIBC_TUNABLES','OMP_NUM_THREADS','MKL_NUM_THREADS','ARROW_USER_SIMD_LEVEL','POLARS_MAX_THREADS')})


def execute(args, base, cell, directory, validation):
	ident=dict(base,cell=cell,validation=validation)
	old=reuse(directory,ident)
	if old: return old
	directory.mkdir(parents=True,exist_ok=True)
	# An interrupted attempt is retained; only missing terminal result may resume.
	attempt=directory/f'attempt-{len(list(directory.glob("attempt-*")))+1:04d}'
	attempt.mkdir()
	command=[sys.executable,'-B',str(HERE/'worker.py'),'--rnx',str(args.rnx),'--out',str(attempt)]
	for k,v in cell.items(): command+=['--'+k,str(v)]
	if validation: command+=['--validate']
	env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1')
	env.pop('POLARS_MAX_THREADS',None)
	if cell['threads']=='single': env['POLARS_MAX_THREADS']='1'
	before=snapshot(); status='FAIL'; error=None; started=time.monotonic_ns()
	try:
		with (attempt/'stdout.log').open('w') as f:
			done=subprocess.run(command,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=3600)
		w=json.loads((attempt/'WORKER.json').read_text())
		if done.returncode or w['status']!='PASS': raise RuntimeError('worker exit/receipt failed')
		status='PASS'
	except BaseException: error=traceback.format_exc()
	artifacts={}
	for p in attempt.iterdir():
		if p.is_file():
			with p.open('rb') as f: os.fsync(f.fileno())
			artifacts[str(p.relative_to(directory))]=digest(p)
	process_wall_ns=time.monotonic_ns()-started
	after=snapshot()
	flags=[]
	for snap in (before,after):
		if snap['load'][0]>1.0: flags.append('load-above-1')
		if any(v is not None and int(v)>=85000 for v in snap['thermal'].values()): flags.append('temperature-at-least-85C')
		if not snap['thermal']: flags.append('thermal-sensors-unavailable')
	r=dict(status=status,process_wall_ns=process_wall_ns,identity=ident,artifacts=artifacts,before=before,after=after,condition_flags=sorted(set(flags)),error=error)
	atomic(directory/'RESULT.json',r)
	if status!='PASS': raise RuntimeError(str(directory/'RESULT.json'))
	return r


def main():
	p=argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out',type=Path,required=True);p.add_argument('--rnx',type=Path,required=True)
	p.add_argument('--install-receipt',type=Path,required=True)
	p.add_argument('--build-receipt',type=Path)
	p.add_argument('--validation',type=Path);p.add_argument('--validate-only',action='store_true')
	p.add_argument('--rows',type=int,nargs='+',default=[10000000,100000000])
	p.add_argument('--kinds',nargs='+',choices=list(SPECS),default=list(SPECS))
	p.add_argument('--layouts',nargs='+',choices=LAYOUTS,default=list(LAYOUTS))
	p.add_argument('--lease',type=Path,required=True)
	p.add_argument('--validation-workers',type=int,default=1)
	a=p.parse_args();a.out=a.out.resolve();a.rnx=a.rnx.resolve();a.out.mkdir(parents=True,exist_ok=True)
	try:
		guard=lock(a.lease);base=identity(a)
		if not a.validate_only:
			assert a.build_receipt, 'verified production build receipt required'
			base.update(candidate_commit=signed_source(a),policy=POLICY)
		install=json.loads(a.install_receipt.read_text()); assert install['status']=='PASS'
		assert base['packages']=={'polars':'2.1.0','polars-runtime-32':'2.1.0','pyarrow':'26.0.1','duckdb':'2.0.0'}
		manifest_path=a.out/'MANIFEST.json'
		if manifest_path.exists():
			assert json.loads(manifest_path.read_text())==base, 'existing matrix identity differs'
		else: atomic(manifest_path,base)
		if not a.validate_only:
			assert a.validation, 'validation receipt required'
			v=json.loads(a.validation.read_text());assert v['status']=='PASS' and validation_identity_matches(v['identity'],base)
			assert v['mode']=='validation'
			expected={json.dumps(c,sort_keys=True) for n,t in itertools.product(a.rows,('single','default')) for c in cells(n,t,a.kinds,a.layouts)}
			observed=[]
			for entry in v['receipts']:
				path=Path(entry['path']); assert digest(path)==entry['sha256']
				child=json.loads(path.read_text()); assert child['status']=='PASS'
				assert reuse(path.parent,child['identity'])
				assert child['identity']==dict(v['identity'],cell=child['identity']['cell'],validation=True)
				observed.append(json.dumps(child['identity']['cell'],sort_keys=True))
			assert len(observed)==len(expected) and set(observed)==expected, 'incomplete validation coverage'
			atomic(a.out/'VALIDATION-REUSE.json',dict(status='PASS',validation_sha256=digest(a.validation),old_identity=v['identity'],new_identity=base,reason='reviewed policy-only; worker/oracle/expression/fixture hashes exact'))
			assert a.rows==[10000000,100000000] and a.kinds==list(SPECS) and a.layouts==list(LAYOUTS)
		receipts=[]
		if a.validate_only:
			available=int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:')))*1024
			per_worker=max(a.rows)*96+1024**3
			workers=min(a.validation_workers,max(1,available//per_worker),os.cpu_count() or 1)
			assert available>=per_worker and workers>=1
			atomic(a.out/'RESOURCES.json',dict(available=available,per_worker=per_worker,workers=workers))
			jobs=[]
			for n,t in itertools.product(a.rows,('single','default')):
				for cell in cells(n,t,a.kinds,a.layouts):
					key='-'.join(str(cell[k]) for k in ('rows','threads','kind','layout','engine','panel'))
					jobs.append((cell,a.out/'cases'/key))
			def validate_one(job):
				cell,d=job;execute(a,base,cell,d,True)
				return dict(path=str(d/'RESULT.json'),sha256=digest(d/'RESULT.json'))
			with ThreadPoolExecutor(max_workers=workers) as pool:
				receipts=list(pool.map(validate_one,jobs))
		else:
			for n,threads in itertools.product(a.rows,('single','default')):
				for replicate in range(5):
					round_dir=a.out/f'{n}-{threads}-{replicate}'
					jobs=[c for c in cells(n,threads,a.kinds,a.layouts) if selected(c,replicate)]
					order=['rnx','polars','arrow','duckdb']; shift=replicate%4
					order=order[shift:]+order[:shift]
					jobs.sort(key=lambda c:(a.kinds.index(c['kind']),a.layouts.index(c['layout']),order.index(c['engine']),c['panel']))
					# Fixed cooldown, never a readiness/progress watcher.
					time.sleep(120)
					for cell in jobs:
						key='-'.join(str(cell[k]) for k in ('kind','layout','engine','panel'))
						available=int(next(line.split()[1] for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:')))*1024
						assert available>=n*96+1024**3, 'insufficient memory for serial case'
						d=round_dir/key;execute(a,base,cell,d,False)
						receipts.append(dict(path=str(d/'RESULT.json'),sha256=digest(d/'RESULT.json')))
		atomic(a.out/'RESULT.json',dict(status='PASS',mode='validation' if a.validate_only else 'timing',identity=base,receipts=receipts))
	except BaseException:
		atomic(a.out/'RESULT.json',dict(status='FAIL',error=traceback.format_exc()))
		print('FAIL: '+str(a.out/'RESULT.json'));return 1
	print('PASS: '+str(a.out/'RESULT.json'));return 0


if __name__=='__main__':raise SystemExit(main())
