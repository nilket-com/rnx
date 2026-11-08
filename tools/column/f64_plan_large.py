"""ADR0221 durable large capture of the reviewed binaries, without rebuilding."""
import child_result
import argparse
import json
import platform
import subprocess
import sys
import traceback
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tools/column'))
import f64_plan_pins
import f64_plan_vectors
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
		changes = subprocess.check_output(['git', 'diff', '--name-only', f64_plan_pins.POLICY['candidate'], head], cwd=ROOT, text=True).splitlines()
		assert all(f.startswith(('tools/column/', 'plans/', 'BACKLOG.md')) for f in changes), changes
		build = json.loads(a.builds.read_text()); assert build['status'] == 'PASS'
		roles = build['builds']; f64_plan_pins.verify_pair(roles)
		for role in roles: assert digest(role['binary']) == role['sha256']
		shape = a.builds.with_name('SIMD.json'); assert json.loads(shape.read_text())['status'] == 'PASS'
		own = json.loads(a.builds.with_name('F64-SHAPE.json').read_text())
		assert own['binary_sha256'] == next(b['sha256'] for b in roles if b['role'] == 'candidate')
		small = json.loads(a.small.read_text())
		preflight = f64_plan_vectors.evaluate(small['records'], roles)
		small_shapes = f64_plan_vectors.executed_shapes(small['records'], own)
		assert small_shapes['status'] == 'PASS'
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
		identity = {'head': head, 'builds': digest(a.builds), 'small': digest(a.small), 'shape': digest(shape), 'f64_shape': digest(a.builds.with_name('F64-SHAPE.json')),
			'controller': digest(__file__), 'tools': {f: digest(ROOT / 'tools/column' / f) for f in ('f64_plan_vectors.py', 'f64_plan_pins.py', '0221_pins.json', 'f64_plan_limits.py', 'f64_plan_loop.py')},
			'product_source_equivalence': changes, 'environment': ir_pool.capture_environment(), 'budget': 1000000000,
			'calls': 2, 'aslr': {'disabled': True, 'setarch': '/usr/bin/setarch', 'machine': platform.machine()}, 'boundary_tooling': ir_pool.PIN}
		limits = json.loads(a.resources.read_text()); workers = min(8, limits['workers'])
		ir_pool.terminal_json(out / 'MANIFEST.json', dict(identity=identity, resources=limits, workers=workers))
		jobs = [dict(kind=k, rows=n, layout=l, role=role, identity=identity, valgrind=str(a.valgrind.resolve()),
			directory=str(out / 'matrix00' / ir_pool.slot_name(k, n, l, role['role'])), import_directory=None)
			for k in f64_plan_vectors.KINDS for l in f64_plan_vectors.LAYOUTS
			for n in (10000000, 100000000) for role in roles]
		captured = ir_pool.run_jobs(jobs, workers)
		raw = [r for r, _ in captured]
		verdict = f64_plan_vectors.evaluate(raw, roles, (10000000, 100000000))
		executed = f64_plan_vectors.executed_shapes(raw, own)
		ir_pool.terminal_json(out / 'EXECUTED-SHAPES.json', executed)
		if executed['status'] != 'PASS': verdict['status'] = 'FAIL'
		ir_pool.terminal_json(out / 'VECTORS.json', verdict)
		for f, h in identity['tools'].items(): assert digest(ROOT / 'tools/column' / f) == h
		result.update(status=verdict['status'], head=head, records=raw, cases=[p for _, p in captured], acceptance=verdict, identity=identity)
	except BaseException as e:
		result.update(error=repr(e), traceback=traceback.format_exc())
	finally:
		child_result.publish(out / 'RESULT.json', result)
		(out / 'NOTICE.txt').write_text(str(out / 'RESULT.json') + '\n')
	return 0 if result['status'] == 'PASS' else 1


if __name__ == '__main__':
	raise SystemExit(main())
