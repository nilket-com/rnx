#!/usr/bin/env python3
"""0197: actual CLI comparison under one exclusive measurement lease.

All expected checksums come from live runs. This is a finite benchmark, not a
verification scheduler: every child is waited on; there are no status polls.
"""
import argparse
import fcntl
import hashlib
import json
import math
import os
from pathlib import Path
import random
import re
import statistics
import struct
import subprocess
import sys
import time
import tomllib

sys.dont_write_bytecode = True
HERE = Path(__file__).resolve().parent
MODES = ("legacy", "execution-off", "execution-on", "luajit")
KERNELS = ("fib", "loops", "numeric_calls", "shared_rows", "awfy_sieve", "awfy_permute")
SEED = 19720261030
GROUP = "{cpu_core/instructions/u,cpu_core/cpu-cycles/u}"


def require(ok, message):
	if not ok:
		raise ValueError(message)


def digest(path):
	return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def document(path, data):
	Path(path).write_text(json.dumps(data, indent=2, allow_nan=False) + "\n")


def text_command(args):
	run = subprocess.run(args, capture_output=True, check=True, text=True)
	return run.stdout + run.stderr


def clean_revision(root):
	require(not text_command(["git", "-C", str(root), "status", "--porcelain", "--untracked-files=no"]).strip(), "measurement source must be committed and clean")
	return text_command(["git", "-C", str(root), "rev-parse", "HEAD"]).strip()


def generated(kernel, rounds, destination):
	require(0 <= rounds <= 2**31, "round count outside exact integer domain")
	# All intermediate arithmetic is below 2**53, including checksum addition.
	# numeric_calls' largest multiply/add is below 18 million; shared_rows'
	# encoded row sum is below 20 million. AWFY returns 669 or 8660.
	for suffix in ("rn", "lua"):
		prefix = (HERE / "kernels" / f"{kernel}.{suffix}").read_text()
		if suffix == "rn":
			body = f'\npub fn main(args){{let checksum=0;for i in 0..{rounds}{{checksum=(checksum+kernel(i%97+1))%1000000007;}}checksum}}\n'
		else:
			body = f'\nlocal checksum=0;for i=0,{rounds - 1} do checksum=(checksum+kernel(i%97+1))%1000000007 end\nio.write(string.format("%.0f\\n",checksum))\n'
		(destination / f"main.{suffix}").write_text(prefix + body)


def validate_child(mode, done, debug, label):
	require(mode in MODES, 'unknown benchmark mode')
	require(done.returncode == 0, f'child failed: {label}')
	if debug:
		require(mode in ('execution-off', 'execution-on'), 'route proof requires execution mode')
		expected = f'rnx: engine=execution native={"on" if mode == "execution-on" else "off"}\n'.encode()
		require(done.stderr == expected, f'kernel refused or emitted diagnostics: {label}')
	else:
		require(not done.stderr, f'child emitted diagnostics: {label}')


def compare_checksum(expected, observed, label):
	require(re.fullmatch(rb'[0-9]+\n', observed) is not None, f'invalid checksum output: {label}')
	require(expected is None or observed == expected, f'live checksum differs: {label}')
	return observed


def validate_capture(wall, startup, label):
	require(math.isfinite(wall) and wall >= 0.5, f'capture too short: {label}')
	require(math.isfinite(startup) and 0 <= startup / wall < 0.05, f'startup exceeds five percent: {label}')


def main(args):
	args.out.mkdir(parents=True, exist_ok=False)
	try:
		return measure(args)
	except BaseException as error:
		document(args.out / "RESULT.json", dict(status="FAIL", error=repr(error)))
		print(f"FAIL routing benchmark; receipt {args.out / 'RESULT.json'}", flush=True)
		raise


def measure(args):
	# The lease remains held through calibration, captures and final validation.
	with args.lock.open("a+") as lease:
		fcntl.flock(lease, fcntl.LOCK_EX | fcntl.LOCK_NB)
		require(args.cpu in os.sched_getaffinity(0), "measurement CPU unavailable")
		os.sched_setaffinity(0, {args.cpu})
		require(os.getloadavg()[0] <= 2.0, "measurement host not quiet")
		available = next(int(line.split()[1])*1024 for line in Path('/proc/meminfo').read_text().splitlines() if line.startswith('MemAvailable:'))
		require(available >= 8*1024**3, "measurement memory unavailable")
		pin = tomllib.loads((HERE.parent.parent / 'Cargo.toml').read_text())['dependencies']['rune']['rev']
		source = dict(rnx=clean_revision(HERE.parent.parent), rune=clean_revision(args.rune_root))
		require(source['rune'] == pin, 'Rune parser/analysis source differs from rnx pin')
		sys.path.insert(0, str(args.rune_root / 'tools/xtask/measure'))
		import perf_parse
		import counter_assess
		import counter_cell
		import measurement_readiness
		import measurement_runtime
		require(not any(name.startswith(('LD_', 'RUNE_')) for name in os.environ), 'undeclared loader or Rune environment override')
		perf_policy=measurement_runtime.check_perf_policy(Path.home())
		readiness_before=measurement_readiness.require_ready(measurement_readiness.snapshot(args.cpu),max_load=2.0,min_available_bytes=8*1024**3)
		governor_path=Path(f'/sys/devices/system/cpu/cpu{args.cpu}/cpufreq/scaling_governor')
		governor=governor_path.read_text().strip()
		require(governor=='performance','measurement CPU governor must be performance')
		dependencies=measurement_runtime.dependency_snapshot([args.rnx,args.luajit,'/usr/bin/perf','/usr/bin/timeout'])
		build=json.loads(args.build_receipt.read_text())
		require(build['commit']==source['rnx'] and build['binary']['sha256']==digest(args.rnx),'build receipt does not bind the measured source/binary')
		document(args.out/'BUILD.json',build)
		identities = dict(source=source, cpu=args.cpu, affinity=sorted(os.sched_getaffinity(0)), seed=SEED,
						  rnx=dict(path=str(args.rnx), sha256=digest(args.rnx), version=text_command([str(args.rnx),'version'])),
						  luajit=dict(path=str(args.luajit), sha256=digest(args.luajit), version=text_command([str(args.luajit),'-v'])),
						  perf=text_command(['/usr/bin/perf','--version']), counters=GROUP,governor=governor,perf_policy=perf_policy,dependencies=dependencies,readiness_before=readiness_before,
						  sources={str(p.relative_to(HERE)):digest(p) for p in sorted((HERE/'kernels').glob('*'))},
						  rune_analysis={name:digest(args.rune_root/'tools/xtask/measure'/name) for name in ['perf_parse.py','counter_assess.py','counter_cell.py','measurement_readiness.py','measurement_runtime.py','analysis_runtime.py']})
		document(args.out/'IDENTITIES.json', identities)
		rng=random.Random(SEED)
		# rnx rejects usize::MAX because legacy reserves it as an unlimited sentinel.
		budget=str(2**(8*struct.calcsize('P'))-2)
		env=dict(HOME=str(Path.home()),PATH='/usr/bin:/bin',LANG='C.UTF-8',LC_ALL='C',TERM='dumb',NO_COLOR='1',RNX_CONFIG='')
		document(args.out/'ENVIRONMENT.json',env)

		def command(mode, directory, debug=False):
			if mode=='luajit':return [str(args.luajit),str(directory/'main.lua')]
			cmd=[str(args.rnx),'--color=never','run','--budget',budget]
			cmd += ['--native=on' if mode=='execution-on' else '--native=off']
			if mode=='legacy':cmd+=['--engine=legacy']
			if debug:cmd+=['--debug-engine']
			return cmd+[str(directory/'main.rn')]

		def run(mode, directory, label, counters=True, debug=False):
			stem=directory/label
			cmd=command(mode,directory,debug)
			if counters:cmd=['/usr/bin/perf','stat','--no-scale','-x',';','-o',str(stem)+'.perf','-e',GROUP,'--']+cmd
			started=time.monotonic_ns()
			done=subprocess.run(['/usr/bin/timeout','--kill-after=5s','900s']+cmd,env=env,stdin=subprocess.DEVNULL,capture_output=True)
			wall=(time.monotonic_ns()-started)/1e9
			Path(str(stem)+'.stdout').write_bytes(done.stdout);Path(str(stem)+'.stderr').write_bytes(done.stderr)
			row=dict(mode=mode,code=done.returncode,wall=wall,command=cmd)
			document(str(stem)+'.json',row)
			validate_child(mode, done, debug, stem)
			if counters:
				parsed=perf_parse.parse(perf_parse.decode(Path(str(stem)+'.perf').read_bytes()),'B1',counter_cell.EVENTS,counter_cell.GROUPS)
				ok,why=perf_parse.probe_ok(parsed);require(ok,f'counters unavailable: {stem}: {why}')
				row.update({name:counter_assess.count(parsed['events'][name]['value']) for name in ('instructions','cycles')})
				document(str(stem)+'.json',row)
			return row,done.stdout

		# Measure process + compiler startup with the same perf wrapper as data.
		startup=args.out/'startup';startup.mkdir()
		(startup/'main.rn').write_text('pub fn main(args){}\n');(startup/'main.lua').write_text('return\n')
		startup_rows={mode:[] for mode in MODES}
		order=[(repeat,mode) for repeat in range(5) for mode in MODES];rng.shuffle(order)
		for repeat,mode in order:
			row,stdout=run(mode,startup,f'{repeat}-{mode}')
			require(not stdout,'startup control returned output');startup_rows[mode].append(row)
		startup_median={mode:statistics.median(row['wall'] for row in rows) for mode,rows in startup_rows.items()}
		# Conservative: maximum startup median versus the fastest kernel mode.
		target=max(0.5,20*max(startup_median.values()))
		results={}
		for kernel in KERNELS:
			root=args.out/kernel;root.mkdir();rounds=1
			compile_startup=root/'startup';compile_startup.mkdir();generated(kernel,0,compile_startup)
			kernel_startup={mode:[] for mode in MODES}
			startup_order=[(repeat,mode) for repeat in range(5) for mode in MODES];rng.shuffle(startup_order)
			for repeat,mode in startup_order:
				row,stdout=run(mode,compile_startup,f'{repeat}-{mode}')
				require(stdout==b'0\n','full-source startup control changed checksum')
				kernel_startup[mode].append(row['wall'])
			kernel_startup_median={mode:statistics.median(rows) for mode,rows in kernel_startup.items()}
			target=max(0.5,20*max(kernel_startup_median.values()))
			while True:
				directory=root/f'calibration-{rounds}';directory.mkdir();generated(kernel,rounds,directory)
				rows={};expected=None
				order=list(MODES);rng.shuffle(order)
				for mode in order:
					row,stdout=run(mode,directory,mode)
					expected=compare_checksum(expected,stdout,f'{directory} {mode}')
					rows[mode]=row
				fastest=min(row['wall'] for row in rows.values())
				if fastest>=target:break
				rounds=max(rounds+1,min(rounds*8,math.ceil(rounds*target/fastest*1.1)))
			for mode in ['execution-off','execution-on']:
				_,stdout=run(mode,directory,mode+'-route',counters=False,debug=True)
				require(stdout==expected,'route witness changed checksum')
			captures={mode:[] for mode in MODES}
			schedule=[(repeat,mode) for repeat in range(5) for mode in MODES];rng.shuffle(schedule)
			document(root/'SCHEDULE.json',schedule)
			for repeat,mode in schedule:
				row,stdout=run(mode,directory,f'repeat-{repeat}-{mode}')
				compare_checksum(expected,stdout,f'{directory} {mode}')
				validate_capture(row['wall'],kernel_startup_median[mode],f'{directory} {mode}')
				captures[mode].append(row)
			summary={}
			for mode,rows in captures.items():
				summary[mode]={}
				for metric in ('wall','instructions','cycles'):
					samples=[row[metric] for row in rows];median=statistics.median(samples)
					summary[mode][metric]=dict(samples=samples,median=median,spread=(max(samples)-min(samples))/median,
						 ratio_to_luajit=median/statistics.median(row[metric] for row in captures['luajit']))
				summary[mode]['cycle_soft_rule_vs_luajit']=counter_assess.assess_cycles([row['cycles'] for row in rows],[row['cycles'] for row in captures['luajit']])
			results[kernel]=dict(rounds=rounds,checksum=expected.decode().strip(),summary=summary,startup_medians=kernel_startup_median,fastest_target_seconds=target,native_on_cycle_soft_rule_vs_off=counter_assess.assess_cycles([row['cycles'] for row in captures['execution-on']],[row['cycles'] for row in captures['execution-off']]))
			document(root/'RESULT.json',dict(status='PASS',**results[kernel]))
		require(identities['rnx']['sha256']==digest(args.rnx) and identities['luajit']['sha256']==digest(args.luajit),'benchmark binary changed')
		require(source==dict(rnx=clean_revision(HERE.parent.parent),rune=clean_revision(args.rune_root)),'benchmark source changed')
		readiness_after=measurement_readiness.require_ready(measurement_readiness.snapshot(args.cpu),max_load=2.0,min_available_bytes=8*1024**3)
		require(readiness_before['observation']['core_throttle_count']==readiness_after['observation']['core_throttle_count'],'thermal throttle count changed')
		require(governor==governor_path.read_text().strip(),'measurement governor changed')
		require(dependencies==measurement_runtime.dependency_snapshot([args.rnx,args.luajit,'/usr/bin/perf','/usr/bin/timeout']),'dynamic dependencies changed')
		document(args.out/'RESULT.json',dict(status='PASS',source=source,results=results,startup_medians=startup_median,readiness_after=readiness_after,
				 minimum_repeats=5,cycle_soft_rule='ratio > 1 + max(2*LuaJIT spread, 0.02)',
				 native_default='pending user decision',size_and_build_cost='separate exact-source build receipts'))
		print(f'PASS routing benchmark; receipt {args.out / "RESULT.json"}',flush=True)


if __name__=='__main__':
	parser=argparse.ArgumentParser(description=__doc__)
	for name in ['rnx','luajit','rune-root','out','build-receipt']:
		parser.add_argument('--'+name,type=lambda value:Path(value).resolve(),required=True)
	parser.add_argument('--cpu',type=int,required=True)
	parser.add_argument('--lock',type=Path,default=Path('/tmp/rnx-runtime-bench.lock'))
	main(parser.parse_args())
