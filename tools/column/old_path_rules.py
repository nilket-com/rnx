"""Old-path evidence must contain the workload before its cost is compared."""


def require_workload(profile, kind, count):
	functions = profile["functions"]
	if profile["total_Ir"] <= 0:
		raise ValueError("empty execution interval")
	proof = {}
	if kind == "callback":
		proof["vm_calls"] = sum(v["calls"] for name, v in functions.items()
			if name.endswith("<rune::runtime::vm::Vm>::run"))
		proof["callback_Ir"] = sum(v["self_Ir"] for name, v in functions.items()
			if "rnx_polars::generated::support::callback::bridge::<(i64,), i64>" in name)
		proof["worker_Ir"] = sum(v["self_Ir"] for name, v in functions.items()
			if "std::sys::backtrace::__rust_begin_short_backtrace" in name
			and "rnx_polars::engine::run<" in name
			and "in_place_polars_core__datatypes__int64chunked" in name)
		if proof["vm_calls"] < count or proof["callback_Ir"] <= 0 or proof["worker_Ir"] <= 0:
			raise ValueError(f"callback workload absent or incomplete: {proof}; expected {count} rows")
	return proof


def compare_growth(before, after):
	"""A constant profile is invalid evidence, even when its delta is zero."""
	old = before[1] - before[0]
	new = after[1] - after[0]
	if old <= 0 or new <= 0:
		raise ValueError(f"vacuous work interval: published growth {old}, candidate growth {new}")
	return {"published_growth_Ir": old, "candidate_growth_Ir": new,
		"extra_growth_Ir": new - old, "pass": new <= old}
