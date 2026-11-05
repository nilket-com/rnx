"""Memory-bounded supplemental Miri admission; no runtime status polling."""
import math

GIB = 1024 ** 3


def admit(cpus, available, ancestor_headroom, measured_peak, requested=None):
	if min(cpus, available, ancestor_headroom, measured_peak) <= 0:
		raise ValueError('positive resource observations required')
	# Round observed peak up to GiB, then reserve another 25% for process-tree
	# overhead and workload variability. RSS is not an aggregate cgroup peak.
	reservation = math.ceil(measured_peak / GIB) * GIB * 5 // 4
	ceiling = min(available, ancestor_headroom) - 2 * GIB
	workers = min(cpus, ceiling // reservation)
	if requested is not None:
		if requested <= 0:
			raise ValueError('positive worker override required')
		workers = min(workers, requested)
	if workers < 1:
		raise ValueError('insufficient headroom for one interpreter')
	return dict(workers=workers, memory_max_bytes=ceiling,
		per_worker_reservation_bytes=reservation, measured_peak_rss_bytes=measured_peak,
		reserve_bytes=2 * GIB, memory_swap_max_bytes=0)
