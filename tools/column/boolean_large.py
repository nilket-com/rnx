"""ADR0220 durable large capture of the reviewed binaries, without rebuilding."""
import argparse
import json
import platform
import subprocess
import sys
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/column'))
import boolean_pins
import boolean_vectors
import ir_pool
sys.path.insert(0, str(ROOT / 'rune/tools/xtask/supplemental'))
from receipts import digest


def main():
	p = argparse.ArgumentParser(description=__doc__)
	for name in ('out', 'builds', 'small', 'resources', 'measurement-tooling', 'valgrind'):
		p.add_argument('--' + name, required=True, type=Path)
	a = p.parse_args(); out = a.out.resolve(); out.mkdir(parents=True, exist_ok=True)
	result = {'status': 'FAIL'}
	try:
		head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
		assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT, text=True).strip()
		changes = subprocess.check_output(['git', 'diff', '--name-only', boolean_pins.POLICY['candidate'], head], cwd=ROOT, text=True).splitlines()
		assert all(f.startswith(('tools/column/', 'plans/')) for f in changes), changes
		build = json.loads(a.builds.read_text()); assert build['status'] == 'PASS'
		roles = build['builds']; boolean_pins.verify_pair(roles)
		for role in roles: assert digest(role['binary']) == role['sha256']
		shape = a.builds.with_name('SIMD.json'); assert json.loads(shape.read_text())['status'] == 'PASS'
		small = json.loads(a.small.read_text())
		preflight = boolean_vectors.evaluate(small['records'], roles)
		ir_pool.terminal_json(out / 'PREFLIGHT.json', preflight)
		assert preflight['status'] == 'PASS'
		c = ir_pool.tooling(a.measurement_tooling.resolve())
		import cli_fixtures
		cli_fixtures.KERNELS.update({
			'f64chain': ('f64', 'pub fn map(x,y) { x * 2.0 + y }', ['1.0', '2.0'], '4.0'),
			'booland': ('bool', 'pub fn map(x,y) { x & y }', ['true', 'false'], 'false'),
			'boolor': ('bool', 'pub fn map(x,y) { x | y }', ['true', 'false'], 'true'),
			'booleq': ('bool', 'pub fn map(x,y) { x == y }', ['true', 'false'], 'false'),
			'boolxor': ('bool', 'pub fn map(x,y) { x ^ y }', ['true', 'false'], 'true'),
			'boolchain': ('bool', 'pub fn map(a,b,c) { (a & b) | c }', ['true', 'false', 'true'], 'true'),
		})
		for role in roles: role['symbols'] = c.symbols(role['binary'])
		identity = {'head': head, 'builds': digest(a.builds), 'small': digest(a.small), 'shape': digest(shape),
			'controller': digest(__file__), 'tools': {f: digest(ROOT / 'tools/column' / f) for f in ('boolean_vectors.py', 'boolean_pins.py', '0220_pins.json', 'boolean_limits.py')},
			'product_source_equivalence': changes, 'environment': ir_pool.capture_environment(), 'budget': 1000000000,
			'calls': 2, 'aslr': {'disabled': True, 'setarch': '/usr/bin/setarch', 'machine': platform.machine()}, 'boundary_tooling': ir_pool.PIN}
		limits = json.loads(a.resources.read_text()); workers = min(8, limits['workers'])
		ir_pool.terminal_json(out / 'MANIFEST.json', dict(identity=identity, resources=limits, workers=workers))
		jobs = [dict(kind=k, rows=n, layout=l, role=role, identity=identity, valgrind=str(a.valgrind.resolve()),
			directory=str(out / 'matrix00' / ir_pool.slot_name(k, n, l, role['role'])), import_directory=None)
			for k in boolean_vectors.EXACT + boolean_vectors.POSITIVE for l in boolean_vectors.LAYOUTS
			for n in (10000000, 100000000) for role in roles]
		captured = ir_pool.run_jobs(jobs, workers)
		raw = [r for r, _ in captured]
		verdict = boolean_vectors.evaluate(raw, roles, (10000000, 100000000))
		ir_pool.terminal_json(out / 'VECTORS.json', verdict)
		for f, h in identity['tools'].items(): assert digest(ROOT / 'tools/column' / f) == h
		result.update(status=verdict['status'], head=head, records=raw, cases=[p for _, p in captured], acceptance=verdict, identity=identity)
	except BaseException as e:
		result.update(error=repr(e), traceback=traceback.format_exc())
	finally:
		ir_pool.terminal_json(out / 'RESULT.json', result)
		(out / 'NOTICE.txt').write_text(str(out / 'RESULT.json') + '\n')
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
