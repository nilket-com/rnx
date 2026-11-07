"""Caller-edge controls for removed F64 initialization and staging."""
import re
from collections import defaultdict
from pathlib import Path

def edges(path):
	lines=Path(path).read_text().splitlines();names={}
	for l in lines:
		m=re.fullmatch(r'c?fn=\((\d+)\) (.+)',l)
		if m:names[m[1]]=m[2]
	cur=callee=None;pending=None;result=defaultdict(lambda:[0,0])
	for l in lines:
		m=re.fullmatch(r'(c?fn)=\((\d+)\)(?: .+)?',l)
		if m:
			if m[1]=='fn':cur=m[2]
			else:callee=m[2]
		elif l.startswith('calls='):pending=int(l.split()[0][6:])
		elif pending is not None and re.match(r'^(0x[0-9a-f]+|[+-]?\d+|\*)\s',l):
			e=result[names.get(cur,cur),names.get(callee,callee)];e[0]+=pending;e[1]+=int(l.split()[-1]);pending=None
	return [{'caller':a,'callee':b,'calls':v[0],'inclusive_Ir':v[1]} for (a,b),v in result.items()]

def check(records):
	failures = []
	for record in records:
		if record['kind'] != 'f64chain':
			continue
		for call, cost in enumerate(record['costs']):
			rows = edges(cost['profile'])
			fill = [e for e in rows if 'filled::<f64>' in e['caller'] and 'memset' in e['callee'] and e['calls'] > 0]
			case = dict(role=record['role'], rows=record['rows'], layout=record['layout'], call=call)
			if (record['role'] == 'baseline') != bool(fill):
				failures.append(dict(case, rule='output fill edge presence', edges=fill))
			if record['role'] == 'candidate':
				for e in rows:
					if 'f64_plan::run::' in e['caller'] and 'memset' in e['callee'] and e['calls'] > 1:
						failures.append(dict(case, rule='repeated driver initialization', edge=e))
	return failures
