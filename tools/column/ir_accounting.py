"""User-approved ELF-object accounting: exact program work, bounded system work."""
from collections import defaultdict
from fractions import Fraction
from pathlib import Path
import re

DYNAMIC_OBJECTS = {"libc.so.6", "ld-linux-x86-64.so.2"}


def account(path, binary):
	text = Path(path).read_text()
	if not re.search(r"^positions: instr line$", text, re.M) or not re.search(r"^events: Ir$", text, re.M):
		raise ValueError("unsupported profile format")
	objects, functions = {}, {}
	for line in text.splitlines():
		m = re.fullmatch(r"c?(ob|fn)=\((\d+)\) (.+)", line)
		if m:
			kind, key, name = m.groups()
			(objects if kind == "ob" else functions)[key] = name
	own = defaultdict(int)
	obj = fn = None
	call = False
	for line in text.splitlines():
		m = re.fullmatch(r"(ob|fn)=\((\d+)\)(?: .+)?", line)
		if m:
			if m[1] == "ob":
				obj = m[2]
			else:
				fn, call = m[2], False
			continue
		if line.startswith("calls="):
			call = True
			continue
		fields = line.split()
		if fn is not None and len(fields) == 3 and re.fullmatch(r"(?:0x[0-9a-fA-F]+|[+-]?\d+|\*)", fields[0]) and re.fullmatch(r"[+-]?\d+|\*", fields[1]):
			if not call:
				own[objects[obj], functions[fn]] += int(fields[2])
			call = False
	total = int(re.search(r"^summary: (\d+)$", text, re.M)[1])
	if sum(own.values()) != total:
		raise ValueError("object self costs do not reconcile with summary")
	rnx, dynamic, other = {}, {}, {}
	for (obj, fn), cost in own.items():
		if obj != "???" and Path(obj).resolve() == Path(binary).resolve():
			rnx[fn] = rnx.get(fn, 0) + cost
		elif Path(obj).name in DYNAMIC_OBJECTS:
			dynamic[obj + "::" + fn] = cost
		else:
			# Never apply the proposed libc allowance to unidentified objects.
			other[obj + "::" + fn] = cost
	if not rnx:
		raise ValueError("measured binary has no self instructions")
	return {"total_Ir": total, "rnx_Ir": sum(rnx.values()),
		"dynamic_Ir": sum(dynamic.values()), "other_Ir": sum(other.values()),
		"rnx_functions": rnx, "dynamic_functions": dynamic, "other_functions": other}


def comparison(left, right):
	if set(left) != set(right):
		raise ValueError("comparison cases differ")
	rows = []
	for key in sorted(left):
		for index, (a, b) in enumerate(zip(left[key], right[key], strict=True)):
			deltas = {}
			for category in ("rnx_functions", "other_functions"):
				fa, fb = a[category], b[category]
				for name in fa.keys() | fb.keys():
					delta = fb.get(name, 0) - fa.get(name, 0)
					if delta:
						deltas[category + "::" + name] = delta
			rows.append({"case": key, "call": index + 1, "exact_partition_deltas": deltas,
				"dynamic_delta": b["dynamic_Ir"] - a["dynamic_Ir"]})
	return {"exact_partitions_match": not any(r["exact_partition_deltas"] for r in rows), "comparisons": rows}


def system_band(first_delta, last_delta, first_rows=10000000, last_rows=100000000):
	"""System-library candidate-minus-baseline slope and constant allowance."""
	if first_rows >= last_rows:
		raise ValueError("row counts must increase")
	slope = Fraction(last_delta - first_delta, last_rows - first_rows)
	constant = first_delta - slope * first_rows
	return {"slope_Ir_per_row": str(slope), "constant_Ir": str(constant),
		"passes": slope <= Fraction(1, 1000) and constant <= 4096}


def evaluate(records, policy, whole_map_evaluator):
	# Keep raw whole-map ceilings and improvement requirements. Only the old
	# flat zero-growth check changes to the exact program/unknown partition.
	raw = whole_map_evaluator(records, policy)
	failures = [f for f in raw["failures"] if f["rule"] != "existing flat zero growth"]
	index = {(r["kind"], r["rows"], r["layout"], r["role"]): r for r in records}
	bands = []
	sizes = sorted(policy["rows"])
	if sizes != [10000000, 100000000]:
		raise ValueError("system slope requires the approved 10M and 100M rows")
	paths = {(r["kind"], r["layout"]) for r in records}
	classification = {(kind, layout): scope for kind, layouts in policy["system_band_scope"].items() for layout, scope in layouts.items()}
	if set(classification) != paths or any(v not in ("changed", "unchanged") for v in classification.values()):
		raise ValueError("explicit changed/unchanged classification must cover exactly the matrix paths")
	for kind, layout in sorted(paths):
		for call in range(2):
			deltas = []
			for rows in sizes:
				a = index[kind, rows, layout, "baseline"]["costs"][call]["accounting"]
				b = index[kind, rows, layout, "candidate"]["costs"][call]["accounting"]
				for value in (a, b):
					if value["total_Ir"] != value["rnx_Ir"] + value["dynamic_Ir"] + value["other_Ir"]:
						raise ValueError("partition does not reconcile")
				deltas.append(b["dynamic_Ir"] - a["dynamic_Ir"])
				if classification[kind, layout] == "unchanged":
					growth = b["rnx_Ir"] + b["other_Ir"] - a["rnx_Ir"] - a["other_Ir"]
					if growth > 0:
						failures.append({"rule": "existing flat program zero growth", "kind": kind, "rows": rows, "call": call, "delta_Ir": growth})
			band = dict(system_band(*deltas), kind=kind, layout=layout, call=call, deltas=deltas, scope=classification[kind, layout])
			bands.append(band)
			if band["scope"] == "unchanged" and not band["passes"]:
				failures.append(dict(band, rule="system library growth"))
	return {"status": "FAIL" if failures else "PASS", "failures": failures,
		"system_bands": bands, "raw_whole_map_verdict": raw, "reviewed_exceptions_applied": []}
