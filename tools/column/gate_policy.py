"""Record-bound required gates and final acceptance, shared by proportional callers."""
import argparse
import json
from pathlib import Path
import re
import subprocess

import child_result as child

ROOT = Path(__file__).resolve().parents[2]
EXACT = {'Cargo.toml', 'Cargo.lock', 'rune/Cargo.toml', 'rune/Cargo.lock', 'tools/verify.py'}
# Conservative source roots include the optional root path dependency. Keep
# this independent of active Cargo features (metadata --no-deps omits excluded
# dependency packages, and a current-only graph misses removed dependencies).
PREFIXES = ('src/', 'rune/crates/', 'tools/project/', 'tools/verify/')


def git(root, *args):
	return subprocess.check_output(['git', '-C', str(root), *args])


def record(root, candidate='HEAD'):
	candidate = git(root, 'rev-parse', '--verify', candidate + '^{commit}').decode().strip()
	subject = git(root, 'show', '-s', '--format=%s', candidate).decode().strip()
	match = re.fullmatch(r'(plan|impl): adr ([0-9]{4}) .+', subject)
	if not match: raise ValueError('candidate is not a record commit')
	plan = candidate if match[1] == 'plan' else git(root, 'rev-parse', candidate + '^').decode().strip()
	plan_subject = git(root, 'show', '-s', '--format=%s', plan).decode().strip()
	if not plan_subject.startswith('plan: adr ' + match[2] + ' '):
		raise ValueError('candidate does not immediately follow its plan')
	baseline = git(root, 'rev-parse', plan + '^').decode().strip()
	# No rename folding: both removed and added names participate in the rule.
	paths = [p.decode('utf-8', 'surrogateescape') for p in git(root, 'diff', '--no-renames', '--name-only', '-z', baseline, candidate).split(b'\0') if p]
	return {'candidate': candidate, 'baseline': baseline, 'plan': plan, 'record': match[2],
		'phase': match[1], 'paths': paths, 'required': required(paths)}


def required(paths):
	return ['cross'] if any(p in EXACT or p.startswith(PREFIXES) for p in paths) else []


def policy_digest():
	return child.identity({p: child.digest(Path(__file__).with_name(p)) for p in ('gate_policy.py', 'child_result.py', 'accept.py')})


def select(root, out, names=None, candidate='HEAD'):
	import accept
	root = Path(root).resolve(); out = Path(out).resolve()
	info = record(root, candidate)
	commands = accept.commands(out, root=root)
	names = list(dict(commands)) if names is None else list(names)
	if len(names) != len(set(names)): raise ValueError('duplicate stage selection')
	if set(names) - dict(commands).keys(): raise ValueError('unknown selected stage')
	selected = set(names) | set(info['required'])
	return {'schema': 1, 'record': info, 'policy': policy_digest(), 'source': child.source(root),
		'root': str(root), 'out': str(out), 'stages': [[n, c] for n, c in commands if n in selected]}


def check_selection(manifest, root, *, final=False):
	if manifest.get('schema') != 1: raise ValueError('unknown selection schema')
	actual = select(root, manifest['out'], [n for n, _ in manifest['stages']], manifest['record']['candidate'])
	if actual != manifest: raise ValueError('selection, source, policy or required gates changed')
	if git(root, 'rev-parse', 'HEAD').decode().strip() != manifest['record']['candidate']:
		raise ValueError('selection is not for checkout candidate')
	if final:
		if manifest['record']['phase'] != 'impl' or git(root, 'status', '--porcelain', '-z'):
			raise ValueError('final acceptance requires a clean implementation commit')


def finalize(manifest, evidence, root=ROOT):
	check_selection(manifest, root, final=True)
	if set(evidence) != {n for n, _ in manifest['stages']}:
		raise ValueError('missing or extraneous stage evidence')
	for name, command in manifest['stages']:
		row = child.evidence(evidence[name], manifest['source'])
		if row['context'].get('candidate') != manifest['record']['candidate']:
			raise ValueError('stage candidate differs: ' + name)
		if row['command'] != command or row['context']['command'] != command:
			raise ValueError('stage command differs: ' + name)
		if name == 'cross' and row['context']['kind'] != 'producer':
			raise ValueError('cross requires its own producer receipt')
	return {'schema': 1, 'status': 'PASS', 'candidate': manifest['record']['candidate'],
		'source': manifest['source'], 'selection': child.identity(manifest), 'evidence': evidence}


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('command', choices=('select', 'finalize'))
	p.add_argument('--root', type=Path, default=ROOT)
	p.add_argument('--out', type=Path, required=True)
	p.add_argument('--stages', help='comma-separated canonical stages; required gates are added')
	p.add_argument('--selection', type=Path)
	p.add_argument('--evidence', type=Path)
	a = p.parse_args()
	result = {'status': 'FAIL'}
	try:
		if a.command == 'select':
			result = select(a.root, a.out.parent, a.stages.split(',') if a.stages else None)
		else:
			if not a.selection or not a.evidence: raise ValueError('selection and evidence required')
			result = finalize(json.loads(a.selection.read_text()), json.loads(a.evidence.read_text()), a.root)
	except Exception as e:
		result['error'] = repr(e)
	finally:
		a.out.parent.mkdir(parents=True, exist_ok=True)
		if a.out.exists(): raise ValueError('refusing to replace existing receipt')
		child.atomic_json(a.out, result)
	return int(result.get('status') == 'FAIL')


if __name__ == '__main__':
	raise SystemExit(main())
