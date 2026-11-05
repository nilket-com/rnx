"""ADR0211 migration: non-regression for every unchanged column path."""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
from old_path_score import routine
from ir_accounting import evaluate as object_evaluate

BASELINE = "002584a48d1f714e3856f5a3d6a99ac86c2204eb"
CANDIDATE = "87857fa2af8d4536d783b38542594017996061af"


LAYOUT_PROOF_SHA256 = "f74240f5fea3233b72c34946cc2dcdf70b27e621ae0e047ebb7b932a696241ed"

def layout_records(records, proof_path, binaries):
	proof_path = Path(proof_path)
	if hashlib.sha256(proof_path.read_bytes()).hexdigest() != LAYOUT_PROOF_SHA256:
		raise ValueError("layout proof pin mismatch")
	proof = json.loads(proof_path.read_text())
	if proof["binaries"] != binaries:
		raise ValueError("layout binary pins differ")
	for path, digest in {**proof["evidence"], proof["libc"]["path"]: proof["libc"]["sha256"]}.items():
		if hashlib.sha256(Path(path).read_bytes()).hexdigest() != digest:
			raise ValueError("layout evidence changed")
	adjusted = deepcopy(records)
	index = {(r["kind"], r["rows"], r["layout"], r["role"]): r for r in adjusted}
	for pair in proof["pairs"]:
		observations = []
		for role in ("baseline", "candidate"):
			cost = index[pair["kind"], pair["rows"], pair["layout"], role]["costs"][pair["call"]]
			pin = pair[role]
			if cost["sha256"] != pin["sha256"] or hashlib.sha256(Path(cost["profile"]).read_bytes()).hexdigest() != pin["sha256"]:
				raise ValueError("layout profile pin mismatch")
			observed = routine(cost["profile"], proof["routine"], int(proof["branch"], 16))
			base = int(next(p["load_bias"] for p in pair["relocation_proof"] if p["role"] == role), 16)
			callers = {}
			for name, count in observed["callers"].items():
				if name.startswith("0x"):
					if int(name, 16) - base != 0x286a0:
						raise ValueError("unresolved layout caller")
					name = "libc.so.6::memcpy@@GLIBC_2.14@plt[0x286a0]"
				callers[name] = callers.get(name, 0) + count
			if callers != pair["resolved_callers"] or observed["taken"] != pin["taken"]:
				raise ValueError("layout callers or branch counts differ")
			observations.append(observed)
			adjustment = proof["extra_Ir_per_taken"] * observed["taken"]
			# Normalize only this pinned routine branch. Raw Ir and raw ceilings stay untouched.
			cost["accounting"]["dynamic_Ir"] -= adjustment
			cost["accounting"]["total_Ir"] -= adjustment
		if observations[1]["self_Ir"] - observations[0]["self_Ir"] != proof["extra_Ir_per_taken"] * (observations[1]["taken"] - observations[0]["taken"]):
			raise ValueError("unattributed layout routine residual")
	return adjusted


def evaluate(records, policy, whole_map_evaluator, baseline, candidate, layout_proof=None, binaries=None):
	if (baseline, candidate) != (BASELINE, CANDIDATE):
		raise ValueError("migration build pair mismatch")
	migration = deepcopy(policy)
	migration["system_band_scope"] = {}
	for row in records:
		migration["system_band_scope"].setdefault(row["kind"], {})[row["layout"]] = "unchanged"
	def ceilings(rows, rules):
		raw = whole_map_evaluator(rows, rules)
		failures = [f for f in raw["failures"] if f["rule"] != "strict nullable improvement"]
		return dict(raw, failures=failures, status="FAIL" if failures else "PASS")
	checked = layout_records(records, layout_proof, binaries) if layout_proof else records
	# Ceilings are evaluated on original raw records, never adjusted totals.
	result = object_evaluate(checked, migration, lambda _rows, rules: ceilings(records, rules))
	result["layout_proof_sha256"] = LAYOUT_PROOF_SHA256 if layout_proof else None
	index = {(r["kind"], r["rows"], r["layout"], r["role"]): r for r in records}
	if len(index) != len(records):
		raise ValueError("duplicate migration case")
	result["partition_deltas"] = []
	for (kind, rows, layout, role), row in sorted(index.items()):
		if role != "candidate":
			continue
		before = index[kind, rows, layout, "baseline"]
		for call, (a, b) in enumerate(zip(before["costs"], row["costs"], strict=True)):
			result["partition_deltas"].append(dict(kind=kind, rows=rows, layout=layout, call=call,
				**{k: b["accounting"][k] - a["accounting"][k] for k in ("total_Ir", "rnx_Ir", "other_Ir", "dynamic_Ir")}))
	result["scope"] = {"baseline": baseline, "candidate_binary_source": candidate,
		"review": "rune-native27406", "new_tolerance": None}
	return result
