"""Fixed-route exact program vectors for the existing eight CLI workloads."""
import argparse
import sys,subprocess,json,hashlib,traceback,os
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
root=Path(__file__).resolve().parents[2];sys.path[:0]=[str(root/'tools/column'),str(root/'rune/tools/xtask/measure')]
from child_result import atomic_json
from old_path_programs import KERNELS,fixture
from ir_pool import capture_environment
from ir_resources import resources
import callgrind,ir_accounting
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--build-receipt',type=Path,required=True)
parser.add_argument('--valgrind-root',type=Path,required=True)
args=parser.parse_args()
out=args.out.resolve();out.mkdir(exist_ok=False)
result={'status':'FAIL','purpose':'exact unchanged program and unknown-object function vectors; system deltas reported separately'}
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
try:
 pair=json.loads(args.build_receipt.read_text());assert pair['status']=='PASS'
 binaries=dict(zip(('baseline','candidate'),pair['builds']))
 entries={}
 for role,b in binaries.items():
  assert sha(b['binary'])==b['sha256']
  raw=subprocess.check_output(['nm','--defined-only',b['binary']],text=True).splitlines()
  dem=subprocess.check_output(['nm','-C','--defined-only',b['binary']],text=True).splitlines()
  a=[s.split()[0] for s in dem if s.endswith(' <rune::experimental_core::execute::PreparedInvocation>::invoke')];assert len(a)==1
  names=[s.split()[-1] for s in raw if s.split()[0]==a[0]];assert len(names)==1
  entries[role]=names[0]
 cpus,available,headroom=resources();workers=min(4,len(cpus),max(0,(min(available,headroom)-2*1024**3)//(2*1024**3)));assert workers>0
 atomic_json(out/'MANIFEST.json',{'binaries':binaries,'entries':entries,'workers':workers,'mem_available':available,'cgroup_headroom':headroom,'per_worker_admission':2*1024**3,'environment':capture_environment(),'sizes':[1,5],'kernels':KERNELS,'policies':['off','on']})
 def job(spec):
  kind,policy,n,role=spec;d=out/f'{kind}-{policy}-{n}-{role}';d.mkdir();r={'status':'FAIL','kind':kind,'policy':policy,'n':n,'role':role}
  try:
   b=binaries[role];assert sha(b['binary'])==b['sha256']
   src=d/'program.rn';src.write_text(fixture(kind,n));opts=['--native='+policy]
   direct=subprocess.run([b['binary'],'run','--debug-engine',*opts,'--budget','1000000000',str(src)],env=capture_environment(),capture_output=True,timeout=120)
   (d/'direct.stdout').write_bytes(direct.stdout);(d/'direct.stderr').write_bytes(direct.stderr)
   assert direct.returncode==0 and b'rnx: engine=execution' in direct.stderr,(direct.returncode,direct.stderr.decode())
   cfg={'binary':b['binary'],'entry':entries[role],'arm':None,'options':opts,'budget':1000000000,'out':str(d),'valgrind_root':str(args.valgrind_root.resolve()),'fixture':str(src),'gdb_receipt':str(d/'GDB.json')}
   atomic_json(d/'CONFIG.json',cfg)
   with (d/'child.log').open('w') as log:p=subprocess.run([sys.executable,'-B',str(root/'tools/column/old_path_child.py'),str(d/'CONFIG.json')],env=capture_environment(),stdout=log,stderr=subprocess.STDOUT,timeout=720)
   assert p.returncode==0
   assert (d/'program.stdout').read_bytes()==direct.stdout
   profiles=list(d.glob('map.callgrind.*'));assert len(profiles)==1
   profile=callgrind.parse(profiles[0]);assert profile['total_Ir']>0
   assert sum(v['self_Ir'] for k,v in profile['functions'].items() if 'Executor' in k and 'invoke_args' in k)>0
   r.update(status='PASS',accounting=ir_accounting.account(profiles[0],b['binary']),profile=str(profiles[0]),profile_sha256=sha(profiles[0]),stdout_sha256=sha(d/'program.stdout'))
  except Exception:r['error']=traceback.format_exc()
  atomic_json(d/'RESULT.json',r);return r
 specs=[(k,p,n,r) for k in KERNELS for p in ('off','on') for n in (1,5) for r in binaries]
 with ThreadPoolExecutor(max_workers=workers) as pool:records=list(pool.map(job,specs))
 result['records']=[str(out/f"{r['kind']}-{r['policy']}-{r['n']}-{r['role']}"/'RESULT.json') for r in records]
 assert all(r['status']=='PASS' for r in records),'capture failures retained per case'
 index={(r['kind'],r['policy'],r['n'],r['role']):r for r in records};comparisons=[];vector_differences=[]
 for k in KERNELS:
  for p in ('off','on'):
   deltas=[]
   for n in (1,5):
    a,b=[index[k,p,n,r] for r in binaries];assert a['stdout_sha256']==b['stdout_sha256']
    for category in ('rnx_functions','other_functions'):
     left,right=a['accounting'][category],b['accounting'][category]
     for name in left.keys()|right.keys():
      delta=right.get(name,0)-left.get(name,0)
      if delta:vector_differences.append({'kind':k,'policy':p,'n':n,'partition':category,'function':name,'delta':delta})
    deltas.append({key:b['accounting'][key]-a['accounting'][key] for key in ('total_Ir','rnx_Ir','dynamic_Ir','other_Ir')})
   for role in binaries:assert index[k,p,5,role]['accounting']['total_Ir']>index[k,p,1,role]['accounting']['total_Ir']
   comparisons.append({'kind':k,'policy':p,'delta_n1':deltas[0],'delta_n5':deltas[1],'extra_growth':{key:deltas[1][key]-deltas[0][key] for key in deltas[0]}})
 result.update(status='FAIL' if vector_differences else 'PASS',comparisons=comparisons,vector_differences=vector_differences)
except Exception:result['error']=traceback.format_exc()
finally:
 atomic_json(out/'RESULT.json',result)
 print(result['status'],out/'RESULT.json',flush=True)
raise SystemExit(result['status']!='PASS')
