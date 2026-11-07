"""Conservative wall estimate from calibration receipts, not performance results."""
import argparse
from collections import defaultdict
import json
from pathlib import Path
from durable import atomic,digest,reuse
from matrix import cells
from fixtures import SPECS,LAYOUTS


def estimate(receipt):
	data=json.loads(receipt.read_text());assert data['status']=='PASS' and data['mode']=='calibration'
	wall=defaultdict(list)
	for e in data['receipts']:
		p=Path(e['path']);assert digest(p)==e['sha256']
		r=json.loads(p.read_text());reuse(p.parent,r['identity'])
		c=r['identity']['cell']
		wall[c['engine'],c['rows'],c['threads']].append(r['process_wall_ns']/1e9)
	# Use slowest observed shape/layout/panel for each engine/size/thread bucket.
	cost={k:max(v) for k,v in wall.items()}
	scenarios=[]
	for label,native_sizes,native_samples in [('approved',[10000000,100000000],5),('native100M',[100000000],5),('native100M-three',[100000000],3)]:
		total=2400.0;counts=defaultdict(int);detail=defaultdict(float)
		for n in (10000000,100000000):
			for t in ('single','default'):
				for c in cells(n,t,list(SPECS),list(LAYOUTS)):
					if c['panel']=='native' and n not in native_sizes:continue
					count=5 if c['panel']=='equivalent' else native_samples
					key=c['engine'],n,t
					assert key in cost, ('missing calibration bucket',key)
					value=cost[key]*count;total+=value;counts[c['panel']]+=count;detail[c['engine']]+=value
		scenarios.append(dict(policy=label,counts=dict(counts),estimated_seconds=total,estimated_hours=total/3600,
			engine_seconds=dict(detail),cooldown_seconds=2400))
	return dict(status='PASS',calibration_sha256=digest(receipt),scenarios=scenarios,
		buckets=[dict(engine=k[0],rows=k[1],threads=k[2],observed_process_seconds=v,estimate_per_sample_seconds=max(v)) for k,v in sorted(wall.items())],
		limitations='Estimate from selected shapes, max observed per bucket; includes construction, six calls and validation. Not a timing result or guarantee. No matrix is automatically launched.')


if __name__=='__main__':
	p=argparse.ArgumentParser(description=__doc__);p.add_argument('receipt',type=Path);p.add_argument('--out',type=Path,required=True)
	a=p.parse_args();atomic(a.out,estimate(a.receipt));print('PASS: '+str(a.out))
