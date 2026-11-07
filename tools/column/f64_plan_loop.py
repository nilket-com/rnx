"""Fail closed on scalar primary arithmetic loops, even with vectorized tails."""
import re


def primary_loop(assembly, *, driver="f64_plan"):
	# Keep the existing F64-plan policy; predicates have their own reviewed
	# four-lane / 32-byte packed arithmetic shape.
	minimum_stride = {"f64_plan": 64, "predicate": 32}[driver]
	instructions = []
	for line in assembly.splitlines():
		m = re.match(r'^\s*([0-9a-f]+):\s+([a-z][a-z0-9.]*)\s*(.*)$', line)
		if m: instructions.append((int(m[1],16),m[2],m[3]))
	loops = []
	for address, op, args in instructions:
		if not op.startswith('j') or op == 'jmp': continue
		m = re.match(r'([0-9a-f]+)\s+<',args)
		if not m or int(m[1],16) >= address: continue
		target = int(m[1],16)
		body = [row for row in instructions if target <= row[0] <= address]
		if any(x[1] in ('addsd','subsd','mulsd','vaddsd','vsubsd','vmulsd','addpd','subpd','mulpd','vaddpd','vsubpd','vmulpd') for x in body):
			loops.append(dict(start=target,end=address,instructions=body))
	if not loops: return dict(status='FAIL',reason='no arithmetic loop')
	primary = min(loops,key=lambda x:(x['start'],x['end']))
	packed = [x for x in primary['instructions'] if x[1] in ('addpd','subpd','mulpd','vaddpd','vsubpd','vmulpd')]
	lanes = sum(8 if '%zmm' in x[2] else 4 if '%ymm' in x[2] else 2 for x in packed)
	strides = []
	for row in primary['instructions']:
		if row[1] not in ('add', 'sub'): continue
		m = re.fullmatch(r'\$(0x[0-9a-f]+|[0-9]+),(%[a-z0-9]+)', row[2])
		if not m: continue
		value = int(m[1], 0)
		if value >= 1 << 63: value -= 1 << 64
		step = value if row[1] == 'add' else -value
		reg = m[2]
		# Convert an index increment to bytes using its actual addressing scale.
		scales = {int(m[1]) for insn in primary['instructions']
			for m in re.finditer(re.escape(reg) + r',([1248])\)', insn[2])}
		if len(scales) != 1: continue
		stride = step * next(iter(scales))
		if stride > 0: strides.append(dict(instruction=row, bytes=stride))
	agree = [x for x in strides if x['bytes'] % minimum_stride == 0 and x['bytes'] == lanes * 8]
	bounds_branches = [x for x in primary['instructions'] if x[1] in ('jae', 'ja', 'jb', 'jbe')
		and (m := re.match(r'([0-9a-f]+)\s+<', x[2]))
		and not primary['start'] <= int(m[1], 16) <= primary['end']]
	return dict(status='PASS' if packed and agree and (driver != "predicate" or not bounds_branches) else 'FAIL', primary=primary,
		driver=driver, bounds_branches=bounds_branches,
		packed=packed, packed_lanes=lanes, strides=strides, matched_strides=agree,
		reason=f'primary packed arithmetic lane count must equal byte stride, a multiple of {minimum_stride}')
