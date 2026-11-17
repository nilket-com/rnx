"""A candidate's record identity: its plan, baseline and changed paths.

Stage selection and finalization belong to the Rust controller (tools/final)."""
import re
import subprocess


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
		'phase': match[1], 'paths': paths}
