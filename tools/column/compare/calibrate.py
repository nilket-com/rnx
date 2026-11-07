"""Runtime-budget calibration only, after full-size oracle validation."""
import argparse
import itertools
import json
from pathlib import Path
import time
import traceback
from durable import atomic,digest,lock,reuse
from fixtures import SPECS,LAYOUTS
from matrix import cells,identity,execute
from estimate import estimate


def main():
	p=argparse.ArgumentParser(description=__doc__)
	for name in ('out','rnx','lease','validation','install-receipt'):p.add_argument('--'+name,type=Path,required=True)
	a=p.parse_args();a.out=a.out.resolve();a.rnx=a.rnx.resolve();a.out.mkdir(parents=True,exist_ok=True)
	a.rows=[10000000,100000000];a.kinds=list(SPECS);a.layouts=list(LAYOUTS)
	try:
		guard=lock(a.lease);base=identity(a)
		v=json.loads(a.validation.read_text());assert v['status']=='PASS' and v['mode']=='validation' and v['identity']==base
		expected={json.dumps(c,sort_keys=True) for n,t in itertools.product(a.rows,('single','default')) for c in cells(n,t,a.kinds,a.layouts)}
		observed=[]
		for entry in v['receipts']:
			path=Path(entry['path']);assert digest(path)==entry['sha256']
			r=json.loads(path.read_text());assert reuse(path.parent,r['identity'])
			assert r['identity']==dict(base,cell=r['identity']['cell'],validation=True)
			observed.append(json.dumps(r['identity']['cell'],sort_keys=True))
		assert len(observed)==len(expected) and set(observed)==expected
		receipts=[]
		for n in a.rows:
			time.sleep(120)
			for t in ('single','default'):
				for c in cells(n,t,['f64chain','boolchain','mixedor'],a.layouts):
					key='-'.join(str(c[k]) for k in ('rows','threads','kind','layout','engine','panel'))
					d=a.out/'cases'/key;execute(a,base,c,d,False)
					receipts.append(dict(path=str(d/'RESULT.json'),sha256=digest(d/'RESULT.json')))
		atomic(a.out/'RESULT.json',dict(status='PASS',mode='calibration',identity=base,receipts=receipts))
		atomic(a.out/'ESTIMATE.json',estimate(a.out/'RESULT.json'))
	except BaseException:
		atomic(a.out/'RESULT.json',dict(status='FAIL',error=traceback.format_exc()))
		print('FAIL: '+str(a.out/'RESULT.json'));return 1
	print('PASS: '+str(a.out/'RESULT.json'));return 0


if __name__=='__main__':raise SystemExit(main())
