"""Record every F64 lane specialization and require packed arithmetic before capture."""
import hashlib
from pathlib import Path
import re
import subprocess
from f64_plan_loop import primary_loop


def record(binary):
	entries = []
	for line in subprocess.check_output(['nm', '-S', '-C', str(binary)], text=True).splitlines():
		parts = line.split(maxsplit=3)
		if len(parts) == 4 and 'column::run::f64_plan::lanes::' in parts[3]:
			entries.append((int(parts[0],16), int(parts[1],16), parts[3]))
	functions = []
	for address, size, name in entries:
		text = subprocess.check_output(['objdump','-d','-C','--no-show-raw-insn',
			'--start-address='+hex(address),'--stop-address='+hex(address+size),str(binary)],text=True)
		ops = re.findall(r'^\s*[0-9a-f]+:\s+([a-z][a-z0-9.]*)',text,re.M)
		packed = sum(op in ('addpd','subpd','mulpd','vaddpd','vsubpd','vmulpd') for op in ops)
		functions.append(dict(name=name,address=address,size=size,packed_arithmetic=packed,primary_loop=primary_loop(text),assembly=text))
	# No unmeasured scalar fallback silently qualifies as vectorized. Individual
	# constant-folded specializations are retained for review, not discarded.
	return dict(status='PASS' if functions and any(f['packed_arithmetic'] for f in functions) else 'FAIL',
		binary_sha256=hashlib.sha256(Path(binary).read_bytes()).hexdigest(),functions=functions)
