"""Render per-cell descriptive medians/ranges; never aggregate engines."""
import argparse
import json
from pathlib import Path
import statistics
from durable import atomic,digest,reuse
from matrix import POLICY,cells,selected
from fixtures import SPECS,LAYOUTS


def render(path):
	suite=json.loads(path.read_text())
	assert suite['status']=='PASS' and suite['mode']=='timing'
	assert suite['identity']['policy']==POLICY
	groups={}
	for entry in suite['receipts']:
		p=Path(entry['path']);assert digest(p)==entry['sha256']
		r=json.loads(p.read_text());reuse(p.parent,r['identity'])
		cell=r['identity']['cell']
		workers=[p.parent/n for n in r['artifacts'] if n.endswith('/WORKER.json')]
		assert len(workers)==1
		w=json.loads(workers[0].read_text());assert w['status']=='PASS'
		t=w['times_ns'];assert len(t)==6 and all(type(n)is int and n>=0 for n in t)
		key=tuple(cell[k] for k in ('kind','layout','rows','threads','engine','panel'))
		groups.setdefault(key,[]).append(dict(first=t[0],warm=statistics.median(t[1:]),raw_ns=t,resolution_ns=w['resolution_ns'],rss_kib=w['process_peak_rss_kib'],settings=w['settings'],condition_flags=r['condition_flags']))
	expected={tuple(c[k] for k in ('kind','layout','rows','threads','engine','panel')) for n in (10000000,100000000) for t in ('single','default') for c in cells(n,t,list(SPECS),list(LAYOUTS)) if selected(c,0)}
	assert set(groups)==expected, 'incomplete matrix coverage'
	out=[]
	for key,samples in sorted(groups.items()):
		assert len(samples)==(5 if key[-1]=='equivalent' else 3)
		assert key[-1]!='native' or key[2]==100000000
		row=dict(zip(('kind','layout','rows','threads','engine','panel'),key));row['samples']=samples
		for mode in ('first','warm'):
			values=[s[mode] for s in samples]
			row[mode+'_ms']={'median':statistics.median(values)/1e6,'min':min(values)/1e6,'max':max(values)/1e6}
		out.append(row)
	return dict(status='PASS',suite_sha256=digest(path),rows=out,
		disclosures=['Native panel is not semantically equal to rnx','Arrow serial direct kernels','rnx single-threaded','First is after preparation','Process RSS includes construction and validation','Primary five samples; secondary native three samples at100M only','rnx and Arrow measured once; reused with disclosure in both thread panels','No aggregate score; no statistical confidence claim'])


if __name__=='__main__':
	p=argparse.ArgumentParser(description=__doc__);p.add_argument('receipt',type=Path);p.add_argument('--out',type=Path,required=True)
	a=p.parse_args();atomic(a.out,render(a.receipt));print('PASS: '+str(a.out))
