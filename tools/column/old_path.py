"""Compare actual CLI execution scopes with mandatory workload-presence guards."""
import child_result
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import traceback

HERE=Path(__file__).resolve().parent
ROOT=HERE.parents[1]
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--mode',choices=('preflight','full'),required=True)
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--build-receipt',type=Path,required=True)
parser.add_argument('--rune-root',type=Path,required=True)
parser.add_argument('--valgrind-root',type=Path,required=True)
args=parser.parse_args()
sys.path.insert(0,str(args.rune_root.resolve(strict=True)/'tools/xtask/measure'))
import callgrind
import measurement_runtime as runtime
import measurement_readiness as readiness
from old_path_programs import KERNELS,fixture
from old_path_rules import require_workload,compare_growth

mode=args.mode
out=args.out.resolve();out.mkdir(exist_ok=False)
result={'status':'FAIL','mode':mode,'records':[],'comparisons':[]}
started=time.monotonic()

def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def ready(lease,path):
	original=readiness.require_ready
	def recorded(observation,**limits):
		path.write_text(json.dumps({'observation':observation,'limits':limits},indent=2)+'\n')
		return original(observation,**limits)
	readiness.require_ready=recorded
	try:return lease.ready(max_load=2,min_available_bytes=8*1024**3)
	finally:readiness.require_ready=original

def symbols(binary):
	raw=subprocess.check_output(['nm','--defined-only',binary],text=True).splitlines()
	demangled=subprocess.check_output(['nm','-C','--defined-only',binary],text=True).splitlines()
	out={}
	for key,name in [('legacy','<rune::runtime::vm::Vm>::run'),('arm','rnx::execute::drive_async::<(rune::runtime::value::Value,)>'),('execution','<rune::experimental_core::execute::PreparedInvocation>::invoke')]:
		addresses=[line.split()[0] for line in demangled if line.endswith(' '+name)]
		assert len(addresses)==1,(key,addresses)
		names=[line.split()[-1] for line in raw if line.split()[0]==addresses[0]]
		assert len(names)==1,(key,names)
		out[key]=names[0]
	return out

try:
	subprocess.run([sys.executable,'-B','-m','unittest','discover','-s',str(HERE),'-p','test_old_path_rules.py'],check=True,capture_output=True)
	build=json.loads((args.build_receipt.resolve(strict=True)).read_text());assert build['status']=='PASS'
	source_commit=subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()
	tooling_changes=subprocess.check_output(['git','diff','--name-only',build['candidate'],source_commit],cwd=ROOT,text=True).splitlines()
	assert all(name.startswith(('tools/column/','plans/impl/')) or name == 'probes/0072/inventory/doc.sh' for name in tooling_changes),'build source differs outside column tooling/inventory producer'
	result.update(source_commit=source_commit,tooling_changes_since_build=tooling_changes)
	pins={str(p):sha(p) for p in HERE.glob('*.py')}
	pins.update({str(p):sha(p) for p in (args.rune_root.resolve(strict=True)/'tools/xtask/measure').glob('*.py')})
	for row in build['builds'].values():
		assert sha(row['binary'])==row['sha256'];pins[row['binary']]=row['sha256']
	entries={role:symbols(row['binary']) for role,row in build['builds'].items()}
	result.update(build_receipt_sha256=sha(args.build_receipt.resolve(strict=True)),pins=pins,symbols=entries)
	def exact():
		for p,h in pins.items():assert sha(p)==h,p
		assert subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip()==source_commit
		assert not subprocess.check_output(['git','status','--porcelain'],cwd=ROOT,text=True).strip()
	vgroot=args.valgrind_root.resolve(strict=True)
	result['dependencies']=runtime.dependency_snapshot([Path(x['binary']) for x in build['builds'].values()]+[vgroot/'bin/valgrind',vgroot/'bin/vgdb',Path('/usr/bin/gdb'),Path('/usr/bin/python3')])
	result['profiler']=runtime.tool_tree(vgroot)
	env={'PATH':os.defpath,'HOME':str(Path.home()),'LANG':'C','LC_ALL':'C','POLARS_MAX_THREADS':'1'}
	result['environment']=env
	families=[(kind,policy) for kind in KERNELS for policy in ('legacy','execution-off','execution-on')]+[('host','legacy'),('callback','legacy')]
	if mode=='preflight':families=[('loops','execution-on'),('numeric_calls','legacy'),('callback','legacy')]
	deadline=time.monotonic()+21600
	time.sleep(180)
	with runtime.lease('/tmp/rnx-runtime-bench.lock',2) as lease:
		for kind,policy in families:
			values=(1,5) if kind in KERNELS else (64,256)
			costs={}
			for n in values:
				source=out/f'{kind}-{policy}-{n}.rn';source.write_text(fixture(kind,n))
				opts=['--native=on' if policy=='execution-on' else '--native=off']
				if policy=='legacy':opts+=['--engine=legacy']
				outputs=[]
				for role in ('published','candidate'):
					exact();binary=build['builds'][role]['binary']
					case=out/f'{kind}-{policy}-{n}-{role}';case.mkdir()
					route='legacy' if policy=='legacy' else 'execution'
					before=ready(lease,case/'READY-direct.json')
					direct=lease.execute([binary,'run','--debug-engine',*opts,'--budget','1000000000',str(source)],cwd=ROOT,env=env,stdout_path=case/'direct.stdout',stderr_path=case/'direct.stderr',timeout=180,deadline=deadline)
					assert direct['status']==0,case
					stdout=(case/'direct.stdout').read_text();outputs.append(stdout)
					assert f'rnx: engine={route}' in (case/'direct.stderr').read_text(),case
					config={'binary':binary,'entry':entries[role][route],'arm':entries[role]['arm'] if route=='legacy' else None,'options':opts,'budget':1000000000,'out':str(case),'valgrind_root':str(vgroot),'fixture':str(source),'gdb_receipt':str(case/'GDB.json')}
					config_path=case/'CONFIG.json';config_path.write_text(json.dumps(config,indent=2)+'\n')
					before=ready(lease,case/'READY-profile.json')
					profiled=lease.execute(['/usr/bin/python3','-B',str(HERE/'old_path_child.py'),str(config_path)],cwd=ROOT,env=env,stdout_path=case/'child.stdout',stderr_path=case/'child.stderr',timeout=720,deadline=deadline)
					assert profiled['status']==0,case
					assert (case/'program.stdout').read_text()==stdout,case
					parts=sorted(case.glob('map.callgrind.*'));assert len(parts)==1,case
					profile=callgrind.parse(parts[0])
					proof=require_workload(profile,kind,n)
					for name,counts in profile['functions'].items():
						if 'experimental_core::frontend::compile' in name:assert counts['self_Ir']==0,'compilation entered interval'
					costs[(n,role)]=profile['total_Ir']
					row={'kind':kind,'policy':policy,'n':n,'role':role,'interval':'outer '+route+' invocation; legacy armed only after drive_async entry; nested callbacks included across threads','Ir':profile['total_Ir'],'workload_proof':proof,'profile_sha256':sha(parts[0]),'direct':direct,'profile_execution':profiled,'gdb':json.loads((case/'GDB.json').read_text()),'profile':str(parts[0])}
					(case/'CAPTURE.json').write_text(json.dumps(row,indent=2)+'\n');result['records'].append(str(case/'CAPTURE.json'))
					exact();time.sleep(5)
				assert outputs[0]==outputs[1],(kind,policy,n)
			low,high=values
			comparison=compare_growth([costs[(low,'published')],costs[(high,'published')]],[costs[(low,'candidate')],costs[(high,'candidate')]])
			result['comparisons'].append(dict(comparison,kind=kind,policy=policy,n=list(values)))
		result['status']='PASS' if all(c['pass'] for c in result['comparisons']) else 'FAIL'
except BaseException as error:
	result['error']=repr(error);(out/'TRACEBACK.txt').write_text(traceback.format_exc())
finally:
	result['seconds']=time.monotonic()-started
	child_result.publish(out/'RESULT.json',result)
	summary='0205 existing-path '+mode+' EXIT '+result['status'];notice=out/'NOTICE.txt'
	notice.write_text(summary+'. Receipt: '+str(out/'RESULT.json')+'\n')
raise SystemExit(0 if result['status']=='PASS' else 1)
