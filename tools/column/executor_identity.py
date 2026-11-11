"""ADR 0236: fail-closed complete-executor instance and instruction identity gate."""
import argparse
import json
from pathlib import Path
import re
import subprocess
import traceback
from child_result import atomic_json, digest
from elf_targets import Elf
from i64_compare_identity import resolve, UnreviewedForm

PREFIX = 'rune::experimental_core::execute::'
NEW = ('SlicedInvocation', 'SliceOutcome', 'start_sliced', 'resume_sliced',
       'sliced_failure', 'sliced_lifecycle_failure', 'budgeted_sliced_intrinsic')


def symbols(binary):
 rows = {}
 for line in subprocess.check_output(['nm', '-S', '-C', '--defined-only', str(binary)], text=True).splitlines():
  parts = line.split(maxsplit=3)
  if len(parts) != 4 or parts[2] not in {'t','T','w','W'}: continue
  name = parts[3]
  if not (name.startswith(('rune::experimental_core::', '<rune::experimental_core::')) or name.startswith('rnx::runner::execution::')): continue
  if any(word in name for word in NEW): continue
  rows.setdefault(name, []).append({'address':int(parts[0],16),'size':int(parts[1],16)})
 assert rows and any('Executor>::invoke_args::<' in n for n in rows), 'missing complete invocation'
 return rows


def run(baseline, candidate, out):
 result = {'status':'FAIL','binaries':[{'path':str(p),'sha256':digest(p)} for p in (baseline,candidate)]}
 try:
  sides = [symbols(p) for p in (baseline,candidate)]
  result['instances'] = sides
  result['scope'] = 'Conservative superset: all emitted experimental_core routines and fixed file-run execution wrapper, excluding only named new sliced entries. Includes heap/operation helpers beyond execute module.'
  differences = {n:{'baseline':[r['size'] for r in sides[0].get(n,[])],
                    'candidate':[r['size'] for r in sides[1].get(n,[])]}
                 for n in sides[0].keys()|sides[1].keys()
                 if [r['size'] for r in sides[0].get(n,[])] != [r['size'] for r in sides[1].get(n,[])]}
  result['shape_differences'] = differences
  if differences:
   result['reason']='fixed executor/helper instance or byte-length drift; stopped before behavioral gates'
  else:
   # Resolve relocations only with already reviewed instruction forms. Any
   # unfamiliar target or instruction fails closed for review, never normalizes away.
   streams=[]; unsupported={'unreviewed form':set(), 'duplicate symbol':set()}; first=None
   for binary, inventory in zip((baseline,candidate),sides):
    elf=Elf(binary); current={}
    for name, instances in inventory.items():
     if len(instances)!=1:
      unsupported['duplicate symbol'].add(name)
      if first is None: first=dict(kind='duplicate symbol', symbol=name, multiplicity=len(instances), binary=str(binary))
      continue
     symbol=instances[0]
     symbol['disassembly']=subprocess.check_output(['objdump','-d','-C','--insn-width=16',
      f"--start-address={symbol['address']}",f"--stop-address={symbol['address']+symbol['size']}",str(binary)],text=True)
     try:
      current[name]=resolve(symbol,elf)
     except UnreviewedForm as error:
      unsupported['unreviewed form'].add(name)
      if first is None: first=dict(kind='unreviewed form', symbol=name, instruction=error.instruction, binary=str(binary))
    streams.append(current)
   result['resolved']=streams
   if any(unsupported.values()):
    result.update(reason='instruction identity not established: unsupported binary content',
                  first_unreviewed=first, affected_symbols=len(set().union(*unsupported.values())),
                  unsupported_classes={k:len(v) for k,v in unsupported.items() if v})
   else:
    result['differences']=[n for n in streams[0] if streams[0][n]!=streams[1][n]]
    result['status']='FAIL' if result['differences'] else 'PASS'
 except Exception:
  result['error']=traceback.format_exc()
 finally:
  atomic_json(out,result)
  print(result['status'],out,flush=True)
 return result['status']!='PASS'

if __name__=='__main__':
 p=argparse.ArgumentParser(description=__doc__)
 p.add_argument('--baseline',type=Path,required=True);p.add_argument('--candidate',type=Path,required=True)
 p.add_argument('--out',type=Path,required=True)
 a=p.parse_args();raise SystemExit(run(a.baseline,a.candidate,a.out))
