"""Fixed-route exact program vectors for the existing eight CLI workloads."""
import tempfile
import argparse
import sys,subprocess,json,hashlib,traceback,os
from pathlib import Path
from concurrent.futures import ThreadPoolExecutor
root=Path(__file__).resolve().parents[2];sys.path[:0]=[str(root/'tools/column'),str(root/'rune/tools/xtask/measure')]
from child_result import atomic_json
from old_path_programs import KERNELS,fixture
from ir_pool import capture_environment
from ir_resources import resources,fixed_budget
import callgrind,ir_accounting
from cli_symbols import invocation_phase_symbols
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('--out',type=Path,required=True)
parser.add_argument('--build-receipt',type=Path,required=True)
parser.add_argument('--valgrind-root',type=Path,required=True)
parser.add_argument('--capture-only', action='store_true', help='Validate complete captures without applying the historical exact-vector verdict')
parser.add_argument('--native-phases', action='store_true', help='Measure preparation and execution separately for native-on cells')
args=parser.parse_args()
out=args.out.resolve();out.mkdir(exist_ok=False)
result={'status':'FAIL','purpose':'exact unchanged program and unknown-object function vectors; system deltas reported separately'}
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()

def retained_loader_cache(store, expected):
	"""Seed once from matching host bytes. A retained copy survives host cache updates."""
	if len(expected) != 64 or any(c not in '0123456789abcdef' for c in expected):
		raise ValueError('invalid loader cache digest')
	store.mkdir(parents=True, exist_ok=True)
	path = store / expected
	if not path.exists():
		data = Path('/etc/ld.so.cache').read_bytes()
		if hashlib.sha256(data).hexdigest() != expected:
			raise ValueError(f'restore the fixed-route loader cache at {path}')
		with tempfile.NamedTemporaryFile(dir=store) as scratch:
			scratch.write(data)
			scratch.flush()
			try:
				os.link(scratch.name, path)
			except FileExistsError:
				pass
	if sha(path) != expected:
		raise ValueError('retained loader cache digest mismatch')
	return path

def verify_mapped_libraries(maps, executable, libraries):
	found = set()
	for line in maps.splitlines():
		fields = line.split(maxsplit=5)
		if len(fields) < 6 or not fields[5].startswith('/') or fields[5] == executable:
			continue
		path = fields[5]
		if path in found:
			continue
		if path not in libraries or sha(path) != libraries[path]:
			raise ValueError(f'unexpected mapped library: {path}')
		stat = Path(path).stat()
		major, minor = [int(n, 16) for n in fields[3].split(':')]
		if (stat.st_dev, stat.st_ino) != (os.makedev(major, minor), int(fields[4])):
			raise ValueError(f'library mapping identity changed: {path}')
		found.add(path)
	if found != set(libraries):
		raise ValueError('loaded library set differs from frozen outer reference')

def native_stack_rows(prefix, executable, arguments, directory):
 """Count the native mappings at the same std -> libc call as the observer."""
 output='/run/rnx-route/out'
 script=directory/'stack.gdb'
 script.write_text("""set pagination off
set confirm off
set startup-with-shell off
set disable-randomization off
set debuginfod enabled off
set breakpoint pending on
break pthread_getattr_np
run
python
import os, pathlib, gdb
proc = pathlib.Path('/proc') / str(gdb.selected_inferior().pid)
assert os.path.samefile(proc/'exe', '/run/rnx-route/installed')
frame = gdb.newest_frame()
assert frame.name().lstrip('_').split('@')[0] == 'pthread_getattr_np'
assert frame.older().name() == 'std::rt::lang_start_internal'
pathlib.Path('/run/rnx-route/out/native.maps').write_bytes((proc/'maps').read_bytes())
end
kill
""")
 subprocess.run([*prefix,'/usr/bin/gdb','-nx','-batch','-x',output+'/stack.gdb','--args',
                 executable,*arguments],check=True,capture_output=True,timeout=120)
 maps=(directory/'native.maps').read_text().splitlines()
 before,=(i for i,line in enumerate(maps) if line.endswith(' [stack]'))
 if before < 1:
  raise ValueError('invalid native stack map')
 return before

def installed_dispatch(build, mode, directory, valgrind):
 """Count the real outer CLI up to its successful exec of the pinned helper."""
 directory.mkdir()
 cache=directory/'cache'
 cache.mkdir()
 fixture=directory/'program.rn'
 fixture.write_text('pub fn main(args) { 42 }\n')
 layout='/run/rnx-route'
 executable=layout+'/installed'
 arguments=['run','--debug-engine','--native='+mode,'--budget','1000000000',layout+'/program.rn']
 bindings=[(build['installed_binary'],'installed'),(valgrind,'valgrind'),(fixture,'program.rn')]
 reference=json.loads((root/'tools/column/fixed_route_policy.json').read_text())['installed_setup']['outer_reference']['host_inputs']
 if os.path.lexists('/etc/ld.so.preload'):
  raise ValueError('unexpected /etc/ld.so.preload')
 loader_cache=retained_loader_cache(root/'target/fixed-route-inputs',reference['loader_cache_sha256'])
 prefix=['/usr/bin/bwrap','--unshare-pid','--die-with-parent','--ro-bind','/','/',
  '--ro-bind',str(loader_cache),'/etc/ld.so.cache','--dev-bind','/dev','/dev',
  '--proc','/proc','--tmpfs','/tmp','--tmpfs','/run','--dir',layout]
 for src,name in bindings:prefix+=['--ro-bind',str(src),layout+'/'+name]
 env=dict(capture_environment(),XDG_CACHE_HOME=layout+'/cache',
  VALGRIND_LIB=layout+'/valgrind/libexec/valgrind',RNX_FILE_ROUTE_FD='3')
 prefix+=['--bind',str(cache),layout+'/cache','--bind',str(directory),layout+'/out','--chdir',layout,
  '--','/usr/bin/env','-i',*(k+'='+v for k,v in env.items()),'/bin/sh','-c',
  'exec 3>out/route; exec "$@"','sh']
 tracer=[layout+'/valgrind/bin/valgrind','--tool=callgrind','--trace-children=yes','--dump-before=execve',
  '--dump-instr=yes','--callgrind-out-file='+layout+'/out/cg.%p','--log-file='+layout+'/out/vg.%p']
 # The second observed run counts only the query. Its total must reproduce
 # the inclusive edge in the unmodified whole-window profile.
 query_arg=['--collect-atstart=no','--toggle-collect=pthread_getattr_np*']
 query_tracer=[s.replace('/out/cg.', '/out/query.').replace('/out/vg.', '/out/vq.') for s in tracer]
 query_tracer[2:2]=query_arg
 for observer in ([],tracer,query_tracer):
  result=subprocess.run([*prefix,*observer,executable,*arguments],check=True,capture_output=True,timeout=120)
  expected={'route':'execution','category':None,'reason':None,'location':None}
  if result.stdout!=b'42\n' or result.stderr!=('rnx: engine=execution native-requested='+mode+'\n').encode() or json.loads((directory/'route').read_text())!=expected:
   raise ValueError('installed launch returned wrong output or route')
 generation,=(cache/'rnx-file-images').iterdir()
 desc=build['measured']
 for name,key in [('engine','engine'),('helper','runner')]:
  if sha(generation/name)!=desc[key]['sha256']:
   raise ValueError('outer CLI launched a different '+name)
 outer,=directory.glob('cg.*.1')
 inner=outer.with_suffix('')
 if set(directory.glob('cg.*'))!={outer,inner}:
  raise ValueError('unexpected Callgrind exec dump or tail')
 def header(path,prefix):
  return [s[len(prefix):] for s in path.read_text().splitlines() if s.startswith(prefix)]
 command=' '.join(arguments)
 if (header(outer,'desc: Trigger: ')!=['--dump-before=execve']
   or header(outer,'cmd:  ')!=[executable+' '+command]
   or header(inner,'desc: Trigger: ')!=['Program termination']
   or header(inner,'cmd:  ')!=['/proc/self/fd/10 11,10,-1 '+command]
   or header(outer,'pid: ')!=header(inner,'pid: ')):
  raise ValueError('invalid installed exec boundary')
 query,=directory.glob('query.*.1')
 if (header(query,'cmd:  ')!=[executable+' '+command]
     or header(query,'desc: Trigger: ')!=['--dump-before=execve']):
  raise ValueError('invalid query capture')
 guest_pid=reference['guest_pid']
 if any(header(path,'pid: ') != [str(guest_pid)] for path in (outer,inner,query)):
  raise ValueError('outer or isolated query ran at a different guest PID')
 observed=ir_accounting.account(query,executable,system_window=True)
 if not (ir_accounting.outer_stack_query(outer,executable)
         == ir_accounting.outer_stack_query(query,executable) == observed['total_Ir']):
  raise ValueError('isolated query does not reproduce the raw outer edge')
 before=native_stack_rows(prefix,executable,arguments,directory)
 verify_mapped_libraries((directory/'native.maps').read_text(),executable,reference['libraries'])
 preload='/libexec/valgrind/vgpreload_core-amd64-linux.so'
 if sha(str(valgrind)+preload)!=reference['vgpreload_sha256'] or sha(loader_cache)!=reference['loader_cache_sha256']:
  raise ValueError('measurement host inputs changed during dispatch')
 if os.path.lexists('/etc/ld.so.preload'):
  raise ValueError('unexpected /etc/ld.so.preload')
 libc='/usr/lib/x86_64-linux-gnu/libc.so.6'
 allocator=ir_accounting.allocator_path(outer,libc,reference['libraries'][libc])
 objects={line.split(') ',1)[1] for line in outer.read_text().splitlines()
          if line.startswith(('ob=(', 'cob=(')) and ') /' in line}
 if objects!={executable,layout+'/valgrind'+preload,*reference['libraries']}:
  raise ValueError('outer profile library set differs from frozen reference')
 counts=ir_accounting.account(outer,executable)
 context={'argv':tracer+[executable,*arguments],'environ':list(env.items()),'cwd':layout}
 launch_hash=hashlib.sha256(json.dumps(context,sort_keys=True,separators=(',',':')).encode()).hexdigest()
 mapping={'allocator':allocator,'host_inputs':reference,'launch_sha256':launch_hash,'profile':str(outer),'profile_sha256':sha(outer),'inner':str(inner),'inner_sha256':sha(inner),
  'W':counts['rnx_Ir']+counts['other_Ir'],'T':counts['total_Ir'],
  'stack_query':{'T':observed['total_Ir'],'W':observed['rnx_Ir']+observed['other_Ir'],'native_before_stack':before},
  'query':str(query),'query_sha256':sha(query),'maps':str(directory/'native.maps'),'maps_sha256':sha(directory/'native.maps')}
 # Record the outer ELF's loader work. A different loader keeps the ordinary rule.
 rule=json.loads((root/'tools/column/fixed_route_policy.json').read_text()).get('relative_relocations')
 if mode=='on' and rule and reference['libraries'].get(rule['loader'])==rule['sha256']:
  loader=rule['loader']
  outer_ir=ir_accounting.loader_instructions(counts,loader)
  if outer_ir is not None:
   table=subprocess.check_output(['/usr/bin/readelf','-Wr',build['installed_binary']],text=True)
   ordinary=sum(len(fields)>=3 and fields[2]=='R_X86_64_RELATIVE' for fields in map(str.split,table.splitlines()))
   if sha(build['installed_binary'])!=desc['installed']['sha256']:
    raise ValueError('installed image changed during relocation measurement')
   mapping['relative_relocations']={'version':rule['version'],'artifacts':desc,'count':ordinary,
    'loader_sha256':rule['sha256'],'outer_Ir':outer_ir,'query_Ir':ir_accounting.loader_instructions(observed,loader) or 0,
    'query_sha256':mapping['query_sha256'],'profile_sha256':mapping['profile_sha256'],'launch_sha256':launch_hash}
 receipt=dict(status='PASS',mode=mode,artifacts=desc,launch=context,route=expected,**mapping)
 proof=directory/'DISPATCH.json'
 atomic_json(proof,receipt)
 return dict(mapping,proof=str(proof),proof_sha256=sha(proof))

try:
 pair=json.loads(args.build_receipt.read_text());assert pair['status']=='PASS' and len(pair['builds'])==2
 binaries=dict(zip(('baseline','candidate'),pair['builds']))
 meter=json.loads((root/'tools/column/fixed_route_policy.json').read_text())['meter']
 entries={}
 phase_symbols={}
 for role,b in binaries.items():
  assert sha(b['binary'])==b['sha256']
  owner=b.get('engine_binary',b['binary'])
  desc=b.get('measured')
  if desc is not None:
   assert desc['version']==(2 if 'installed_binary' in b else 1) and type(desc['version']) is int and desc['target']=='x86_64-unknown-linux-gnu'
   assert desc['runner']=={'path':'/run/rnx-route/rnx','sha256':b['sha256']}
   expected={'version','target','runner'}
   if 'engine_binary' in b:expected.add('engine')
   if 'installed_binary' in b:
    expected.add('installed')
    assert 'engine_binary' in b and desc['installed']=={'path':'/run/rnx-route/installed','sha256':sha(b['installed_binary'])}
   assert set(desc)==expected
  if 'engine_binary' in b:
   assert desc['engine']['path']=='/run/rnx-route/libengine_image.so'
   assert args.native_phases and sha(owner)==b['measured']['engine']['sha256']
  raw=subprocess.check_output(['nm','--defined-only',owner],text=True).splitlines()
  dem=subprocess.check_output(['nm','-C','--defined-only',owner],text=True).splitlines()
  a=[s.split()[0] for s in dem if s.endswith(' <rune::experimental_core::execute::PreparedInvocation>::invoke')];assert len(a)==1
  names=[s.split()[-1] for s in raw if s.split()[0]==a[0]];assert len(names)==1
  entries[role]=names[0]
  if args.native_phases:
   src=subprocess.check_output(['git','show',b['source_commit']+':rune/crates/rune/src/experimental_core/execute.rs'],cwd=root,text=True)
   phase_entry,phase_symbols[role]=invocation_phase_symbols(owner,src)
   assert phase_entry==entries[role]
   if 'engine_binary' in b:
    runner_names={line.split()[-1] for line in subprocess.check_output(['nm','--defined-only',b['binary']],text=True).splitlines() if line.split()}
    if runner_names.intersection([phase_entry,*phase_symbols[role].values()]):raise ValueError("engine selector also defined in runner")
 observation=resources();workers=fixed_budget(observation);available=observation['mem_available_bytes'];headroom=observation['ancestor_headroom_bytes']
 atomic_json(out/'MANIFEST.json',{'binaries':binaries,'entries':entries,'workers':workers,'mem_available':available,'cgroup_headroom':headroom,'per_worker_admission':2*1024**3,'environment':capture_environment(),'sizes':[1,5],'kernels':KERNELS,'policies':['off','on']})
 dispatch={}
 for role,b in binaries.items():
  if 'installed_binary' in b:
   for mode in ('off','on'):
    dispatch[role,mode]=installed_dispatch(b,mode,out/f'dispatch-{role}-{mode}',args.valgrind_root.resolve())
 def job(spec):
  kind,policy,n,role=spec;d=out/f'{kind}-{policy}-{n}-{role}';d.mkdir();r={'status':'FAIL','kind':kind,'policy':policy,'n':n,'role':role}
  try:
   b=binaries[role];assert sha(b['binary'])==b['sha256']
   src=d/'program.rn';src.write_text(fixture(kind,n));opts=['--native='+policy]
   direct_env=capture_environment()
   if 'installed_binary' in b:direct_env['XDG_CACHE_HOME']=str(out/'installed-cache')
   direct=subprocess.run([b.get('installed_binary',b['binary']),'run','--debug-engine',*opts,'--budget','1000000000',str(src)],env=direct_env,capture_output=True,timeout=120)
   (d/'direct.stdout').write_bytes(direct.stdout);(d/'direct.stderr').write_bytes(direct.stderr)
   assert direct.returncode==0 and b'rnx: engine=execution' in direct.stderr,(direct.returncode,direct.stderr.decode())
   cfg={'binary':b['binary'],'entry':entries[role],'arm':None,'options':opts,'budget':1000000000,'out':str(d),'valgrind_root':str(args.valgrind_root.resolve()),'fixture':str(src),'gdb_receipt':str(d/'GDB.json')}
   phased=args.native_phases and policy=='on'
   measured_binary=b['binary']
   if args.native_phases:
    cfg['canonical_layout']=True
    cfg['meter']=meter
    measured_binary='/run/rnx-route/rnx'
   engine=None
   if 'measured' in b:cfg['measured']=b['measured']
   if 'engine_binary' in b:
    cfg['engine_binary']=b['engine_binary'];engine=b['measured']['engine']['path']
   if 'installed_binary' in b:
    cfg['installed_binary']=b['installed_binary']
    r['dispatch']=dispatch[role,policy]
   if phased:cfg['phase_symbols']=phase_symbols[role]
   atomic_json(d/'CONFIG.json',cfg)
   with (d/'child.log').open('w') as log:p=subprocess.run([sys.executable,'-B',str(root/'tools/column/old_path_child.py'),str(d/'CONFIG.json')],env=capture_environment(),stdout=log,stderr=subprocess.STDOUT,timeout=720)
   assert p.returncode==0
   assert (d/'program.stdout').read_bytes()==direct.stdout
   if 'meter' in cfg:
    assert (d/'program.stderr').read_bytes()==direct.stderr
   if phased:
    whole,parts=ir_accounting.phase_accounts(d,measured_binary,engine,log="valgrind.log")
    boundary=json.loads((d/'GDB.json').read_text())
    assert boundary['status']=='PASS' and boundary['split'] in phase_symbols[role]
    assert sum(v for k,v in parts['execution']['accounting']['rnx_functions'].items() if k==boundary['split'])>0
    r.update(accounting=whole,phases=parts,boundary=boundary)
   else:
    profiles=list(d.glob('map.callgrind.*'));assert len(profiles)==1
    profile=callgrind.parse(profiles[0]);assert profile['total_Ir']>0
    assert sum(v['self_Ir'] for k,v in profile['functions'].items() if 'Executor' in k and 'invoke_args' in k)>0
    r.update(accounting=ir_accounting.account(profiles[0],measured_binary,engine),profile=str(profiles[0]),profile_sha256=sha(profiles[0]))
   if not phased and (engine or 'meter' in cfg):r['image_boundary']=json.loads((d/'GDB.json').read_text())
   r.update(status='PASS',stdout_sha256=sha(d/'program.stdout'),fixture_sha256=sha(src))
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
 result.update(status='FAIL' if vector_differences and not args.capture_only else 'PASS',comparisons=comparisons,vector_differences=vector_differences)
except Exception as error:
 result['error']=traceback.format_exc()
 if hasattr(error,'numbers'):result['admission']=error.numbers
finally:
 atomic_json(out/'RESULT.json',result)
 print(result['status'],out/'RESULT.json',flush=True)
raise SystemExit(result['status']!='PASS')
