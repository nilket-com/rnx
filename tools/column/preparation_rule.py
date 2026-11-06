"""ADR 0213: narrow reviewed preparation constant; never a per-row allowance."""
import hashlib
import json

BASE = "0f95d955081fd21d43b04bba6b5fac6f1b6bc084"
CANDIDATE = "1263b1e449d7470e947701874f075841b9843739"
FUNCTIONS = (
	"rune::experimental_core::column::plan::prepare",
	"<rune::experimental_core::column::PreparedColumn>::prepare",
	"<alloc::vec::Vec<core::option::Option<usize>>>::resize",
	"rune::experimental_core::column::filled::<usize>",
)

VECTORS = {
	"multiply3": (1060, -977, -63, 36),
	"i64add": (753, -716, -53, 26),
	"axpy": (1060, -977, -63, 36),
	"f64const": (756, -719, -53, 26),
	"f64add": (708, -670, -53, 26),
	"boolnot": (586, -568, -48, 18),
}
LAYOUTS = ("flat", "sliced-chunked-null", "alternating", "deterministic-random")


def check(samples, baseline, candidate):
	if (baseline, candidate) != (BASE, CANDIDATE):
		raise ValueError("unreviewed pair")
	if {(s["rows"], s["call"]) for s in samples} != {(n, c) for n in (4096, 10000000, 100000000) for c in (0, 1)} or len(samples) != 6:
		raise ValueError("missing or duplicate sizes/calls")
	kind, layout = samples[0]["kind"], samples[0]["layout"]
	if kind not in VECTORS or layout not in LAYOUTS:
		raise ValueError("unreviewed path")
	expected = VECTORS[kind]
	vectors = []
	for sample in samples:
		if sample["kind"] != kind or sample["layout"] != layout:
			raise ValueError("unreviewed path")
		a, b = sample["baseline"], sample["candidate"]
		vector = tuple(b["functions"].get(f, 0) - a["functions"].get(f, 0) for f in FUNCTIONS)
		allowance = sum(vector)
		if allowance > sum(expected):
			raise ValueError("preparation constant exceeds reviewed cap")
		if b["program_Ir"] - a["program_Ir"] - max(0, allowance) > 0:
			raise ValueError("unattributed program growth")
		if vector != expected:
			raise ValueError("size/call-dependent or unreviewed vector")
		vectors.append(vector)
	if len(set(vectors)) != 1:
		raise ValueError("size/call-dependent preparation growth")
	return {"status": "PASS", "preparation_delta": sum(vectors[0]), "functions": dict(zip(FUNCTIONS, vectors[0], strict=True)), "evidence_sha256": hashlib.sha256(json.dumps(samples, sort_keys=True).encode()).hexdigest()}
