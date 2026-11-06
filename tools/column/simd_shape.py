#!/usr/bin/env python3
"""Fail closed when protected SIMD instruction shapes change before capture."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

NAMES = ('add_constant', 'fused', 'add_pair', 'zero_null')
PREFIX = 'rune::experimental_column_primitives::simd::'


def shapes(text, names=NAMES):
	functions = {}
	current = None
	for line in text.splitlines():
		header = re.match(r'^([0-9a-f]+) <(.+)>:$', line)
		if header:
			current = header[2]
			if not any(current == PREFIX + name or re.fullmatch(re.escape(PREFIX + name) + r'::h[0-9a-f]+', current) for name in names):
				current = None
				continue
			if current in functions:
				raise ValueError('duplicate symbol: ' + current)
			functions[current] = []
			continue
		insn = re.match(r'^\s*([0-9a-f]+):\s+([a-z][a-z0-9.]*)\s*(.*?)\s*$', line)
		if current and insn:
			functions[current].append((int(insn[1], 16), insn[2], insn[3]))
	result = {}
	for name in names:
		matches = [rows for symbol, rows in functions.items() if symbol == PREFIX + name or re.fullmatch(re.escape(PREFIX + name) + r'::h[0-9a-f]+', symbol)]
		if len(matches) != 1 or not matches[0]:
			raise ValueError('missing/ambiguous protected symbol: ' + name)
		rows = matches[0]
		addresses = {address: i for i, (address, _, _) in enumerate(rows)}
		normal = []
		for _, op, operands in rows:
			# Relocations may change; preserve branch destinations within the
			# function as instruction indices, and external symbol identities.
			if op.startswith('j') or op.startswith('call'):
				m = re.match(r'^([0-9a-f]+) <(.+)>$', operands)
				if m:
					address = int(m[1], 16)
					operands = ('local:' + str(addresses[address])) if address in addresses else re.sub(r'::h[0-9a-f]+', '', m[2])
			if '# ' in operands:
				code, annotation = operands.split('# ', 1)
				if '(%rip)' in code:
					code = re.sub(r'-?0x[0-9a-f]+\(%rip\)', 'RELOC(%rip)', code)
				operands = code.strip()
			normal.append((op, operands))
		result[name] = normal
	return result


def compare(a, b, names=NAMES):
	return {name: {'equal': a[name] == b[name], 'baseline_instructions': len(a[name]), 'candidate_instructions': len(b[name])} for name in names}


def binary_shapes(binary, names=NAMES):
	# Resolve nm aliases by address/extent; objdump may choose any alias label.
	entries = {}
	for line in subprocess.check_output(['nm', '-S', '-C', str(binary)], text=True).splitlines():
		parts = line.split(maxsplit=3)
		if len(parts) == 4 and parts[3] in {PREFIX + name for name in names}:
			if parts[3] in entries:
				raise ValueError('ambiguous protected symbol: ' + parts[3])
			entries[parts[3]] = (int(parts[0], 16), int(parts[1], 16))
	chunks = []
	for name in names:
		address, size = entries[PREFIX + name]
		if size <= 0:
			raise ValueError('empty symbol: ' + name)
		text = subprocess.check_output(['objdump', '-d', '-C', '--no-show-raw-insn',
			'--start-address=' + hex(address), '--stop-address=' + hex(address + size), str(binary)], text=True)
		lines = text.splitlines()
		start = next(i for i, line in enumerate(lines) if line.startswith(f'{address:016x} <'))
		lines[start] = f'{address:016x} <{PREFIX + name}>:'
		chunks.append('\n'.join(lines[start:]))
	return shapes('\n'.join(chunks), names)


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--baseline', required=True, type=Path)
	p.add_argument('--candidate', required=True, type=Path)
	p.add_argument('--out', required=True, type=Path)
	a = p.parse_args()
	r = {'status': 'FAIL', 'binaries': {}}
	try:
		parsed = []
		for role in ['baseline', 'candidate']:
			binary = getattr(a, role)
			r['binaries'][role] = {'path': str(binary), 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()}
			parsed.append(binary_shapes(binary))
		r['shapes'] = dict(zip(['baseline', 'candidate'], parsed))
		r['comparison'] = compare(*parsed)
		if all(row['equal'] for row in r['comparison'].values()):
			r['status'] = 'PASS'
	except Exception as error:
		r['error'] = repr(error)
	a.out.parent.mkdir(parents=True, exist_ok=True)
	with a.out.open('x') as f:
		json.dump(r, f, indent=2)
		f.write('\n')
	print(r['status'], a.out)
	return 0 if r['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
