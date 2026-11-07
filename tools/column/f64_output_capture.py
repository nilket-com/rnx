import argparse,json,os,platform,shutil,subprocess,sys,traceback
from pathlib import Path
ROOT=Path(__file__).resolve().parents[2]
sys.path.insert(0,str(ROOT/'tools/column'));import ir_pool
sys.path.insert(0,str(ROOT/'rune/tools/xtask/supplemental'));from receipts import Run,atomic_json,digest
p=argparse.ArgumentParser();p.add_argument('--resources',required=True);p.add_argument('--out',required=True);p.add_argument('--controls',required=True);p.add_argument('--source',required=True);p.add_argument('--baseline-builds',required=True);p.add_argument('--measurement-tooling',required=True);p.add_argument('--valgrind',required=True);args=p.parse_args();OUT=Path(args.out).resolve()
OUT.mkdir(exist_ok=True);r={'status':'FAIL'}
try:
	head=subprocess.check_output(['git','rev-parse','HEAD'],text=True).strip()
	assert not subprocess.check_output(['git','status','--porcelain'],text=True).strip()
	assert subprocess.check_output(['git','show','-s','--format=%s'],text=True).startswith('impl: adr 0222 ')
	core=Path(args.controls).resolve();assert json.loads(core.read_text())['status']=='PASS'
	source=Path(args.source).resolve()
	assert not subprocess.check_output(['git','status','--porcelain'],cwd=source,text=True).strip()
	subprocess.run(['git','checkout','--detach',head],cwd=source,check=True)
	subprocess.run(['git','verify-commit',head],cwd=source,check=True)
	command=['cargo','+1.98.1','build','--locked','--release','--manifest-path','adapters/polars/Cargo.toml','--bin','rnx-polars']
	env=dict(os.environ,CARGO_BUILD_JOBS='4',PYTHONDONTWRITEBYTECODE='1')
	old=json.loads(Path(args.baseline_builds).read_text());baseline=dict(next(x for x in old['builds'] if x['role']=='candidate'),role='baseline')
	assert digest(baseline['binary'])==baseline['sha256']
	diff=subprocess.check_output(['git','diff','--name-only',baseline['source_commit'],'7c51bf600f9f0655b6d98034dfc2b0d4caafa2ce'],text=True).splitlines()
	assert all(x.startswith(('ci/','plans/','tools/column/','BACKLOG.md')) for x in diff)
	with Run(OUT/'builds',{'head':head,'command':command,'controller':digest(__file__),'core':digest(core)},False) as run:
		attempt,reused=run.begin('candidate')
		log=attempt/'build.log'
		with log.open('x') as f:done=subprocess.run(command,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=14400)
		if done.returncode:
			run.finish(attempt,'FAIL',[log],exit=done.returncode);raise RuntimeError('build failed')
		binary=attempt/'rnx-polars';shutil.copy2(source/'adapters/polars/target/release/rnx-polars',binary)
		candidate={'role':'candidate','source_commit':head,'rnx':head,'rune':head,'binary':str(binary),'sha256':digest(binary),'command':command}
		run.finish(attempt,'PASS',[log,binary],build=candidate)
	roles=[baseline,candidate];atomic_json(OUT/'BUILDS.json',{'status':'PASS','builds':roles,'baseline_source_equivalence':diff})
	# Shape and hot-loop gates precede the dynamic capture.
	import simd_shape,simd_hot_loop
	names=tuple(dict.fromkeys((*simd_shape.NAMES,*simd_hot_loop.EXPECTED)))
	shapes={role['role']:simd_shape.binary_shapes(role['binary'],names) for role in roles}
	comparison=simd_shape.compare(shapes['baseline'],shapes['candidate'],names)
	hot={role:simd_hot_loop.assess(value) for role,value in shapes.items()}
	shape_pass=all(v['equal'] for v in comparison.values()) and all(v['status']=='PASS' for value in hot.values() for v in value.values())
	atomic_json(OUT/'SIMD.json',{'status':'PASS' if shape_pass else 'FAIL','shapes':shapes,'comparison':comparison,'hot_loops':hot})
	assert shape_pass,'protected SIMD shape/hot-loop gate'
	import f64_plan_shape
	own=f64_plan_shape.record(candidate['binary'])
	atomic_json(OUT/'F64-SHAPE.json',own)
	assert own['status']=='PASS','F64 vector shape requires review'
	c=ir_pool.tooling(Path(args.measurement_tooling).resolve())
	import cli_fixtures
	cli_fixtures.KERNELS.update({
	'f64chain':('f64','pub fn map(x,y) { x * 2.0 + y }',['1.0','2.0'],'4.0'),
	'booland':('bool','pub fn map(x,y) { x & y }',['true','false'],'false'),
	'boolor':('bool','pub fn map(x,y) { x | y }',['true','false'],'true'),
	'booleq':('bool','pub fn map(x,y) { x == y }',['true','false'],'false'),
	'boolxor':('bool','pub fn map(x,y) { x ^ y }',['true','false'],'true'),
	'boolchain':('bool','pub fn map(a,b,c) { (a & b) | c }',['true','false','true'],'true')})
	for role in roles:role['symbols']=c.symbols(role['binary'])
	identity={'head':head,'controller':digest(__file__),'environment':ir_pool.capture_environment(),'budget':1000000000,'calls':2,'aslr':{'disabled':True,'setarch':'/usr/bin/setarch','machine':platform.machine()},'boundary_tooling':ir_pool.PIN}
	limits=json.loads(Path(args.resources).read_text());workers=min(4,limits['workers']);atomic_json(OUT/'MANIFEST.json',dict(identity=identity,resources=limits,workers=workers))
	cases=[(k,l) for k in ['booland','boolor','boolchain','booleq','boolxor','f64chain','multiply3','i64add','axpy','f64const','f64add','boolnot'] for l in cli_fixtures.LAYOUTS]
	jobs=[dict(kind=k,rows=n,layout=l,role=role,identity=identity,valgrind=str(Path(args.valgrind).resolve()),directory=str(OUT/'matrix00'/ir_pool.slot_name(k,n,l,role['role'])),import_directory=None) for k,l in cases for n in [4096,65536] for role in roles]
	records=ir_pool.run_jobs(jobs,workers)
	import f64_output_vectors as f64_plan_vectors
	raw=[x for x,y in records]
	verdict=f64_plan_vectors.evaluate(raw, roles)
	assert own['binary_sha256']==candidate['sha256']
	executed=f64_plan_vectors.executed_shapes(raw,own)
	atomic_json(OUT/'EXECUTED-SHAPES.json',executed)
	verdict['executed_shapes']=executed
	if executed['status']!='PASS': verdict['status']='FAIL'
	atomic_json(OUT/'VECTORS.json',verdict)
	r.update(status=verdict['status'],head=head,records=raw,cases=[y for x,y in records],acceptance=verdict)
except BaseException as e:r.update(error=repr(e),traceback=traceback.format_exc())
finally:
	atomic_json(OUT/'RESULT.json',r);(OUT/'NOTICE.txt').write_text(str(OUT/'RESULT.json')+'\n')
raise SystemExit(0 if r['status']=='PASS' else 1)
