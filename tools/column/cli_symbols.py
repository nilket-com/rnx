"""Exact plain-map boundaries, excluding the consuming reuse sibling."""
import subprocess


def symbols(binary):
	raw = subprocess.check_output(["nm", str(binary)], text=True).splitlines()
	demangled = subprocess.check_output(["nm", "-C", str(binary)], text=True).splitlines()
	by_address = {}
	for line in raw:
		fields = line.split()
		if len(fields) == 3:
			by_address.setdefault(fields[0], []).append(fields[2])
	answer = {}
	for role, fragments in {
		"map": ["FunctionHandler", "::call_impl", "rnx_polars::column::script::map,"],
		"worker": ["rnx_polars::column::apply_joined::<"],
	}.items():
		matches = [line.split()[0] for line in demangled if all(f in line for f in fragments)]
		assert len(matches) == 1, (role, matches)
		names = by_address[matches[0]]
		assert len(names) == 1, (role, names)
		answer[role] = names[0]
	return answer

