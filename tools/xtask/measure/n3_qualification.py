"""Replay only the pinned N3 CGU1 capture under its reviewed exact-cell qualification.

Raw profiles remain immutable. Every declared capture byte and per-cell core
function/call table is pinned before the three approved cells are normalized.
This is not a reusable cost allowance, a recapture, or a full-controller seal.
"""
import copy
from fractions import Fraction
import hashlib
import json
from pathlib import Path
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import callgrind
import callgrind_packet
import callgrind_rules
import gen_inputs
import role_evidence

TIP = 'ee28ecf2cdb484295765a221385ccf3a85749ab4'
CANDIDATE = 'cbc81682526bc8f2d49948e9bcfa47fce51e5da9'
ALLOWED = {'tools/xtask/measure/callgrind_rules.py',
	'tools/xtask/measure/test_callgrind_n1_cold.py',
	'tools/xtask/measure/n3_qualification.py',
	'tools/xtask/measure/test_n3_qualification.py',
	'tools/xtask/measure/qualifications/n3.json',
	'tools/xtask/measure/qualifications/n3-proof.json'}
EXPECTED = {'on/while': 2626, 'on/numeric': 3364, 'on/shared_rows': 27796}
POLICY = {'schema': 1, 'record': 'N3', 'baseline_milestone': 'N2', 'candidate_commit': 'cbc81682526bc8f2d49948e9bcfa47fce51e5da9', 'tip_commit': 'ee28ecf2cdb484295765a221385ccf3a85749ab4', 'entries': [{'rule': 'exact-scalar-entry-and-cold-preparation', 'cells': {'on/while': 2626, 'on/numeric': 3364}, 'calls': 1000, 'steady_Ir': {'on/while': 1000, 'on/numeric': 1000}, 'cold_Ir': {'on/while': 1626, 'on/numeric': 2364}}, {'rule': 'exact-unreachable-predecessor-analysis', 'cells': {'on/shared_rows': 27796}, 'calls': 1, 'steady_Ir': {'on/shared_rows': 0}, 'cold_Ir': {'on/shared_rows': 27796}}], 'proof': 'n3-proof.json', 'proof_sha256': '7b04585b01f30aa3be22bd9c2a6ff5166bbe3b4ac565ca11e45d681d066beb97'}


def require(ok, message):
	if not ok:
		raise ValueError(message)


def digest(value):
	return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def core(name):
	return 'experimental_core::' in name or 'experimental_native::' in name


def facts(profile):
	return dict(core=callgrind.core_total(profile), whole=profile['total_Ir'],
		core_functions_sha256=digest({n: r for n, r in profile['functions'].items() if core(n)}))


def pin(path):
	data = Path(path).read_bytes()
	return dict(sha256=hashlib.sha256(data).hexdigest(), bytes=len(data))


def qualify(tip, candidate, cell, expected):
	"""Normalize a reviewed cell only after exact live cost and call-table checks.

	This helper's expected facts come only from the hash-pinned reviewed proof in
	analyze. Other cells receive no normalization; raw whole costs stay checked.
	"""
	require(set(expected) == {'tip', 'candidate', 'reference'}, 'invalid cell proof')
	require(facts(tip) == expected['tip'] and facts(candidate) == expected['candidate'],
		'exact cell costs or core call table changed')
	delta = callgrind.core_total(candidate) - callgrind.core_total(tip)
	if cell in EXPECTED:
		require(delta == EXPECTED[cell], 'exact qualified cell delta changed')
	elif cell.startswith('off/'):
		require(delta == 0, 'native-off core changed')
	else:
		require(delta <= 0, 'unqualified positive core cell')
	require(tip['total_Ir'] > 0 and
		Fraction(candidate['total_Ir'] - tip['total_Ir'], tip['total_Ir']) <= Fraction(1, 1000),
		'raw whole-process bound exceeded')
	adjusted = copy.deepcopy(candidate)
	if cell in EXPECTED:
		# The reviewed allowance is the exact net core delta of this fixed capture,
		# not a symbol-name exemption. Preserve raw costs and call tables in output.
		for name in adjusted['functions'].keys() | tip['functions'].keys():
			if core(name):
				row = adjusted['functions'].setdefault(name, dict(self_Ir=0, calls=0))
				row['self_Ir'] = tip['functions'].get(name, {}).get('self_Ir', 0)
		adjusted['total_Ir'] -= delta
	return adjusted, dict(cell=cell, qualified_net_core_Ir=delta if cell in EXPECTED else 0,
		raw_tip=facts(tip), raw_candidate=facts(candidate), raw_differences=callgrind.core_differences(tip, candidate))


def tree_proof(repo, final_commit):
	def git(*args):
		return subprocess.check_output(['git', '-C', str(repo), *args])
	for rev in (TIP, CANDIDATE, final_commit):
		subprocess.run(['git', '-C', str(repo), 'verify-commit', rev], capture_output=True, check=True)
	require(git('rev-parse', final_commit + '^') == git('rev-parse', CANDIDATE + '^'),
		'qualification cannot be inherited by a later record')
	changed = git('diff', '--name-only', CANDIDATE, final_commit).decode().splitlines()
	require(set(changed) <= ALLOWED, 'runtime, harness, manifest or lock changed since capture')
	require(not git('diff', final_commit, '--', *sorted(ALLOWED)), 'uncommitted qualification code')
	return dict(measured_commit=CANDIDATE, final_commit=final_commit, changed=changed,
		unchanged_runtime=True, diff_sha256=hashlib.sha256(git('diff', '--binary', CANDIDATE, final_commit)).hexdigest())


def analyze(root, repo, final_commit, accounting):
	root, repo, accounting = (Path(p).resolve(strict=True) for p in (root, repo, accounting))
	tool = Path(__file__).parent
	policy = json.loads((tool / 'qualifications/n3.json').read_text())
	require(policy == POLICY, 'unreviewed N3 qualification policy')
	proof_path = tool / policy['proof'] if '/' in policy['proof'] else tool / 'qualifications' / policy['proof']
	require(pin(proof_path)['sha256'] == policy['proof_sha256'], 'reviewed N3 proof changed')
	proof = json.loads(proof_path.read_text())
	require(pin(accounting) == proof['accounting'], 'reviewed attribution changed')
	attribution = json.loads(accounting.read_text())
	require(attribution['candidate'] == CANDIDATE, 'attribution candidate changed')
	tree = tree_proof(repo, final_commit)
	spec = json.loads((root / 'SPEC.json').read_text())
	declared = callgrind_packet.declared(spec)
	require(spec['symbol_map'] is None and 'qualifications' not in spec, 'unexpected inherited qualification')
	require(set(declared) == set(proof['capture_files']), 'capture file declaration changed')
	for name, expected in proof['capture_files'].items():
		require(pin(root / name) == expected, 'original capture bytes changed: ' + name)
	for name, expected in proof['capture_context'].items():
		require(pin(root / name) == expected, 'original capture context changed')
	for row in attribution['assembly'].values():
		require(pin(row['assembly']['path']) == {k:row['assembly'][k] for k in ('sha256','bytes')}, 'reviewed Ready assembly changed')
	context = json.loads((root / 'CAPTURE-CONTEXT.json').read_text())
	roles = context['roles']
	require(all(r['commit'] == CANDIDATE for r in roles.values() if r['role'] == 'candidate'), 'measured candidate changed')
	require(all(r['commit'] == TIP for r in roles.values() if r['role'] == 'tip'), 'tip changed')
	role_checks = {name: role_evidence.check(repo, role) for name, role in roles.items()}
	def parsed(row):
		return callgrind.parse(root / row['profile'], spec['positions'])
	def checksum(row, expected):
		require((root / row['stdout']).read_text().splitlines()[0] ==
			f'CHECKSUM {expected} invocations=1000', 'live checksum differs')
	def guard(paths):
		return {key: str(root / path) for key, path in paths.items()}
	controls = {}
	for name, rows in spec['controls']['profiles'].items():
		for label, row in rows.items():
			value = 0 if label.endswith('0') and not label.endswith('100') else (300 if name == 'is' else 150)
			checksum(row, gen_inputs.checksum([value] * 1000))
		controls[name] = {label: parsed(row) for label, row in rows.items()}
	deltas, _ = callgrind_rules.assess_controls(controls, guard(spec['controls']['guard']))
	require(all(c['core'] == 0 for c in deltas.values()), 'core controls changed')
	phases = {}
	for mode, phase in spec['phases'].items():
		cells, raw = {}, {}
		for name, cell in phase['cells'].items():
			label = mode + '/' + name
			values = [int(x) for x in (root / cell['input_file']).read_text().splitlines()[:1000]]
			require(len(values) == 1000, 'incomplete input stream')
			expected = gen_inputs.checksum([gen_inputs.model(name, x) for x in values])
			for role in ('tip', 'candidate', 'reference'):
				checksum(cell[role], expected)
			before, after, reference = (parsed(cell[r]) for r in ('tip', 'candidate', 'reference'))
			require(facts(reference) == proof['cells'][label]['reference'], 'reference changed')
			adjusted, raw[name] = qualify(before, after, label, proof['cells'][label])
			cells[name] = dict(tip=before, candidate=adjusted, reference=reference,
				iterations=sum(values) if name in ('while', 'numeric', 'calls') else None, guard=guard(phase['guard']))
		phases[mode] = dict(assessment=callgrind_rules.assess_phase_with_controls(
			cells, controls, guard=guard(spec['controls']['guard'])), raw_accounting=raw)
	# All capture bytes must remain unchanged throughout the replay.
	require(all(pin(root / name) == expected for name, expected in proof['capture_files'].items()), 'capture changed during replay')
	require(all(pin(root / name) == expected for name, expected in proof['capture_context'].items()), 'capture context changed during replay')
	return dict(status='PASS',record='N3',policy=policy,tree_proof=tree,role_evidence=role_checks,
		phases=phases,accounting=pin(accounting),proof=pin(proof_path),
		operators={name:pin(tool/name) for name in ('n3_qualification.py','callgrind_rules.py')},
		scope='Existing 38-profile CGU1 replay only; raw costs retained; exact named N3 qualification; no recapture, inherited policy, CGU16 or full-controller seal')


if __name__ == '__main__':
	import argparse
	parser = argparse.ArgumentParser(description=__doc__)
	for name in ('capture','repo','final-commit','accounting','output'):
		parser.add_argument('--'+name,required=True)
	args = parser.parse_args()
	output = Path(args.output)
	require(not output.exists(), 'output already exists')
	try:
		result = analyze(args.capture,args.repo,args.final_commit,args.accounting)
	except BaseException as e:
		output.write_text(json.dumps(dict(status='FAIL',error=repr(e)),indent=2)+'\n')
		raise
	output.write_text(json.dumps(result,indent=2)+'\n')
	print('PASS N3 qualified replay; receipt',output)
