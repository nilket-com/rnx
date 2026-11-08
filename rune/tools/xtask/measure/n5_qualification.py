"""N5-only host-entry qualification; raw whole/process costs stay unadjusted."""
import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
import callgrind

TIP = '0fed676af917e5d66f5bde49280f550488d02d70'
CANDIDATE = 'f4af80a2c565f579347e9c04d87b1d9e03060ce0'
PLAN = '1be9bd3a8478e6768b4b69d4d95cfd37d498989f'
INVOKE = '<rune::experimental_core::execute::Executor>::invoke'
CHECKOUT = 'rune::experimental_core::region::cache::checkout'
CELLS = frozenset('on/' + k for k in ('while', 'numeric', 'calls', 'shared_rows'))
DIAGNOSTICS = frozenset(('scalar_while', 'range', 'b1_calls', 'n2_i64', 'n2_bool', 'n3_i64', 'n3_bool', 'n4_i64', 'n4_bool'))
ALLOWED = frozenset(('tools/xtask/measure/seal_evidence.py', 'tools/xtask/measure/test_seal_evidence.py', 'tools/xtask/measure/counter_controller.py', 'tools/xtask/measure/counter_packet.py', 'tools/xtask/measure/test_counter_native_mappings.py', 'tools/xtask/measure/callgrind_rules.py', 'tools/xtask/measure/n5_qualification.py',
	'tools/xtask/measure/test_n5_qualification.py', 'tools/xtask/measure/qualifications/n5.json',
	'tools/xtask/measure/qualifications/n5-proof.json', 'tools/xtask/src/measure_checks.rs'))
PROOF_SHA256 = '23164a58f61a0ea5338b8ea990ba6647901133806ed8ba4d0b85d2cd68832af4'
POLICY = dict(schema=1, record='N5', baseline_milestone='N4', candidate_commit=CANDIDATE,
	tip_commit=TIP, profile='cgu1', plan_commit=PLAN,
	entries=[dict(rule='exact-host-invocation-entry', symbol=INVOKE, calls=1000,
		cells={c:(6000 if c == 'on/shared_rows' else 8000) for c in CELLS}, diagnostic_cells={c:8000 for c in DIAGNOSTICS})],
	proof='n5-proof.json', proof_sha256=PROOF_SHA256)


def require(ok, message):
	if not ok:
		raise ValueError(message)


def checked_policy():
	root = Path(__file__).parent / 'qualifications'
	policy = json.loads((root / 'n5.json').read_text())
	require(policy == POLICY, 'unreviewed N5 policy')
	require(hashlib.sha256((root / policy['proof']).read_bytes()).hexdigest() == PROOF_SHA256,
		'N5 proof changed')
	return policy


def tree_proof(repo, final_commit):
	def git(*args):
		return subprocess.check_output(['git', '-C', str(repo), *args])
	for rev in (TIP, CANDIDATE, final_commit):
		subprocess.run(['git', '-C', str(repo), 'verify-commit', rev], capture_output=True, check=True)
	require(git('rev-parse', final_commit + '^').decode().strip() == PLAN,
		'N5 qualification cannot be inherited')
	changed = git('diff', '--name-only', CANDIDATE, final_commit).decode().splitlines()
	require(set(changed) <= ALLOWED, 'production/harness/lock changed since reviewed N5 source')
	require(not git('diff', final_commit, '--', *sorted(ALLOWED)), 'uncommitted N5 qualification')
	return dict(measured_commit=CANDIDATE, final_commit=final_commit, tip_commit=TIP,
		profile='cgu1', production_identical=True, tooling_only_delta=changed,
		delta_sha256=hashlib.sha256(git('diff', '--binary', CANDIDATE, final_commit)).hexdigest())


def validate_binding(proof):
	require(isinstance(proof, dict) and proof.get('measured_commit') == CANDIDATE
		and proof.get('tip_commit') == TIP and proof.get('profile') == 'cgu1'
		and proof.get('production_identical') is True
		and set(proof.get('tooling_only_delta', [])) <= ALLOWED,
		'missing exact N5 source/profile binding')


def vm_key(name):
	# Identity comparison only: do not project or rewrite any captured profile.
	return re.sub(r'invoke_args::<(true|false), (true|false), false>', r'invoke_args::<\1, \2>', name)


def core_functions(profile):
	result = {}
	for name, row in profile['functions'].items():
		if 'experimental_core::' in name or 'experimental_native::' in name:
			key = vm_key(name)
			value = result.setdefault(key, dict(self_Ir=0, calls=0))
			value['self_Ir'] += row['self_Ir']
			value['calls'] += row['calls']
	return result


def qualify_short(profiles, cell, proof):
	"""Only the exact reviewed steady invoke delta; cold accounting stays raw."""
	checked_policy(); validate_binding(proof)
	require(cell in DIAGNOSTICS and set(profiles) == {1000, 2000}, 'wrong N5 short cell/counts')
	differences = {}
	for n in (1000, 2000):
		require(set(profiles[n]) == {'tip','candidate'}, 'incomplete N5 short pair')
		left, right = (core_functions(profiles[n][r]) for r in ('tip','candidate'))
		require(left.get(INVOKE, {}).get('calls') == right.get(INVOKE, {}).get('calls') == n,
			'N5 host invoke count changed')
		require({k:v for k,v in left.items() if 'invoke_args' in k} == {k:v for k,v in right.items() if 'invoke_args' in k}, 'N5 VM closure identity changed')
		require(left.get(CHECKOUT, {}).get('calls') == right.get(CHECKOUT, {}).get('calls'), 'N5 checkout call count changed')
		differences[n] = {name:right.get(name, {}).get('self_Ir',0) - left.get(name, {}).get('self_Ir',0)
			for name in left.keys() | right.keys()}
	names = differences[1000].keys() | differences[2000].keys()
	steady = {name:differences[2000].get(name,0) - differences[1000].get(name,0) for name in names}
	require(steady.get(INVOKE) == 8000, 'N5 exact host-entry cost changed')
	require(all(delta <= 0 for name,delta in steady.items() if name != INVOKE),
		'N5 other positive steady core cost')
	require(steady.get(CHECKOUT,0) == 0 and all(delta == 0 for name,delta in steady.items() if 'invoke_args' in name),
		'N5 checkout or VM closure identity changed')
	deltas = {n:callgrind.core_total(profiles[n]['candidate']) - callgrind.core_total(profiles[n]['tip']) for n in (1000,2000)}
	return dict(status='PASS',cell=cell,raw_deltas=deltas,qualified_steady_Ir=8000,
		raw_steady_Ir=deltas[2000]-deltas[1000],cold_setup_delta=2*deltas[1000]-deltas[2000],
		steady_functions=steady,scope='Only +8 per HOST invocation; cold and whole costs unqualified')


def qualify(tip, candidate, cell, proof):
	"""Prepare a core-only view; stock whole/control bounds retain RAW costs."""
	checked_policy(); validate_binding(proof)
	if cell.startswith('off/'):
		require(cell.split('/',1)[1] in ('while','numeric','calls','fib','shared_rows'), 'unknown off cell')
		require(callgrind.core_total(tip) == callgrind.core_total(candidate), 'native-off core changed')
		return copy.deepcopy(candidate), dict(cell=cell,qualified_entry_Ir=0)
	require(cell in CELLS, 'cell outside pinned N5 host-entry policy')
	expected = POLICY['entries'][0]['cells'][cell]
	before, after = (p['functions'].get(INVOKE,{}) for p in (tip,candidate))
	require(before.get('calls') == after.get('calls') == 1000
		and after.get('self_Ir',0)-before.get('self_Ir',0) == expected, 'N5 exact host-entry cost/count changed')
	left, right = core_functions(tip), core_functions(candidate)
	for name in left.keys() | right.keys():
		if 'invoke_args' in name:
			require(left.get(name) == right.get(name), 'N5 VM closure identity changed')
	adjusted = copy.deepcopy(candidate)
	adjusted['functions'][INVOKE]['self_Ir'] -= expected
	return adjusted, dict(cell=cell,qualified_entry_Ir=expected,raw_core=callgrind.core_total(candidate),
		raw_total_Ir=candidate['total_Ir'],raw_differences=callgrind.core_differences(tip,candidate),
		scope='Whole-process cost remains raw; no other function receives an allowance')


def native_return_facts(root, spec):
	root = Path(root)
	proof = json.loads((Path(__file__).parent / 'qualifications/n5-proof.json').read_text())['native_return_validation']
	def checked(pin):
		data = Path(pin['path']).read_bytes()
		require(len(data) == pin['bytes'] and hashlib.sha256(data).hexdigest() == pin['sha256'], 'native return evidence changed')
		return data
	observed = json.loads(checked(proof['receipt']))
	trace = json.loads(checked(proof['trace']))
	context = json.loads((root / 'CAPTURE-CONTEXT.json').read_text())
	require(observed['status'] == trace['status'] == 'PASS' and observed['binary'] == proof['binary']
		and context['roles']['candidate-cgu1-on']['binary'] == proof['binary'], 'native return binary binding differs')
	checked(proof['binary']); checked(proof['source'])
	stream = checked(proof['stream'])
	require(stream == (root / spec['phases']['on']['cells']['fib']['input_file']).read_bytes(), 'native return input differs')
	events = trace['events']
	counts = dict(entries=len(events), completions=sum(e['status'] == 1 for e in events),
		replays=sum(e['status'] == 2 for e in events), slot_limit=64)
	require(all(counts[k] == proof[k] == observed[k] for k in ('entries','completions','replays'))
		and counts['entries'] == 1000 and counts['completions'] + counts['replays'] == 1000,
		'native entry/completion counts differ')
	require([e['root_argument'] for e in events] == [int(x) for x in stream.splitlines()[:1000]], 'native return arguments differ')
	return counts


def analyze(root, repo, final_commit):
	import callgrind_packet
	import callgrind_rules
	import gen_inputs
	root = Path(root)
	proof = tree_proof(repo, final_commit)
	spec = json.loads((root / 'SPEC.json').read_text())
	callgrind_packet.declared(spec)
	return_counts = native_return_facts(root, spec)
	for row in json.loads((root / 'RUNS.json').read_text()):
		for key in ('profile','stdout','stderr'):
			pin = row[key]; data = Path(pin['path']).read_bytes()
			require(len(data) == pin['bytes'] and hashlib.sha256(data).hexdigest() == pin['sha256'], 'original capture changed')
	require(spec['symbol_map'] is None and 'qualifications' not in spec, 'inherited qualification')
	def read(row):
		return callgrind.parse(root / row['profile'], spec['positions'])
	def checksum(row, expected):
		require((root / row['stdout']).read_text().splitlines()[0] ==
			f'CHECKSUM {expected} invocations=1000', 'live checksum differs')
	def guard(paths):
		return {key:str(root / path) for key,path in paths.items()}
	controls = {}
	for name, rows in spec['controls']['profiles'].items():
		for label,row in rows.items():
			value = 0 if label.endswith('0') and not label.endswith('100') else (300 if name == 'is' else 150)
			checksum(row, gen_inputs.checksum([value]*1000))
		controls[name] = {label:read(row) for label,row in rows.items()}
	deltas,_ = callgrind_rules.assess_controls(controls, guard(spec['controls']['guard']))
	require(all(row['core'] == 0 for row in deltas.values()), 'native-off controls changed')
	phases = {}
	for mode,phase in spec['phases'].items():
		cells,accounting = {},{}
		for name,cell in phase['cells'].items():
			values = [int(x) for x in (root / cell['input_file']).read_text().splitlines()[:1000]]
			require(len(values) == 1000, 'incomplete input stream')
			expected = gen_inputs.checksum([gen_inputs.model(name,x) for x in values])
			for role in ('tip','candidate','reference'):
				checksum(cell[role], expected)
			before,after,reference = (read(cell[r]) for r in ('tip','candidate','reference'))
			if mode == 'on' and name == 'fib':
				adjusted,accounting[name] = after,dict(cell='on/fib',qualified_entry_Ir=0,scope='N5-active fib is unqualified')
			else:
				adjusted,accounting[name] = qualify(before,after,mode+'/'+name,proof)
			cells[name] = dict(tip=before,candidate=adjusted,reference=reference,
				iterations=sum(values) if name in ('while','numeric','calls') else None,guard=guard(phase['guard']))
		phases[mode] = dict(assessment=callgrind_rules.assess_phase_with_controls(
			cells,controls,guard=guard(spec['controls']['guard']), mode=mode,
			native_returns=return_counts if mode == 'on' else None),raw_accounting=accounting)
	return dict(status='PASS',record='N5',policy=checked_policy(),tree_proof=proof,phases=phases,
		scope='CGU1 only; raw whole costs and off-core0; invoke-only exact8; no inherited policy or full-controller seal')
