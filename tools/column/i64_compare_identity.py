"""ADR 0233 protected kernels: exact instructions and resolved relocation targets."""
import argparse
import difflib
import hashlib
import json
from pathlib import Path
import re
import subprocess
import struct
from elf_targets import Elf
from masked_float_identity import selected as initialized_float

PREFIX = 'rune::experimental_core::column::run::predicate::'
SOURCES = ('rune/crates/rune/src/experimental_column_primitives.rs',
	'rune/crates/rune/src/experimental_core/column/predicate.rs')


def selected(name):
	return initialized_float(name) or name.startswith((PREFIX + 'comparisons::<', PREFIX + 'arithmetic::<'))


def extract(binary, elf):
	result = {}
	for line in subprocess.check_output(['nm', '-S', '-C', '--defined-only', str(binary)], text=True).splitlines():
		p = line.split(maxsplit=3)
		if len(p) != 4 or p[2] not in {'t', 'T', 'w', 'W'} or not selected(p[3]):
			continue
		address, size = int(p[0], 16), int(p[1], 16)
		assert size and p[3] not in result, 'empty or duplicate protected symbol'
		data = elf.read(address, size)
		assembly = subprocess.check_output(['objdump', '-d', '-C', '--insn-width=16',
			f'--start-address={address}', f'--stop-address={address+size}', str(binary)], text=True)
		result[p[3]] = dict(address=address, size=size, bytes=data.hex(), disassembly=assembly)
	for fragment in ('comparisons::<false,', 'comparisons::<true,', 'arithmetic::<', 'simd::masked_float'):
		assert any(fragment in n for n in result), ('missing protected family', fragment)
	return result


class UnreviewedForm(ValueError):
	"""Known parser coverage limit, not malformed data or a failed tool."""
	def __init__(self, reason, instruction):
		self.instruction = instruction
		super().__init__(reason + ': ' + instruction)


def address_comment(asm):
	"""Only objdump's trailing address comment, never a hash in a symbol."""
	depth = 0
	for i, char in enumerate(asm):
		if char == '<': depth += 1
		elif char == '>': depth = max(0, depth - 1)
		elif char == '#' and depth == 0 and i and asm[i-1].isspace():
			match = re.match(r'#\s*([0-9a-f]+)(?=\s|$)', asm[i:])
			if match: return i, int(match[1], 16)
	return None


def resolve(symbol, elf):
	rows = []
	parsed = [m for line in symbol['disassembly'].splitlines()
		if (m := re.match(r'^\s*([0-9a-f]+):\s*((?:[0-9a-f]{2} )+)\s*\t(.+)$', line))]
	# Coverage is a tool invariant even when the first instruction is unreviewed.
	assert sum(len(bytes.fromhex(m[2])) for m in parsed) == symbol['size'], 'incomplete disassembly'
	for m in parsed:
		address, raw, asm = int(m[1], 16), bytes.fromhex(m[2]), m[3]
		try:
			comment = address_comment(asm)
			instruction = (asm[:comment[0]] if comment else asm).strip()
			op = instruction.split()[0]
			target = dict(bytes=raw.hex())
			if '%rip' in instruction:
				assert comment, 'unresolved RIP target'
				addr = comment[1]
				if op in {'vmovdqa', 'vmovdqu', 'vmovaps', 'vmovups', 'movdqa', 'movdqu', 'movaps', 'movups', 'movapd', 'movupd', 'vmovapd', 'vmovupd', 'andpd'}:
					assert re.search(r',%[xy]mm\d+$', instruction), 'expected vector load'
					size = 32 if '%ymm' in instruction else 16
					target = dict(data=elf.read(addr, size).hex())
				elif op in {'vbroadcastsd', 'vbroadcastss'}:
					target = dict(data=elf.read(addr, 8 if op.endswith('sd') else 4).hex())
				elif op == 'call':
					target = dict(code=elf.code(elf.pointer(addr)))
				elif op == 'lea':
					try:
						target = dict(location=elf.location(addr))
					except AssertionError as error:
						literals = (b'assertion failed: self.len() >= len',
							b'assertion failed: out.len() >= len.div_ceil(64)')
						matches = [v for v in literals if elf.read(addr, len(v)) == v]
						if len(matches) != 1:
							raise UnreviewedForm('unreviewed address literal', asm) from error
						target = dict(assertion=matches[0].hex())
				else:
					raise UnreviewedForm('unreviewed RIP instruction', asm)
				target['encoding_prefix'] = raw[:-4].hex()
				instruction = re.sub(r'-?0x[0-9a-f]+\(%rip\)', 'TARGET(%rip)', instruction)
			elif op.startswith('j') or op == 'call':
				match = re.fullmatch(r'([0-9a-f]+) <.+>', instruction[len(op):].strip())
				if not match: raise UnreviewedForm('unreviewed indirect branch', instruction)
				addr = int(match[1], 16)
				if symbol['address'] <= addr < symbol['address'] + symbol['size']:
					target = dict(local=addr-symbol['address'])
				else:
					target = dict(code=elf.code(addr))
				width = 1 if raw[0] == 0xeb or 0x70 <= raw[0] <= 0x7f else 4
				target['encoding_prefix'] = raw[:-width].hex()
				instruction = op + ' TARGET'
			rows.append(dict(offset=address-symbol['address'], size=len(raw), instruction=instruction, target=target))
		except UnreviewedForm:
			raise
		except (AssertionError, ValueError, struct.error) as error:
			raise UnreviewedForm(str(error), asm) from error
	assert sum(r['size'] for r in rows) == symbol['size'], 'incomplete disassembly'
	return rows


def compare(before, after):
	differences = {n: dict(baseline=before.get(n), candidate=after.get(n))
		for n in before.keys() | after.keys() if before.get(n) != after.get(n)}
	return dict(status='FAIL' if differences or not before else 'PASS', differences=differences)


def evaluate(binaries, revisions):
	sources = [{p: subprocess.check_output(['git', 'show', rev + ':' + p], text=True).splitlines()
		for p in SOURCES} for rev in revisions]
	mappings = {}
	for p in SOURCES:
		mapping = {}
		for block in difflib.SequenceMatcher(a=sources[0][p], b=sources[1][p], autojunk=False).get_matching_blocks():
			for i in range(block.size):
				mapping[block.b+i+1] = block.a+i+1
		mappings[p] = mapping
	streams, raw, locations = [], [], []
	for side, binary in enumerate(binaries):
		elf = Elf(binary)
		symbols = extract(binary, elf)
		raw.append(symbols)
		current = {n: resolve(s, elf) for n, s in symbols.items()}
		for name, rows in current.items():
			for row in rows:
				loc = row['target'].get('location')
				if loc is None:
					continue
				paths = [p for p in SOURCES if loc['file'].endswith('/' + p) or loc['file'] == p]
				assert len(paths) == 1, ('unexpected panic source', loc)
				p = paths[0]
				line = loc['line']
				mapped = mappings[p][line] if side else line
				assert sources[side][p][line-1] == sources[0][p][mapped-1]
				locations.append(dict(side=side, symbol=name, raw=loc, baseline_line=mapped))
				row['target'] = dict(location=dict(file=p, line=mapped, column=loc['column'], source=sources[0][p][mapped-1]))
		streams.append(current)
	r = compare(*streams)
	r.update(raw_symbols=raw, resolved=streams, locations=locations, source_commits=revisions,
		binaries=[dict(path=str(b), sha256=hashlib.sha256(b.read_bytes()).hexdigest()) for b in binaries])
	return r


def main():
	p = argparse.ArgumentParser(description=__doc__)
	for name in ('baseline', 'candidate'):
		p.add_argument('--' + name, required=True, type=Path)
		p.add_argument('--' + name + '-source', required=True)
	p.add_argument('--out', required=True, type=Path)
	a = p.parse_args()
	try:
		r = evaluate([a.baseline, a.candidate], [a.baseline_source, a.candidate_source])
	except Exception:
		import traceback
		r = dict(status='FAIL', error=traceback.format_exc())
	a.out.parent.mkdir(parents=True, exist_ok=True)
	a.out.write_text(json.dumps(r, indent=2) + '\n')
	print(r['status'], a.out)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
