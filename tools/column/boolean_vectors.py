"""ADR 0220 exact protected vectors; no unreviewed preparation exemptions."""
from pathlib import Path
import hashlib
import sys

ROOT = Path(__file__).resolve().parents[2]
# Installed under tools/column; callers running a target draft supply parser paths.
sys.path.insert(0, str(ROOT / 'rune/tools/xtask/measure'))
import callgrind

EXACT = ('i64add', 'axpy', 'f64const', 'f64add', 'boolnot', 'multiply3', 'f64chain', 'booleq', 'boolxor')
POSITIVE = ('booland', 'boolor', 'boolchain')
LAYOUTS = ('flat', 'sliced-chunked-null', 'alternating', 'deterministic-random')



def vector(cost):
	path = Path(cost['profile'])
	if hashlib.sha256(path.read_bytes()).hexdigest() != cost['sha256']:
		raise ValueError('profile digest mismatch')
	functions = callgrind.parse(path)['functions']
	out = {}
	for name, ir in cost['accounting']['rnx_functions'].items():
		canonical = name
		if canonical in out:
			raise ValueError('normalization collision: ' + canonical)
		if name not in functions or functions[name]['self_Ir'] != ir:
			raise ValueError('function/object cost mismatch: ' + name)
		out[canonical] = (ir, functions[name].get('calls', 0))
	return out


def compare(a, b):
	return {name: {'baseline': a.get(name, (0, 0)), 'candidate': b.get(name, (0, 0))}
		for name in a.keys() | b.keys() if a.get(name, (0, 0)) != b.get(name, (0, 0))}


def evaluate(records, builds, sizes=(4096, 65536)):
	from boolean_pins import verify_pair, check_vectors
	verify_pair(builds)
	if tuple(sizes) not in ((4096, 65536), (10000000, 100000000)):
		raise ValueError("unreviewed matrix sizes")
	keys = {(k, l, n, r) for k in EXACT + POSITIVE for l in LAYOUTS for n in sizes for r in ('baseline', 'candidate')}
	index = {(r['kind'], r['layout'], r['rows'], r['role']): r for r in records}
	if len(index) != len(records) or set(index) != keys:
		raise ValueError('missing, duplicate or unexpected matrix case')
	failures, comparisons, witnesses = [], [], []
	for kind in EXACT + POSITIVE:
		for layout in LAYOUTS:
			for rows in sizes:
				a = index[kind, layout, rows, 'baseline']
				b = index[kind, layout, rows, 'candidate']
				if a['fixture_sha256'] != b['fixture_sha256'] or len(a['costs']) != 2 or len(b['costs']) != 2:
					raise ValueError('fixture/call mismatch')
				for call, (ac, bc) in enumerate(zip(a['costs'], b['costs'], strict=True)):
					av, bv = vector(ac), vector(bc)
					case = dict(kind=kind, layout=layout, rows=rows, call=call)
					if kind in POSITIVE:
						hits = sum(v[1] for f, v in bv.items() if 'column::run::boolean::run::' in f)
						prior = sum(v[1] for f, v in av.items() if 'column::run::boolean::run::' in f)
						witnesses.append(dict(case, packed_calls=hits, normal_packed_calls=prior))
						if hits <= 0 or prior != 0:
							failures.append(dict(case, rule='live packed witness'))
						continue
					exact, applied = check_vectors(av, bv, kind, layout, rows)
					unknown_a, unknown_b = ac['accounting']['other_Ir'], bc['accounting']['other_Ir']
					if exact or unknown_a != unknown_b:
						failures.append(dict(case, rule='exact function vector', differences=exact, unknown_before=unknown_a, unknown_after=unknown_b))
					comparisons.append(dict(case, exact_differences=exact, applied_pins=applied, program_delta=bc['accounting']['rnx_Ir']-ac['accounting']['rnx_Ir'], system_delta=bc['accounting']['dynamic_Ir']-ac['accounting']['dynamic_Ir']))
	if tuple(sizes) == (10000000, 100000000):
		from boolean_limits import limits
		failures.extend(limits(index, EXACT, POSITIVE, LAYOUTS, sizes))
	return dict(status='FAIL' if failures else 'PASS', failures=failures, comparisons=comparisons, witnesses=witnesses)

