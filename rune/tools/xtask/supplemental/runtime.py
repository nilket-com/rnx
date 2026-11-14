"""Linux resource observations and traced, bounded child execution."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import time
from memory_resources import observe


def resources():
	return observe()


def execute(command, directory, env, cwd, timeout, label='process'):
	out = directory / (label + '.stdout')
	err = directory / (label + '.stderr')
	start = time.monotonic()
	with out.open('x') as stdout, err.open('x') as stderr:
		child = subprocess.Popen(command, cwd=cwd, env=env, stdout=stdout, stderr=stderr, start_new_session=True)
		try:
			code = child.wait(timeout=timeout)
		except BaseException:
			os.killpg(child.pid, signal.SIGKILL)
			child.wait()
			raise
	return {'command': command, 'exit': code, 'seconds': time.monotonic() - start}


def traces(directory):
	intervals = []
	locks = []
	for path in sorted(directory.glob('trace.*')):
		start = end = None
		for line in path.read_text().splitlines():
			if 'flock(' in line and '.package-cache' in line:
				match = re.search(r'<([0-9.]+)>$', line)
				if match:
					locks.append({'trace': path.name, 'seconds': float(match[1]), 'event': line})
			if 'execve(' in line and '/bin/miri"' in line and '"--test"' in line and '"--crate-name"' in line and '= 0' in line:
				if start is not None:
					raise ValueError('multiple interpreter starts in one trace')
				start = float(line.split()[0])
			if start is not None and ('exit_group(' in line or '+++ exited with' in line or '+++ killed by' in line):
				end = float(line.split()[0])
		if start is not None:
			if end is None or end < start:
				raise ValueError('unterminated interpreter trace')
			intervals.append({'start': start, 'end': end, 'trace': path.name})
	return intervals, locks


def overlap(rows):
	events = []
	for row in rows:
		for span in row['intervals']:
			events.extend([(span['start'], 1), (span['end'], -1)])
	live = peak = 0
	for _, delta in sorted(events):
		live += delta
		peak = max(peak, live)
	return peak


def check_tests(stdout, expected):
	observed = re.findall(r'^test (\S+) \.\.\. (ok|ignored[^\n]*)$', stdout, re.M)
	if sorted(name for name, _ in observed) != sorted(expected):
		raise ValueError('observed tests differ from exact shard inventory')
	if not re.search(r'^test result: ok\.', stdout, re.M):
		raise ValueError('missing successful libtest summary')
	return [name for name, status in observed if status.startswith('ignored')]
