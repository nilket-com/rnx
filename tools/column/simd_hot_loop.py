#!/usr/bin/env python3
"""Exact ADR 0216 eight-lane loop-count gate, separate from dynamic acceptance."""
import argparse
import json
from pathlib import Path
import re
from simd_shape import binary_shapes

EXPECTED = {'add_constant': (2, 13), 'fused': (1, 25), 'add_pair': (1, 19)}


def assess(shapes):
	result = {}
	for name, (expected_loops, instructions) in EXPECTED.items():
		rows = shapes[name]
		loops = []
		for end, (op, operand) in enumerate(rows):
			if not op.startswith('j') or not operand.startswith('local:'):
				continue
			start = int(operand.split(':')[1])
			if start >= end:
				continue
			body = rows[start:end + 1]
			# The main loop is straight-line except for its backedge and stores
			# two full vectors. Reject larger CFG cycles and scalar tail loops.
			if any(code.startswith('j') for code, _ in body[:-1]):
				continue
			stores = sum(code.startswith('vmov') and re.match(r'%ymm\d+,.*\(', operands) is not None for code, operands in body)
			stride = any(code == 'add' and operands.startswith('$0x8,') for code, operands in body)
			if stores == 2 and stride:
				loops.append({'start': start, 'end': end, 'instructions': len(body), 'body': body})
		result[name] = {'status': 'PASS' if len(loops) == expected_loops and all(x['instructions'] == instructions for x in loops) else 'FAIL', 'expected': instructions, 'loops': loops}
	return result


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--candidate', type=Path, required=True)
	p.add_argument('--composition', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args()
	r = {'status': 'FAIL', 'compositions': {}}
	try:
		for role in ['candidate', 'composition']:
			r['compositions'][role] = assess(binary_shapes(getattr(a, role)))
		if all(k['status'] == 'PASS' for role in r['compositions'].values() for k in role.values()):
			r['status'] = 'PASS'
	except Exception as error:
		r['error'] = repr(error)
	with a.out.open('x') as f:
		json.dump(r, f, indent=2)
		f.write('\n')
	print(r['status'], a.out)
	return 0 if r['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
