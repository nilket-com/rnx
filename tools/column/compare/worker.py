"""Fresh-process worker; full output checks occur outside the timed call."""
import argparse
import json
import os
from pathlib import Path
import re
import resource
import subprocess
import time
import traceback
from durable import atomic, digest
from fixtures import arrow_inputs, manifest, validate


def run(args):
	m=manifest(args.kind,args.rows,args.layout)
	atomic(args.out/'FIXTURE.json',m)
	calls=1 if args.validate else 6
	if args.engine=='rnx':
		assert args.panel=='equivalent'
		from rnx_worker import script
		program=args.out/'program.rn'; program.write_text(script(m,calls,not args.validate))
		command=[str(args.rnx),'run','--budget','100000000000',str(program)]
		log=args.out/'rnx.log'
		with log.open('w') as f:
			done=subprocess.run(command,stdout=f,stderr=subprocess.STDOUT,timeout=3600)
		assert done.returncode==0, 'rnx exit '+str(done.returncode)
		text=log.read_text()
		if args.validate:
			assert re.findall(r'^VALIDATED (\d+)$',text,re.M)==['0']
			times=[]
		else:
			matches=re.findall(r'^SAMPLE (\d+) (\d+)$',text,re.M)
			assert [int(x[0]) for x in matches]==list(range(6))
			times=[int(x[1]) for x in matches]
		return dict(status='PASS',fixture_sha256=m['sha256'],times_ns=times,
			timer='Rust Instant via monotonic_ns',resolution_ns=None,timestamp_unit_ns=1,
			settings={'binary_sha256':digest(args.rnx),'execution':'single-threaded column kernel'},
			validation='full Series.equals_missing against scalar-oracle fixture',
			process_peak_rss_kib=resource.getrusage(resource.RUSAGE_CHILDREN).ru_maxrss)
	from engines import prepare
	table=arrow_inputs(m)
	call,export,settings=prepare(args.engine,args.kind,table,args.panel=='native',args.threads)
	construction_rss=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss
	times=[]; checks=[]
	for _ in range(calls):
		if args.validate:
			result=call()
		else:
			start=time.monotonic_ns(); result=call(); elapsed=time.monotonic_ns()-start
			times.append(elapsed)
		checks.append(validate(m,export(result),args.panel=='native'))
		del result
	return dict(status='PASS',fixture_sha256=m['sha256'],times_ns=times,validation=checks,
		timer='Python monotonic_ns',resolution_ns=int(time.get_clock_info('monotonic').resolution*1e9),
		settings=settings,construction_peak_rss_kib=construction_rss,
		process_peak_rss_kib=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss)


def main():
	p=argparse.ArgumentParser(description=__doc__)
	for name in ('kind','layout','engine','panel','threads'): p.add_argument('--'+name,required=True)
	p.add_argument('--rows',type=int,required=True); p.add_argument('--out',type=Path,required=True)
	p.add_argument('--rnx',type=Path,required=True); p.add_argument('--validate',action='store_true')
	a=p.parse_args(); a.out.mkdir(parents=True,exist_ok=True)
	try: result=run(a)
	except BaseException: result=dict(status='FAIL',error=traceback.format_exc())
	atomic(a.out/'WORKER.json',result)
	print(result['status']+': '+str(a.out/'WORKER.json'))
	return 0 if result['status']=='PASS' else 1


if __name__=='__main__': raise SystemExit(main())
