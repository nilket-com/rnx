"""Reviewed repetition-based old-path scoring; raw costs remain immutable."""
import argparse
from collections import defaultdict
import hashlib
import json
from pathlib import Path
import re

from ir_accounting import account


def sha(path):
	return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def evidence(root, pins):
	if not pins:
		raise ValueError("missing attribution hashes")
	for name, expected in pins.items():
		path = root / name
		if not re.fullmatch(r"[0-9a-f]{64}", expected) or sha(path) != expected:
			raise ValueError("attribution hash mismatch: " + name)


def interval(before, after):
	if len(before) != 3 or len(after) != 3:
		raise ValueError("exactly three repeats required")
	return min(after) - max(before), max(after) - min(before)


def slope_pass(low, high):
	# The minimum possible extra growth must be nonpositive. Every sample is
	# retained; this is an observed envelope, not a confidence interval.
	return high[0] <= low[1]


def routine(path, name, branch):
	lines = Path(path).read_text().splitlines()
	names = {}
	for line in lines:
		m = re.fullmatch(r"c?fn=\((\d+)\) (.+)", line)
		if m:
			names[m[1]] = m[2]
	current = callee = None
	pending = None
	pc = 0
	callers = defaultdict(int)
	taken = 0
	self_ir = 0
	for line in lines:
		m = re.fullmatch(r"(c?fn)=\((\d+)\)(?: .+)?", line)
		if m:
			if m[1] == "fn":
				current = names[m[2]]
			else:
				callee = names[m[2]]
			continue
		if line.startswith("calls="):
			pending = int(line[6:].split()[0])
			continue
		if line.startswith("jcnd="):
			if current == name and pc == branch:
				taken += int(line[5:].split("/")[0])
			continue
		fields = line.split()
		if len(fields) != 3 or not re.fullmatch(r"(?:0x[0-9a-fA-F]+|[+-]?\d+|\*)", fields[0]):
			continue
		if not fields[2].isdigit():
			continue
		address = fields[0]
		if address.startswith(("+", "-")):
			pc += int(address)
		elif address != "*":
			pc = int(address, 0)
		if pending is not None:
			if callee == name:
				callers[current] += pending
			pending = None
		elif current == name:
			self_ir += int(fields[2])
	return {"self_Ir": self_ir, "calls": sum(callers.values()), "callers": dict(callers), "taken": taken}


def layout_delta(before, after, rule):
	if before["calls"] != after["calls"] or before["callers"] != after["callers"]:
		raise ValueError("layout routine call counts differ")
	delta = after["self_Ir"] - before["self_Ir"]
	explained = (after["taken"] - before["taken"]) * rule["extra_Ir_per_taken"]
	if delta != explained:
		raise ValueError("unattributed layout routine residual")
	return delta


def score(samples, policy, root):
	evidence(root, policy["evidence"])
	for role, pin in policy["binaries"].items():
		if not re.fullmatch(r"[0-9a-f]{64}", pin):
			raise ValueError("invalid binary pin")
	groups = defaultdict(dict)
	for s in samples:
		kind, mode, n, role, trial = s["key"]
		if s["binary_sha256"] != policy["binaries"][role]:
			raise ValueError("candidate or baseline pin mismatch")
		key = (kind, mode)
		if (n, role, trial) in groups[key]:
			raise ValueError("duplicate repeat")
		groups[key][n, role, trial] = s
	if set(groups) != {tuple(k) for k in policy["cases"]}:
		raise ValueError("case coverage differs")
	rows, failures = [], []
	for (kind, mode), group in sorted(groups.items()):
		expected = {(n, role, t) for n in (1, 5) for role in ("published", "candidate") for t in range(3)}
		if set(group) != expected:
			raise ValueError("repeat coverage differs")
		constant = policy["constants"].get(kind + "/" + mode)
		if constant:
			evidence(root, constant["evidence"])
			if mode == "execution-off":
				raise ValueError("native-off constant forbidden")
		intervals = defaultdict(list)
		for n in (1, 5):
			values = {cat: {role: [] for role in ("published", "candidate")} for cat in ("program", "unknown", "system", "raw_system")}
			layout = policy.get("layout") if [kind, mode] == policy.get("layout", {}).get("case") else None
			for trial in range(3):
				left, right = [group[n, role, trial] for role in ("published", "candidate")]
				if layout:
					evidence(root, layout["evidence"])
					layout_delta(left["layout"], right["layout"], layout)
				for role, s in (("published", left), ("candidate", right)):
					a = s["accounting"]
					values["program"][role].append(a["rnx_Ir"] - (sum(constant["functions"].values()) if constant and role == "candidate" else 0))
					values["unknown"][role].append(a["other_Ir"])
					values["raw_system"][role].append(a["dynamic_Ir"])
					# Remove only the pinned routine's address-dependent extra path,
					# not the routine itself or any other libc work.
					values["system"][role].append(a["dynamic_Ir"] - (s["layout"]["taken"] * layout["extra_Ir_per_taken"] if layout else 0))
			if constant:
				for name, delta in constant["functions"].items():
					a, b = [[group[n, role, t]["accounting"]["rnx_functions"].get(name, 0) for t in range(3)] for role in ("published", "candidate")]
					lo, hi = interval(a, b)
					if not lo <= delta <= hi:
						failures.append({"rule": "constant function attribution", "kind": kind, "n": n, "function": name})
			for cat in ("program", "unknown", "system"):
				v = interval(values[cat]["published"], values[cat]["candidate"])
				intervals[cat].append(v)
				if cat == "program" and (v != (0, 0) if mode == "execution-off" else v[0] > 0):
					failures.append({"rule": "program constant/exact-off", "kind": kind, "mode": mode, "n": n, "interval": v})
				if cat == "unknown" and not v[0] <= 0 <= v[1]:
					failures.append({"rule": "unknown outside spread", "kind": kind, "mode": mode, "n": n, "interval": v})
			rows.append({"kind": kind, "mode": mode, "n": n, "values": values, "constant_Ir": sum(constant["functions"].values()) if constant else 0})
		for cat, bounds in intervals.items():
			if not slope_pass(*bounds):
				failures.append({"rule": cat + " positive slope beyond spread", "kind": kind, "mode": mode, "intervals": bounds})
	return {"status": "FAIL" if failures else "PASS", "failures": failures, "rows": rows, "policy": policy["review"], "note": "three-repeat observed envelopes; raw costs retained"}


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument("--captures", type=Path, required=True)
	p.add_argument("--policy", type=Path, required=True)
	p.add_argument("--evidence-root", type=Path, required=True)
	p.add_argument("--out", type=Path, required=True)
	p.add_argument("--original", type=Path, required=True)
	args = p.parse_args()
	policy = json.loads(args.policy.read_text())
	samples = []
	for path in sorted(args.captures.glob("*/CAPTURE.json")):
		s = json.loads(path.read_text())
		if s["status"] != "PASS":
			raise ValueError("failed capture")
		evidence(Path("/"), s["artifacts"])
		binary = path.parent / "binary"
		if sha(binary) != s["binary_sha256"]:
			raise ValueError("binary changed")
		s["accounting"] = account(s["profile"], binary)
		layout = policy.get("layout")
		if layout and s["key"][:2] == layout["case"]:
			s["layout"] = routine(s["profile"], layout["function"], int(layout["branch"], 16))
		samples.append(s)
	original = json.loads(args.original.read_text())
	if original.get("error") or not original.get("comparisons"):
		raise ValueError("original suite incomplete")
	covered = {tuple(k) for k in policy["cases"]}
	if any(not c["pass"] and (c["kind"], c["policy"]) not in covered for c in original["comparisons"]):
		raise ValueError("original failure missing from repeat suite")
	result = score(samples, policy, args.evidence_root)
	result.update(original_receipt_sha256=sha(args.original), original_status=original["status"], original_comparisons=original["comparisons"])
	result.update(policy_sha256=sha(args.policy), scorer_sha256=sha(__file__))
	args.out.write_text(json.dumps(result, indent=2) + "\n")
	print(result["status"], args.out)
	return 0 if result["status"] == "PASS" else 1


if __name__ == "__main__":
	raise SystemExit(main())
