"""Compare initialized F64 SIMD symbol bytes, with verbatim disassembly on mismatch."""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

PREFIX = 'rune::experimental_column_primitives::simd::'
NAMES = ('floats', 'float_pair', 'masked_float_constant', 'masked_float_pair')


def selected(name):
	return any(name == PREFIX + n or name.startswith(PREFIX + n + '::<') for n in NAMES)


def extract(binary):
	symbols = {}
	for line in subprocess.check_output(['nm', '-S', '-C', '--defined-only', str(binary)], text=True).splitlines():
		p = line.split(maxsplit=3)
		if len(p) != 4 or p[2] not in ('t', 'T', 'w', 'W') or not selected(p[3]):
			continue
		address, size = int(p[0], 16), int(p[1], 16)
		assert size > 0 and p[3] not in symbols, 'empty or duplicate symbol'
		assembly = subprocess.check_output(['objdump', '-d', '-C', '--insn-width=16',
			f'--start-address={address}', f'--stop-address={address+size}', str(binary)], text=True)
		data = bytearray()
		for row in assembly.splitlines():
			m = re.match(r'^\s*[0-9a-f]+:\s*((?:[0-9a-f]{2} )+)', row)
			if m: data.extend(bytes.fromhex(m[1]))
		assert len(data) == size, (p[3], size, len(data))
		symbols[p[3]] = dict(address=address, size=size, sha256=hashlib.sha256(data).hexdigest(), bytes=data.hex(), disassembly=assembly)
	assert symbols and any('masked_float' in n for n in symbols), 'empty protected inventory'
	return symbols


def evaluate(a, b):
	differences = {n: dict(baseline=a.get(n), candidate=b.get(n)) for n in a.keys() | b.keys()
		if n not in a or n not in b or a[n]['bytes'] != b[n]['bytes']}
	return dict(status='FAIL' if differences else 'PASS', differences=differences,
		baseline_symbols=list(a), candidate_symbols=list(b),
		note='Raw symbol bytes, no relocation/literal normalization; addresses/disassembly retained for attribution.')


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--baseline', required=True, type=Path)
	p.add_argument('--candidate', required=True, type=Path)
	p.add_argument('--out', required=True, type=Path)
	p.add_argument('--resolve', action='store_true')
	p.add_argument('--baseline-source')
	p.add_argument('--candidate-source')
	a = p.parse_args()
	before, after = extract(a.baseline), extract(a.candidate)
	r = evaluate(before, after)
	if a.resolve:
		assert a.baseline_source and a.candidate_source
		r = resolved_evaluate(before, after, a.baseline, a.candidate, a.baseline_source, a.candidate_source)
	a.out.parent.mkdir(parents=True, exist_ok=True)
	a.out.write_text(json.dumps(r, indent=2) + '\n')
	print(r['status'], a.out)
	return r['status'] != 'PASS'




def resolve(symbol, elf):
	"""Fail closed on instruction forms outside the reviewed initialized kernels."""
	stream = []
	for line in symbol['disassembly'].splitlines():
		m = re.match(r'^\s*([0-9a-f]+):\s*((?:[0-9a-f]{2} )+)\s*\t(.+)$', line)
		if not m: continue
		address, raw, asm = int(m[1], 16), bytes.fromhex(m[2]), m[3]
		instruction = asm.split('#', 1)[0].strip()
		op = instruction.split()[0]
		resolved = None
		if '%rip' in instruction:
			target = int(re.search(r'#\s*([0-9a-f]+)', asm)[1], 16)
			if op == 'vmovdqa':
				assert '%ymm' in instruction
				resolved = dict(data=elf.read(target, 32).hex())
			elif op == 'call':
				resolved = dict(code=elf.code(elf.pointer(target)))
			elif op == 'lea':
				resolved = dict(location=elf.location(target))
			else: raise ValueError('unreviewed RIP operand: ' + asm)
			instruction = re.sub(r'-?0x[0-9a-f]+\(%rip\)', 'TARGET(%rip)', instruction)
		elif op.startswith('j'):
			target = int(instruction.split()[1], 16)
			assert symbol['address'] <= target < symbol['address']+symbol['size']
			instruction = op + ' OFFSET=' + str(target-symbol['address'])
		else:
			# Nonrelocated instructions keep bytes as well as operand text.
			resolved = dict(bytes=raw.hex())
		stream.append(dict(offset=address-symbol['address'], size=len(raw), instruction=instruction, target=resolved))
	assert sum(x['size'] for x in stream) == symbol['size']
	return stream


def resolved_evaluate(before, after, baseline, candidate, baseline_source, candidate_source):
	import difflib
	from elf_targets import Elf
	path = 'rune/crates/rune/src/experimental_column_primitives.rs'
	sources = [subprocess.check_output(['git', 'show', rev + ':' + path], text=True).splitlines()
		for rev in (baseline_source, candidate_source)]
	mapping = {}
	for block in difflib.SequenceMatcher(a=sources[0], b=sources[1], autojunk=False).get_matching_blocks():
		for i in range(block.size): mapping[block.b+i+1] = block.a+i+1
	streams = []
	locations = []
	for side, (binary, symbols) in enumerate(((baseline, before), (candidate, after))):
		elf = Elf(binary)
		current = {name: resolve(value, elf) for name, value in symbols.items()}
		for name, rows in current.items():
			for row in rows:
				location = (row['target'] or {}).get('location')
				if location:
					assert location['file'].endswith('/' + path), 'unexpected panic file'
					line = location['line']
					assert 1 <= line <= len(sources[side])
					mapped = mapping[line] if side else line
					assert sources[side][line-1] == sources[0][mapped-1]
					locations.append(dict(side=side, symbol=name, offset=row['offset'], raw=dict(location), baseline_line=mapped, source=sources[side][line-1]))
					row['target'] = dict(location=dict(file=path, baseline_line=mapped,
						column=location['column'], source=sources[side][line-1]))
		streams.append(current)
	differences = {n: dict(baseline=streams[0].get(n), candidate=streams[1].get(n))
		for n in streams[0].keys() | streams[1].keys() if streams[0].get(n) != streams[1].get(n)}
	return dict(status='FAIL' if differences else 'PASS', raw=evaluate(before, after),
		differences=differences, resolved_streams=streams, source_locations=locations,
		baseline_source=baseline_source, candidate_source=candidate_source,
		note='RIP data resolved to loaded bytes; GOT to actual symbol; branches to local offsets; panic sites mapped only through unchanged source lines. Raw differences retained.')


if __name__ == '__main__':
	raise SystemExit(main())
