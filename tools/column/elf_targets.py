"""Minimal ELF64 little-endian target reader for the x86-64 identity diagnostic."""
import struct
import subprocess


class Elf:
	def __init__(self, path):
		self.data = path.read_bytes()
		assert self.data[:6] == b'\x7fELF\x02\x01'
		assert struct.unpack_from('<H', self.data, 18)[0] == 62
		phoff, shoff = struct.unpack_from('<QQ', self.data, 32)
		phsize, phnum, shsize, shnum = struct.unpack_from('<HHHH', self.data, 54)
		self.loads = []
		for i in range(phnum):
			t, _, off, addr, _, size, _, _ = struct.unpack_from('<IIQQQQQQ', self.data, phoff+i*phsize)
			if t == 1: self.loads.append((addr, size, off))
		self.relocations = {}
		for i in range(shnum):
			_, typ, _, _, off, size, _, _, _, entry = struct.unpack_from('<IIQQQQIIQQ', self.data, shoff+i*shsize)
			if typ != 4: continue
			assert entry == 24
			for pos in range(off, off+size, entry):
				addr, info, value = struct.unpack_from('<QQq', self.data, pos)
				if info & 0xffffffff == 8: self.relocations[addr] = value
		self.symbols = {}
		for line in subprocess.check_output(['nm', '-C', '--defined-only', str(path)], text=True).splitlines():
			parts = line.split(maxsplit=2)
			if len(parts) == 3:
				self.symbols.setdefault(int(parts[0], 16), []).append(parts[2])

	def read(self, address, size):
		for addr, length, off in self.loads:
			if addr <= address and address+size <= addr+length:
				return self.data[off+address-addr:off+address-addr+size]
		raise ValueError(f'unmapped target {address:x}+{size}')

	def pointer(self, address):
		return self.relocations.get(address, struct.unpack('<Q', self.read(address, 8))[0])

	def code(self, address):
		assert address in self.symbols, f'unresolved code target {address:x}'
		return sorted(self.symbols[address])

	def location(self, address):
		length, line, column = struct.unpack('<QII', self.read(address+8, 16))
		assert 0 < length < 4096 and line > 0 and column > 0, 'unrecognized location literal'
		return dict(file=self.read(self.pointer(address), length).decode(), line=line, column=column)
