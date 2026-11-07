"""Pin reviewed executed predicate shapes; relocations alone may differ."""
import hashlib
import json
from pathlib import Path
import re
from f64_plan_loop import primary_loop

PREFIX = 'rune::experimental_core::column::run::predicate::'


def normalize(assembly):
	rows = []
	for line in assembly.splitlines():
		m = re.match(r'^\s*([0-9a-f]+):\s+([a-z][a-z0-9.]*)\s*(.*?)\s*$', line)
		if m: rows.append((int(m[1], 16), m[2], m[3]))
	if not rows: raise ValueError('empty disassembly')
	addresses = {a: i for i, (a, _, _) in enumerate(rows)}
	result = []
	for _, op, operands in rows:
		if op.startswith(('j', 'call')):
			m = re.match(r'^([0-9a-f]+) <(.+)>$', operands)
			if m:
				a = int(m[1], 16)
				operands = 'local:' + str(addresses[a]) if a in addresses else re.sub(r'::h[0-9a-f]+', '', m[2])
		if '# ' in operands:
			operands = re.sub(r'-?0x[0-9a-f]+\(%rip\)', 'RELOC(%rip)', operands.split('# ', 1)[0]).strip()
		result.append([op, operands])
	return result


def check(shape, snapshot):
	functions = {}
	for f in shape['functions']:
		if f['name'] in functions: raise ValueError('duplicate shape')
		functions[f['name']] = f
	failures = []
	for name, expected in snapshot['functions'].items():
		f = functions.get(name)
		if f is None:
			failures.append(dict(name=name, rule='missing reviewed specialization'))
			continue
		p = Path(f['assembly'])
		if hashlib.sha256(p.read_bytes()).hexdigest() != f['sha256']:
			raise ValueError('assembly digest mismatch')
		assembly = p.read_text()
		if normalize(assembly) != expected['instructions']:
			failures.append(dict(name=name, rule='reviewed instruction snapshot changed'))
		if '::arithmetic::' in name and primary_loop(assembly, driver='predicate')['status'] != 'PASS':
			failures.append(dict(name=name, rule='packed arithmetic primary loop'))
	return dict(status='FAIL' if failures else 'PASS', failures=failures,
		binary_sha256=shape['binary_sha256'], reviewed=sorted(snapshot['functions']))


def executed(records, shape, snapshot):
	from boolean_vectors import vector
	verdict = check(shape, snapshot)
	hits = set()
	for record in records:
		if record['role'] != 'candidate': continue
		for cost in record['costs']:
			hits.update(name for name, (ir, calls) in vector(cost).items()
				if name.startswith(PREFIX) and ('::comparisons::' in name or '::arithmetic::' in name) and (ir or calls))
	unknown = hits - snapshot['functions'].keys()
	if unknown or not hits:
		verdict['failures'].append(dict(rule='unreviewed or missing executed kernels', unknown=sorted(unknown)))
	verdict['status'] = 'FAIL' if verdict['failures'] else 'PASS'
	verdict['executed'] = sorted(hits)
	return verdict
