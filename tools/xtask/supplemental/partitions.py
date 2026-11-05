"""Validate exact libtest partitions before admitting any Miri jobs."""
from collections import Counter


def validate(inventory, partitions):
	if not inventory or len(set(inventory)) != len(inventory):
		raise ValueError('empty or duplicate subset inventory')
	if not partitions or any(not shard for shard in partitions):
		raise ValueError('empty partition')
	flat = [name for shard in partitions for name in shard]
	if Counter(flat) != Counter(inventory):
		raise ValueError('partitions must cover exact inventory once, including ignored tests')
	return [list(shard) for shard in partitions]


def balanced(inventory, durations, shard_count):
	"""Greedy measured-duration packing; never invent timings for missing tests."""
	if set(durations) != set(inventory):
		raise ValueError('complete measured durations required')
	import math
	if any(not math.isfinite(v) or v < 0 for v in durations.values()):
		raise ValueError('invalid duration')
	if not 1 <= shard_count <= len(inventory):
		raise ValueError('invalid shard count')
	shards = [[] for _ in range(shard_count)]
	loads = [0.0] * shard_count
	for name in sorted(inventory, key=lambda n: (-durations[n], n)):
		index = min(range(shard_count), key=lambda i: (loads[i], len(shards[i]), i))
		shards[index].append(name)
		loads[index] += durations[name]
	return validate(inventory, shards)
