"""ADR 0238 cross-program ownership and invocation controls with durable terminal receipts."""
import argparse
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import time
import traceback
import child_result

ROOT=Path(__file__).resolve().parents[2]

def main():
	p=argparse.ArgumentParser(description=__doc__)
	p.add_argument('--scope',choices=('development','behavior','miri'),required=True)
	p.add_argument('--out',type=Path,required=True)
	a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
	r={'status':'FAIL','scope':a.scope,'checks':[]}
	try:
		identity=child_result.source(ROOT)
		env=dict(os.environ,CARGO_BUILD_JOBS='4',CARGO_INCREMENTAL='0')
		with (out/'preflight.log').open('x') as log:
			subprocess.run([sys.executable,'-B',str(ROOT/'ci/preflight.py')],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,check=True)
		base=['cargo','+1.98.1','test','--locked','-p','rune','--profile','engine-dev','--features','native-code,fmt,workspace','--lib']
		if a.scope in ('development','behavior'):
			commands=[('domain',base+['experimental_core::domain::tests','--','--nocapture'])]
			if a.scope=='behavior':
				commands += [('token-and-executor',base+['experimental_core::execute','--','--nocapture']),
					('eval-cli',['cargo','+1.98.1','test','--locked','--test','eval_sliced','--','--nocapture']),
					('session-witness',['cargo','+1.98.1','test','--locked','-p','rnx','--lib','routing_inventory::retained_function_crosses_programs_with_session_wiring_pending','--','--nocapture'])]
		else:
			from cross_program_miri import run
			r.update(source=identity, head=child_result.head(ROOT), sharded_miri=run(out))
			commands=[]
		r.update(source=identity,head=child_result.head(ROOT),environment=child_result.environment(env),commands=commands)
		for name,cmd in commands:
			assert child_result.source(ROOT)==identity,'source changed'
			start=time.monotonic();log=out/(name+'.log')
			with log.open('x') as f:job=child_result.run(cmd,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT,directory=out/(name+'-child'),timeout=10800)
			counts=re.findall(r'test result: ok\. (\d+) passed;',log.read_text())
			row={'name':name,'exit':job.returncode,'validation':job.validation,'positive_tests':sum(map(int,counts)),'seconds':time.monotonic()-start,'log_sha256':child_result.digest(log)}
			row['status']='PASS' if job.returncode==0 and row['positive_tests']>0 and child_result.source(ROOT)==identity else 'FAIL'
			child_result.atomic_json(out/(name+'.json'),row);r['checks'].append(row)
			if row['status']!='PASS':raise RuntimeError(name+' failed')
		r['status']='PASS'
	except Exception:r['error']=traceback.format_exc()
	finally:
		child_result.publish(out/'RESULT.json',r);print(r['status'],out/'RESULT.json')
	raise SystemExit(r['status']!='PASS')

if __name__=='__main__':main()
