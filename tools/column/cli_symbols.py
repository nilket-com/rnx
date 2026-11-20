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



def invocation_phase_symbols(binary, source):
	"""Identify the prepared entry and the seven executor bodies in the paired ELF."""
	body = source.split("\n\tfn invoke_args<")
	if len(body) != 2:
		raise ValueError("ambiguous invoke_args source")
	before, marker, after = body[1].partition("let result = (|| {")
	if (not marker or "(||" in before or not after.lstrip().startswith("if store.is_closed()")
			or "self.enter(code, store, function, args, None)" not in after.partition("'frames: loop")[0]):
		raise ValueError("unreviewed invoke_args body")
	owner = "<rune::experimental_core::execute::Executor>::invoke_args"
	cache = "rune::experimental_core::region::cache::"
	expected = set()
	for native, collections, recursion, methods in (
		(False, False, False, False),
		(True, False, False, False), (True, True, False, False),
		(True, False, True, False), (True, True, True, False),
		(True, False, True, True), (True, True, True, True),
	):
		flags = ", ".join(str(v).lower() for v in (native, collections, recursion, methods))
		program = "methods::MethodProgram" if methods else "recursion::RecursiveProgram"
		expected.add(f"{owner}::<{flags}, {cache}{program}>::{{closure#0}}")
	entry = "<rune::experimental_core::execute::PreparedInvocation>::invoke"
	raw = subprocess.check_output(["nm", "--defined-only", str(binary)], text=True)
	names = {}
	for line in raw.splitlines():
		fields = line.split(maxsplit=2)
		if len(fields) == 3:
			names.setdefault(fields[0], []).append(fields[2])
	demangled = subprocess.check_output(["nm", "-C", "--defined-only", str(binary)], text=True)
	found = {}
	for line in demangled.splitlines():
		fields = line.split(maxsplit=2)
		if len(fields) != 3:
			continue
		addr, _, name = fields
		if name == entry or (name.startswith(owner + "::<") and name.endswith(">::{closure#0}")):
			if name in found or len(names.get(addr, [])) != 1:
				raise ValueError("ambiguous invocation symbol: " + name)
			found[name] = names[addr][0]
	if set(found) != expected | {entry} or len(set(found.values())) != len(found):
		raise ValueError("unreviewed invocation specialization set")
	return found.pop(entry), found
