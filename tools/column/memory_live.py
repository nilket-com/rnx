"""Limited-unit cache/anonymous control, with fixed phase reads and owned files."""
import argparse
import mmap
import os
from pathlib import Path
import traceback

from child_result import atomic_json
from ir_resources import resources, fixed_budget, memory
from owned_build import OwnedBuild

GIB = 1024**3


def run(out):
	out.mkdir(exist_ok=False)
	result = dict(status='FAIL', observations=[])
	try:
		with OwnedBuild(out, 'payload') as owner:
			owner.path.mkdir()
			result['observations'].append(dict(phase='before', reading=resources()))
			with (owner.path/'clean-file').open('xb') as stream:
				block = bytes(8*1024**2)
				for _ in range(2*GIB//len(block)): stream.write(block)
				stream.flush(); os.fsync(stream.fileno())
			del block
			cache = resources()
			raw = min([cache['mem_available_bytes']] + [limit['raw_headroom_bytes']
				for group in cache['groups'] for limit in group['limits'].values() if limit is not None])
			result['observations'].append(dict(phase='clean-cache', reading=cache, raw_headroom_bytes=raw,
				workers=fixed_budget(cache)))
			if raw >= 4*GIB: raise ValueError('cache fixture did not expose the raw-accounting refusal')
			if cache['binding_limit'] != 'memory.high': raise ValueError('finite memory.high did not bind live control')
			held = mmap.mmap(-1, 5*GIB//2, flags=mmap.MAP_PRIVATE|mmap.MAP_ANONYMOUS)
			try:
				held[::4096] = b'x'*(5*GIB//2//4096)
				anonymous = resources()
				try: fixed_budget(anonymous)
				except memory.InsufficientMemory as error:
					result['observations'].append(dict(phase='anonymous-held', reading=anonymous, refusal=error.numbers))
				else: raise ValueError('real anonymous pressure did not refuse')
			finally: held.close()
			atomic_json(out/'OBSERVATIONS.json', result['observations'])
		result.update(status='PASS', release=owner.result)
	except Exception:
		result['error'] = traceback.format_exc()
	finally:
		atomic_json(out/'RESULT.json', result)
		print(result['status'], out/'RESULT.json', flush=True)
	return result['status'] != 'PASS'


if __name__ == '__main__':
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out', type=Path, required=True)
	raise SystemExit(run(p.parse_args().out.absolute()))
