"""ADR 0280: paired JSON parse cost, excluding file I/O and compilation from the timer."""
import argparse
import json
import os
from pathlib import Path
import re
import shutil
import statistics
import subprocess
import time
import traceback

import child_result as child
from owned_build import run_process


def document(kind, count):
	if kind == 'object':
		return json.dumps({f'key-{i:06d}': i for i in range(count)}, separators=(',', ':'))
	return json.dumps(list(range(count)), separators=(',', ':'))


def program(path):
	# Parsing, the host result and unwrap are timed; loading the file, printing,
	# length validation and dropping the result are outside that interval.
	return '''pub fn main(_args) {
 let text = fs::read(%s).unwrap();
 for _ in 0..3 {
  let before = time::monotonic_ns();
  let value = json::parse(text).unwrap();
  let elapsed = time::monotonic_ns() - before;
  println!("JSON_PARSE {} {}", elapsed, value.len());
 }
}
''' % json.dumps(str(path))


def run(builds, out, processes, timer):
	out.mkdir(parents=True, exist_ok=False)
	result = dict(status='FAIL', processes=processes, calls_per_process=3, rows=[],
		timing_scope='parse + outer result + unwrap; excludes file I/O, compilation, print, validation and final result destruction',
		rss_scope='whole CLI process peak, including source compilation and file loading; KiB from GNU time')
	try:
		proof = json.loads(builds.read_text())
		assert proof['status'] == 'PASS'
		by_role = {r['role']: r for r in proof['builds']}
		assert set(by_role) == {'baseline', 'candidate'}
		for row in by_role.values():
			assert child.digest(Path(row['binary'])) == row['sha256'], 'binary identity mismatch'
		result['builds'] = {role: {k: row[k] for k in ('source_commit', 'sha256')} for role, row in by_role.items()}
		env = dict(os.environ, CARGO_TERM_COLOR='never', NO_COLOR='1')
		for kind, count in [('object', 1000), ('object', 100000), ('array', 100000)]:
			data = out / f'{kind}-{count}.json'
			data.write_text(document(kind, count))
			script = out / f'{kind}-{count}.rn'
			script.write_text(program(data))
			for engine in ('execution', 'legacy'):
				cells = {role: dict(role=role, engine=engine, kind=kind, count=count, calls=[], rss_kib=[], wall_seconds=[]) for role in by_role}
				for process in range(processes):
					# Swap pair order by process; every role gets the same script path.
					order = ('baseline', 'candidate') if process % 2 == 0 else ('candidate', 'baseline')
					for role in order:
						stem = f'{kind}-{count}-{engine}-{role}-{process}'
						log, rss = out / (stem + '.log'), out / (stem + '.rss')
						cmd = [str(timer), '-f', '%M', '-o', str(rss), by_role[role]['binary'], 'run', '--debug-engine', '--budget', '2000000']
						if engine == 'legacy': cmd.append('--engine=legacy')
						cmd.append(str(script))
						start = time.monotonic()
						with log.open('x') as stream:
							p = run_process(cmd, cwd=out, env=env, stdout=stream, stderr=subprocess.STDOUT, timeout=300)
						assert p.returncode == 0, (stem, 'CLI failed', p.returncode)
						text = log.read_text()
						assert f'rnx: engine={engine}' in text, (stem, 'wrong route')
						assert 'refusal=' not in text and 'fallback' not in text, (stem, 'refused route')
						calls = [(int(ns), int(n)) for ns, n in re.findall(r'^JSON_PARSE (\d+) (\d+)$', text, re.M)]
						assert len(calls) == 3 and all(ns > 0 and n == count for ns, n in calls), (stem, 'wrong result', calls)
						cell = cells[role]
						cell['calls'].append([ns for ns, _ in calls]); cell['rss_kib'].append(int(rss.read_text()))
						cell['wall_seconds'].append(time.monotonic() - start)
				for cell in cells.values():
					cell['median_parse_ns'] = statistics.median(statistics.median(v) for v in cell['calls'])
					cell['median_peak_rss_kib'] = statistics.median(cell['rss_kib'])
					result['rows'].append(cell)
		result['status'] = 'PASS'
	except BaseException:
		result['error'] = traceback.format_exc()
	finally:
		child.atomic_json(out / 'RESULT.json', result)
		print(result['status'], out / 'RESULT.json', flush=True)
	return result['status'] != 'PASS'


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--builds', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	p.add_argument('--processes', type=int, default=6)
	p.add_argument('--time', type=Path, default=Path(shutil.which('time') or '/usr/bin/time'))
	a = p.parse_args()
	if a.processes < 2 or a.processes % 2: p.error('an even process count >= 2 is required')
	return run(a.builds.resolve(), a.out.resolve(), a.processes, a.time.resolve())


if __name__ == '__main__':
	raise SystemExit(main())
