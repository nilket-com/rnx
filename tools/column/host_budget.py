"""Build and case parallelism derived from the detected CPUs and memory, never a fixed count.

Memory is the smaller of the host's available memory and the headroom under
every finite high/max limit of this process's control groups and their
ancestors, so a final inside a systemd unit is sized by the unit's limits.
Too little memory for one worker raises InsufficientMemory."""
import os
import re

from ir_resources import memory, resources

GIB = 1024 ** 3
# Left free for the controller, the shell and Cargo's own resolver.
RESERVE_BYTES = 2 * GIB
# A rustc job on the Polars crates peaks near this; more jobs than memory allows
# would trade a faster build for an out-of-memory kill.
JOB_MEMORY_BYTES = 3 * GIB
# A native contract case runs one test thread; its interpreter children and
# fixtures stay well under this.
CASE_MEMORY_BYTES = 2 * GIB


def inherited_width(environment=None):
	"""Inside a final, its jobserver's size: the budget it measured on a quiet host.

	A stage that measured again would see the memory its neighbors use and
	shrink or refuse; the jobserver's tokens already bound them all."""
	flags = (os.environ if environment is None else environment).get('MAKEFLAGS', '')
	width = re.search(r'(?:^|\s)-j(\d+)', flags)
	return int(width.group(1)) if width and '--jobserver-auth=fifo:' in flags else None


def _workers(cpus, per_worker, observation):
	inherited = inherited_width() if observation is None else None
	if inherited: return cpus(inherited)
	o = observation if observation is not None else resources()
	workers, _ = memory.capacity(cpus(len(o['cpu_ids'])), o['mem_available_bytes'],
		o['ancestor_headroom_bytes'], RESERVE_BYTES, per_worker, observation=o)
	return workers


def build_jobs(observation=None):
	return _workers(lambda cpus: cpus, JOB_MEMORY_BYTES, observation)


# A final's stages run beside each other: page cache, test children and the
# controllers count against the same control group, and a group pushed to its
# high limit is throttled until wall-clock-bounded tests miss their deadlines.
FINAL_RESERVE_SHARE = 0.25


def final_jobs(observation=None):
	"""The whole final's job tokens: build_jobs with a quarter of its memory held back."""
	o = observation if observation is not None else resources()
	limit = min(o['mem_available_bytes'], o['ancestor_headroom_bytes'])
	reserve = max(RESERVE_BYTES, int(limit * FINAL_RESERVE_SHARE))
	workers, _ = memory.capacity(len(o['cpu_ids']), o['mem_available_bytes'], o['ancestor_headroom_bytes'],
		reserve, JOB_MEMORY_BYTES, observation=o)
	return workers


def memory_events():
	"""This process's control group memory.events counters (high, max, oom...), if readable."""
	try:
		line = next(l for l in open('/proc/self/cgroup').read().splitlines() if l.startswith('0::'))
		text = open('/sys/fs/cgroup' + line[3:] + '/memory.events').read()
		return {k: int(v) for k, v in (row.split() for row in text.splitlines())}
	except (OSError, StopIteration, ValueError):
		return None


def case_width(observation=None):
	"""Concurrent single-threaded cases: half the CPUs leave room for their children."""
	return _workers(lambda cpus: max(1, cpus // 2), CASE_MEMORY_BYTES, observation)
