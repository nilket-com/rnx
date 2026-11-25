"""Limited-unit cache/anonymous control, with bounded provisioning and owned files."""
import argparse
import mmap
import os
from pathlib import Path
import traceback

from child_result import atomic_json
from ir_resources import resources, fixed_budget, memory
from owned_build import OwnedBuild

GIB = 1024**3


def append_extent(stream, size):
	block = bytes(8*1024**2)
	for _ in range(size // len(block)):
		stream.write(block)
	stream.flush()
	os.fsync(stream.fileno())


def check_clean_cache(stream, observations):
	written = 0
	for extent in (2*GIB, GIB//2, GIB//2):
		append_extent(stream, extent)
		written += extent
		cache = resources()
		leaf = cache['groups'][0]
		high = leaf['limits']['memory.high']
		row = dict(phase='clean-cache', file_bytes=written, reading=cache)
		observations.append(row)
		if high is None or high['limit_bytes'] != 5*GIB:
			raise ValueError('live fixture requires its own 5 GiB memory.high')
		# Establish the contrast from kernel fields independently of the
		# accounting result under test. Score this same sample once.
		raw = high['limit_bytes'] - leaf['current_bytes']
		f = leaf['fields']
		clean = min(f['inactive_file'], f['file'] - f['shmem'])
		clean -= f['file_dirty'] + f['file_writeback'] + f['file_mapped']
		row.update(raw_headroom_bytes=raw, clean_inactive_bytes=clean)
		if not raw < 4*GIB <= raw + clean:
			continue
		if (cache['binding_cgroup'], cache['binding_limit']) != (leaf['group'], 'memory.high'):
			raise ValueError('own memory.high did not bind live control')
		row['workers'] = fixed_budget(cache)
		if row['workers'] != 1:
			raise ValueError('clean-cache accounting did not admit exactly one worker')
		return
	raise ValueError('cache fixture precondition unmet after 3 GiB')


def run(out):
	out.mkdir(exist_ok=False)
	result = dict(status='FAIL', observations=[])
	try:
		with OwnedBuild(out, 'payload') as owner:
			owner.path.mkdir()
			result['observations'].append(dict(phase='before', reading=resources()))
			with (owner.path/'clean-file').open('xb') as stream:
				check_clean_cache(stream, result['observations'])
			held = mmap.mmap(-1, 5*GIB//2, flags=mmap.MAP_PRIVATE|mmap.MAP_ANONYMOUS)
			try:
				held[::4096] = b'x'*(5*GIB//2//4096)
				anonymous = resources()
				try: fixed_budget(anonymous)
				except memory.InsufficientMemory as error:
					result['observations'].append(dict(phase='anonymous-held', reading=anonymous, refusal=error.numbers))
					if (error.numbers['binding_cgroup'], error.numbers['binding_limit']) != (
						anonymous['groups'][0]['group'], 'memory.high'
					):
						raise ValueError('anonymous pressure did not refuse under own memory.high') from error
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
