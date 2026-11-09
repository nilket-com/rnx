"""Focused resumable invocation controls with durable terminal receipts."""
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
	p.add_argument('--scope',choices=('behavior','miri'),required=True)
	p.add_argument('--out',type=Path,required=True)
	a=p.parse_args();out=a.out.resolve();out.mkdir(parents=True,exist_ok=False)
	r={'status':'FAIL','scope':a.scope,'checks':[]}
	try:
		identity=child_result.source(ROOT)
		env=dict(os.environ,CARGO_BUILD_JOBS='4',CARGO_INCREMENTAL='0')
		if a.scope=='behavior':
			commands=[('token-and-executor',['cargo','+1.98.1','test','--locked','-p','rune','--profile','engine-dev','--features','native-code,fmt,workspace','--lib','experimental_core::execute','--','--nocapture']),('eval-cli',['cargo','+1.98.1','test','--locked','--test','eval_sliced','--','--nocapture'])]
		else:
			m=json.loads((ROOT/'rune/tools/xtask/manifests/miri.json').read_text())
			env['MIRIFLAGS']='-Zmiri-strict-provenance'
			commands=[('miri-sliced',['cargo','+'+m['toolchain'],'miri','test','--locked','-p','rune','--profile','engine-dev','--features',m['features'],'--lib','experimental_core::execute::sliced::sliced_tests','--','--nocapture'])]
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
