"""Whole-map CK2 acceptance; capture completion alone never establishes a pass."""
import math
from cli_fixtures import KERNELS, LAYOUTS

if not __debug__:
	raise RuntimeError("acceptance requires Python assertions enabled")

POLICY_NAMES = {"multiply3": "mul3"}


def reviewed_exception(policy, provenance, kind, layout, rows, call, delta):
	if provenance is None:
		return None
	for entry in policy.get("reviewed_exceptions", []):
		if all(provenance.get(key) == entry[key] for key in ("candidate", "baseline", "attribution_sha256")) and (kind, layout, rows, call) == (entry["kind"], entry["layout"], entry["rows"], entry["call"]) and 0 < delta <= entry["max_delta_Ir"]:
			return entry
	return None


def evaluate(records, policy, provenance=None):
	expected = {(kind, n, layout, role) for kind in KERNELS for n in policy["rows"] for layout in LAYOUTS for role in ("baseline", "candidate")}
	index = {}
	for row in records:
		key = row["kind"], row["rows"], row["layout"], row["role"]
		assert key in expected and key not in index, ("unexpected or duplicate capture", key)
		assert len(row["costs"]) == 2, "first and warm call required"
		for cost in row["costs"]:
			assert type(cost["Ir"]) is int and cost["Ir"] > 0, "empty/nonintegral profile"
			assert math.isfinite(cost["Ir_per_row"])
			assert cost["Ir_per_row"] == cost["Ir"] / row["rows"], "inconsistent cost"
		index[key] = row
	assert set(index) == expected, "incomplete acceptance matrix"
	failures = []
	applied = []
	for kind in KERNELS:
		name = POLICY_NAMES.get(kind, kind)
		for n in policy["rows"]:
			for call in range(2):
				def cost(layout, role="candidate"):
					return index[kind, n, layout, role]["costs"][call]["Ir"] / n
				flat = cost("flat")
				label = {"kind": kind, "rows": n, "call": call}
				if name in policy["flat_ir_per_row_max"] and flat > policy["flat_ir_per_row_max"][name]:
					failures.append(dict(label, rule="flat ceiling", actual=flat, ceiling=policy["flat_ir_per_row_max"][name]))
				if kind in ("i64add", "axpy", "boolnot", "f64const") and flat > cost("flat", "baseline"):
					delta = index[kind, n, "flat", "candidate"]["costs"][call]["Ir"] - index[kind, n, "flat", "baseline"]["costs"][call]["Ir"]
					exception = reviewed_exception(policy, provenance, kind, "flat", n, call, delta)
					if exception is None:
						failures.append(dict(label, rule="existing flat zero growth", actual=flat, baseline=cost("flat", "baseline")))
					else:
						applied.append(dict(label, delta_Ir=delta, exception=exception))
				for layout in LAYOUTS[1:]:
					actual = cost(layout)
					ceiling = policy["nullable_boolnot_ir_per_row_max"] if kind == "boolnot" else policy["nullable_numeric_ratio_to_new_flat_max"] * flat
					if actual > ceiling:
						failures.append(dict(label, layout=layout, rule="nullable ceiling", actual=actual, ceiling=ceiling))
					if kind != "boolnot":
						previous = policy["published_nullable_ir_per_row"].get(name, cost(layout, "baseline"))
						if policy["nullable_numeric_must_improve_published"] and (actual >= previous or actual >= cost(layout, "baseline")):
							failures.append(dict(label, layout=layout, rule="strict nullable improvement", actual=actual, published=previous, paired=cost(layout, "baseline")))
	return {"status": "FAIL" if failures else "PASS", "failures": failures, "reviewed_exceptions_applied": applied, "boundary": policy["boundary"]}


def controls(policy):
	import copy
	costs = {"i64add": 2, "axpy": 3, "boolnot": 0.06, "f64add": 2, "f64const": 2, "multiply3": 10}
	records = []
	for kind in KERNELS:
		for n in policy["rows"]:
			for layout in LAYOUTS:
				for role in ("baseline", "candidate"):
					value = costs[kind] if layout == "flat" else 3 if kind == "boolnot" else costs[kind] * 4
					if role == "baseline":
						value = value + 1 if layout == "flat" else 200
					ir = round(value * n)
					records.append({"kind": kind, "rows": n, "layout": layout, "role": role, "costs": [{"Ir": ir, "Ir_per_row": ir / n} for _ in range(2)]})
	assert evaluate(records, policy)["status"] == "PASS"
	structural = [
		(records[:-1], "incomplete acceptance matrix"),
		(records + [records[0]], ("unexpected or duplicate capture", (records[0]["kind"], records[0]["rows"], records[0]["layout"], records[0]["role"]))),
	]
	for rows, expected in structural:
		try:
			evaluate(rows, policy)
		except AssertionError as error:
			assert error.args == (expected,), (error.args, expected)
		else:
			raise AssertionError("structural control unexpectedly passed")
	mutations = [
		("multiply3", "flat", 12.1, "flat ceiling"),
		("f64add", "flat", 4.1, "flat ceiling"),
		("boolnot", "alternating", 5.1, "nullable ceiling"),
		("i64add", "flat", 3.1, "existing flat zero growth"),
		("f64const", "flat", 3.1, "existing flat zero growth"),
		("axpy", "deterministic-random", 19, "nullable ceiling"),
		("multiply3", "alternating", 155.29694058, "strict nullable improvement"),
		("i64add", "flat", 0, None),
	]
	for kind, layout, value, rule in mutations:
		changed = copy.deepcopy(records)
		row = next(r for r in changed if r["kind"] == kind and r["layout"] == layout and r["role"] == "candidate")
		ir = round(value * row["rows"])
		row["costs"][0] = {"Ir": ir, "Ir_per_row": ir / row["rows"]}
		if rule is None:
			try:
				evaluate(changed, policy)
			except AssertionError as error:
				assert error.args == ("empty/nonintegral profile",), error.args
			else:
				raise AssertionError("zero profile unexpectedly passed")
			continue
		result = evaluate(changed, policy)
		assert result["status"] == "FAIL"
		assert any(f["rule"] == rule and f["kind"] == kind and f["rows"] == row["rows"] and f["call"] == 0 and f.get("layout", "flat") == layout for f in result["failures"]), (kind, layout, rule, result)
	return len(structural) + len(mutations)
