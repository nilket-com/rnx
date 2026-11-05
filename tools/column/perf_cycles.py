"""Parse perf's user-cycle CSV, including hybrid CPU PMU aliases."""
from decimal import Decimal


def user_cycles(text):
	counted, inactive = [], []
	for line in text.splitlines():
		if not line or line.startswith("#"):
			continue
		fields = [f.strip() for f in line.split(";")]
		if len(fields) < 5 or fields[2] not in ("cycles:u", "cpu_core/cycles/u", "cpu_atom/cycles/u"):
			continue
		value, event, runtime, percent = fields[0], fields[2], fields[3], fields[4]
		if value == "<not counted>" and Decimal(runtime) == 0 and Decimal(percent) == 0:
			inactive.append(event)
			continue
		if not value.isdigit() or int(value) <= 0 or Decimal(runtime) <= 0 or Decimal(percent) != 100:
			raise ValueError("missing or multiplexed cycle counter")
		counted.append({"event": event, "cycles": int(value), "runtime_ns": int(runtime)})
	if len(counted) != 1 or len(set(inactive)) != len(inactive):
		raise ValueError("expected one active cycle PMU")
	return {"user_cycles": counted[0]["cycles"], "active": counted[0], "inactive": inactive}


def equivalent_output(actual, expected):
	"""Ignore only declared elapsed-ms diagnostics; compare all result fields."""
	import re
	pattern = r"(?m)^(COLUMN_PREP|COLUMN_OK call=[01]) ms=[0-9]+(?= )"
	def stable(text):
		return re.sub(pattern, r"\1 ms=<elapsed>", text)
	return stable(actual) == stable(expected)
