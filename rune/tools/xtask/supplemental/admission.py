"""Memory-bounded supplemental Miri admission; no runtime status polling."""
import math
from memory_resources import capacity

GIB = 1024 ** 3


def admit(cpus, available, ancestor_headroom, measured_peak, requested=None, *, observation=None):
	if type(measured_peak) is not int or measured_peak <= 0:
		raise ValueError('positive resource observations required')
	# Round observed peak up to GiB, then reserve another 25% for process-tree
	# overhead and workload variability. RSS is not an aggregate cgroup peak.
	reservation = math.ceil(measured_peak / GIB) * GIB * 5 // 4
	workers, ceiling = capacity(cpus, available, ancestor_headroom, 2*GIB, reservation,
		requested=requested, observation=observation)
	return dict(workers=workers, memory_max_bytes=ceiling,
		per_worker_reservation_bytes=reservation, measured_peak_rss_bytes=measured_peak,
		reserve_bytes=2 * GIB, memory_swap_max_bytes=0)


def observed_admission(observation, measured_peak, requested=None):
	return admit(len(observation['cpu_ids']), observation['mem_available_bytes'],
		observation['ancestor_headroom_bytes'], measured_peak, requested, observation=observation)
