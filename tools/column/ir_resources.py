"""Admission limits and exact determinism checks for the instruction-count pool."""
import importlib.util
from pathlib import Path

_path = Path(__file__).resolve().parents[2]/'rune/tools/xtask/supplemental/memory_resources.py'
_spec = importlib.util.spec_from_file_location('ir_memory_resources', _path)
memory = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(memory)


def launch_budget(cpus, available, ancestor_headroom, measured_peak, reserve, *, observation=None):
	"""Declare an exact derived ceiling before systemd launches any workers."""
	if type(measured_peak) is not int or measured_peak <= 0:
		raise ValueError("invalid resource observation")
	per_case = (measured_peak * 6 + 4) // 5
	workers, ceiling = memory.capacity(len(cpus), available, ancestor_headroom, reserve, per_case, observation=observation)
	return {"detected_cpus": list(cpus), "mem_available_bytes": available,
		"ancestor_headroom_bytes": ancestor_headroom, "reserve_bytes": reserve,
		"memory_max_bytes": ceiling, "memory_swap_max_bytes": 0,
		"measured_case_peak_bytes": measured_peak, "per_case_admission_bytes": per_case,
		"workers": workers}


def worker_budget(cpus, available, cgroup_headroom, measured_peak, reserve):
	return launch_budget(cpus, available, cgroup_headroom, measured_peak, reserve)["workers"]


def resources(extra_group=None):
	return memory.observe(extra_group)


def observed_budget(observation, measured_peak, reserve):
	return launch_budget(observation['cpu_ids'], observation['mem_available_bytes'],
		observation['ancestor_headroom_bytes'], measured_peak, reserve, observation=observation)


def fixed_budget(observation):
	gib = 1024**3
	workers, _ = memory.capacity(len(observation['cpu_ids']), observation['mem_available_bytes'],
		observation['ancestor_headroom_bytes'], 2*gib, 2*gib, requested=4, observation=observation)
	return workers


def compare_determinism(serial, parallel):
	if set(serial) != set(parallel):
		raise ValueError("determinism subset incomplete")
	mismatches = []
	for key in sorted(serial):
		# Exact first/warm total instruction counts; never tolerance or normalization.
		if serial[key] != parallel[key]:
			mismatches.append({"case": key, "serial": serial[key], "parallel": parallel[key]})
	return {"status": "FAIL" if mismatches else "PASS", "mismatches": mismatches}

