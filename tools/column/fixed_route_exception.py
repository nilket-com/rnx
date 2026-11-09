"""ADR0237 exact, candidate-bound exception; never a general performance band."""
import argparse
import json
import hashlib
from fractions import Fraction
from pathlib import Path
from child_result import atomic_json, digest

POLICY = Path(__file__).with_name('fixed_route_0237.json')
PARTITIONS = ('rnx_functions', 'other_functions')

def differences(a, b):
	return {k:b.get(k,0)-a.get(k,0) for k in sorted(a.keys()|b.keys()) if b.get(k,0)!=a.get(k,0)}

def vector_pin(inventory, delta):
	# Canonical UTF-8 JSON binds both zero-delta membership and nonzero changes.
	payload={'inventory':sorted(inventory),'delta':delta}
	encoded=json.dumps(payload,sort_keys=True,separators=(',',':'),ensure_ascii=True).encode('utf-8')
	return {'sha256':hashlib.sha256(encoded).hexdigest(), 'function_count':len(inventory), 'delta_Ir':sum(delta.values())}

def evaluate(records, binaries, policy):
	if policy.get('schema') != 2: raise ValueError('unsupported policy schema')
	if binaries != policy['binaries']:
		raise ValueError('wrong measured binary pair')
	index = {}
	for r in records:
		key = (r['kind'], r['policy'], r['n'], r['role'])
		if key in index or r['status'] != 'PASS': raise ValueError('duplicate or failed capture')
		index[key] = r
	expected = {(r['kind'],r['policy'],r['n'],role) for r in policy['cases'] for role in ('baseline','candidate')}
	if set(index) != expected: raise ValueError('unlisted or missing workload/size/role')
	for case in policy['cases']:
		key=(case['kind'],case['policy'],case['n'])
		a,b=[index[*key,role]['accounting'] for role in ('baseline','candidate')]
		for value in (a,b):
			if value['rnx_Ir'] != sum(value['rnx_functions'].values()): raise ValueError('program vector sum mismatch')
			if value['other_Ir'] != sum(value['other_functions'].values()): raise ValueError('unknown vector sum mismatch')
			if value['total_Ir'] != value['rnx_Ir']+value['other_Ir']+value['dynamic_Ir']: raise ValueError('partition sum mismatch')
		for part in PARTITIONS:
			actual=vector_pin(a[part].keys()|b[part].keys(),differences(a[part],b[part]))
			if actual != case[part]: raise ValueError(f'unapproved function inventory or delta: {key} {part}')
		if b['dynamic_Ir']-a['dynamic_Ir'] != case['dynamic_delta']: raise ValueError(f'system capture changed: {key}')
	# Display two-point formulas only; authorize no extrapolated size or workload.
	formulas=[]
	for kind,mode in sorted({(r['kind'],r['policy']) for r in policy['cases']}):
		ds=[index[kind,mode,n,'candidate']['accounting']['rnx_Ir']-index[kind,mode,n,'baseline']['accounting']['rnx_Ir'] for n in (1,5)]
		slope=Fraction(ds[1]-ds[0],4)
		formulas.append({'kind':kind,'policy':mode,'delta_n1':ds[0],'delta_n5':ds[1],'per_n_step':str(slope),'constant':str(ds[0]-slope)})
	return {'status':'PASS','scope':'0237 measured pair and listed cases only; remaining gates required; publication not authorized','formulas':formulas}

def load(capture, pair):
	build=json.loads(pair.read_text())
	if build['status']!='PASS': raise ValueError('build failed')
	binaries={}
	for role,row in zip(('baseline','candidate'),build['builds'],strict=True):
		if digest(row['binary'])!=row['sha256']:raise ValueError('binary changed')
		binaries[role]=row['sha256']
	data=json.loads(capture.read_text())
	if data.get('error'):raise ValueError('capture incomplete')
	records=[]
	for path in data['records']:
		r=json.loads(Path(path).read_text())
		if digest(r['profile'])!=r['profile_sha256']:raise ValueError('profile changed')
		from ir_accounting import account
		role=r['role']; binary=build['builds'][('baseline','candidate').index(role)]['binary']
		if account(r['profile'],binary)!=r['accounting']:raise ValueError('profile accounting mismatch')
		records.append(r)
	return records,binaries

def main():
	p=argparse.ArgumentParser(description=__doc__)
	p.add_argument('--capture',type=Path,required=True);p.add_argument('--pair',type=Path,required=True);p.add_argument('--out',type=Path,required=True)
	a=p.parse_args();result={'status':'FAIL'}
	try:
		records,binaries=load(a.capture,a.pair)
		result=evaluate(records,binaries,json.loads(POLICY.read_text()))
		result.update(capture_sha256=digest(a.capture),pair_sha256=digest(a.pair),policy_sha256=digest(POLICY))
	except Exception as error: result['error']=repr(error)
	atomic_json(a.out,result);print(result['status'],a.out)
	raise SystemExit(result['status']!='PASS')

if __name__=='__main__':main()
