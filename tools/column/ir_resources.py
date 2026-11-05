"""Admission limits and exact determinism checks for the instruction-count pool."""
import os
from pathlib import Path


def launch_budget(cpus, available, ancestor_headroom, measured_peak, reserve):
	"""Declare an exact derived ceiling before systemd launches any workers."""
	if not cpus or any(type(v) is not int or v <= 0 for v in (available, ancestor_headroom, measured_peak)):
		raise ValueError("invalid resource observation")
	if type(reserve) is not int or reserve < 0:
		raise ValueError("invalid reserve")
	ceiling = min(available, ancestor_headroom) - reserve
	per_case = (measured_peak * 6 + 4) // 5
	if ceiling < per_case:
		raise ValueError("insufficient memory for one case")
	return {"detected_cpus": list(cpus), "mem_available_bytes": available,
		"ancestor_headroom_bytes": ancestor_headroom, "reserve_bytes": reserve,
		"memory_max_bytes": ceiling, "memory_swap_max_bytes": 0,
		"measured_case_peak_bytes": measured_peak, "per_case_admission_bytes": per_case,
		"workers": min(len(cpus), ceiling // per_case)}


def worker_budget(cpus, available, cgroup_headroom, measured_peak, reserve):
	return launch_budget(cpus, available, cgroup_headroom, measured_peak, reserve)["workers"]


def resources(extra_group=None):
	cpus = sorted(os.sched_getaffinity(0))
	mem = dict(line.split(":", 1) for line in Path("/proc/meminfo").read_text().splitlines())
	available = int(mem["MemAvailable"].split()[0]) * 1024
	headroom = available
	unified = next(line.split(":", 2)[2] for line in Path("/proc/self/cgroup").read_text().splitlines() if line.startswith("0::"))
	root = Path("/sys/fs/cgroup")
	group = root / unified.lstrip("/")
	groups = [group]
	if extra_group:
		groups.append(root / extra_group.lstrip("/"))
	for group in groups:
		# Every ancestor limit binds; availability on the host alone is insufficient.
		while group.is_relative_to(root):
			limit = group / "memory.max"
			used = group / "memory.current"
			if limit.exists() and used.exists():
				value = limit.read_text().strip()
				if value != "max":
					headroom = min(headroom, max(0, int(value) - int(used.read_text())))
			if group == root:
				break
			group = group.parent
	return cpus, available, headroom


def compare_determinism(serial, parallel):
	if set(serial) != set(parallel):
		raise ValueError("determinism subset incomplete")
	mismatches = []
	for key in sorted(serial):
		# Exact first/warm total instruction counts; never tolerance or normalization.
		if serial[key] != parallel[key]:
			mismatches.append({"case": key, "serial": serial[key], "parallel": parallel[key]})
	return {"status": "FAIL" if mismatches else "PASS", "mismatches": mismatches}

