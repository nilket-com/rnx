"""ADR 0238: signed paired build, identity first, then exact fixed-route vectors."""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import traceback
import child_result as receipts
from failure_detail import retain, StageFailure

ROOT = Path(__file__).resolve().parents[2]
BASELINE = '2ed1b182e1464c5abae4ce4042774fa41e8956cc'


def stage(out, name, command, receipt):
	log = out / (name + '.log')
	job = value = None
	try:
		with log.open('x') as stream:
			job = subprocess.run(command, cwd=ROOT, stdout=stream,
				stderr=subprocess.STDOUT, timeout=28800)
		value = json.loads(receipt.read_text())
		row = dict(exit=job.returncode, command=list(map(str, command)),
			receipt=str(receipt), sha256=receipts.digest(receipt),
			log_sha256=receipts.digest(log), status=value.get('status'))
		receipts.atomic_json(out / (name + '-stage.json'), row)
		if job.returncode != 0 or value.get('status') != 'PASS':
			raise RuntimeError('child failed: ' + str(value.get('error', value.get('status'))))
	except Exception as error:
		wrapper = out/(name+'-stage.json')
		receipts.atomic_json(wrapper, dict(status='FAIL', exit=job.returncode if job else None,
			error=str(error), error_type=type(error).__name__))
		failure = retain(out, name, receipt=receipt if isinstance(value, dict) else wrapper,
			logs=[log], roots=[out], error=str(error))
		raise StageFailure(name, failure) from error
	return value


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out', type=Path, required=True)
	p.add_argument('--cargo', type=Path, required=True)
	p.add_argument('--valgrind-root', type=Path, required=True)
	a = p.parse_args()
	out = a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL', baseline=BASELINE)
	try:
		assert not subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT), 'dirty candidate'
		r.update(candidate=receipts.head(ROOT), source=receipts.source(ROOT))
		with (out/'preflight.log').open('x') as log:
			subprocess.run([sys.executable,'-B','ci/preflight.py'],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,check=True)
		pair = stage(out, 'build', [sys.executable, '-B', 'tools/column/paired_build.py',
			'--out', str(out/'pair'), '--baseline', BASELINE, '--candidate', r['candidate'],
			'--cargo', str(a.cargo.absolute())], out/'pair/RESULT.json')
		baseline, candidate = [row['binary'] for row in pair['builds']]
		stage(out, 'identity', [sys.executable, '-B', 'tools/column/executor_identity.py',
			'--baseline', baseline, '--candidate', candidate, '--out', str(out/'IDENTITY.json')], out/'IDENTITY.json')
		stage(out, 'vectors', [sys.executable, '-B', 'tools/column/fixed_route_vectors.py',
			'--out', str(out/'VECTORS'), '--build-receipt', str(out/'pair/RESULT.json'),
			'--valgrind-root', str(a.valgrind_root.resolve())], out/'VECTORS/RESULT.json')
		assert receipts.head(ROOT) == r['candidate'] and receipts.source(ROOT) == r['source'], 'source changed'
		r['status'] = 'PASS'
	except Exception as error:
		r['error'] = traceback.format_exc()
		if isinstance(error, StageFailure): r['failure'] = error.failure
	finally:
		receipts.publish(out/'RESULT.json', r)
		print(r['status'], out/'RESULT.json', flush=True)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
