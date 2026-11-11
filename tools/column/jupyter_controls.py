"""Build and run the installed-kernelspec contract with a pinned external client."""
import argparse
import os
from pathlib import Path
import subprocess
import sys
import traceback
import child_result as child

ROOT = Path(__file__).resolve().parents[2]


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args(); out = a.out.resolve(); out.mkdir(parents=True, exist_ok=False)
	r = dict(status='FAIL', checks=[])
	try:
		python = Path(os.environ.get('RNX_JUPYTER_PYTHON', str(ROOT/'target/jupyter-client/bin/python')))
		if not python.is_file(): raise ValueError('set RNX_JUPYTER_PYTHON to the pinned Jupyter client environment')
		commands = [
			['cargo','+1.98.1','build','--locked','--bin','rnx'],
			['cargo','+1.98.1','build','--locked','--manifest-path','jupyter/Cargo.toml','--bin','rnx-jupyter'],
			[str(python),'-B','tests/test_jupyter_execution.py',str(ROOT/'jupyter/target/debug/rnx-jupyter'),str(ROOT/'target/debug/rnx')],
		]
		for i, command in enumerate(commands):
			with (out/f'{i}.log').open('x') as log:
				done = child.run(command,root=ROOT,cwd=ROOT,env=dict(os.environ,PYTHONDONTWRITEBYTECODE='1',CARGO_BUILD_JOBS='4'),stdout=log,stderr=subprocess.STDOUT,directory=out/f'{i}-child',timeout=1800)
			r['checks'].append(dict(command=command,validation=done.validation))
			if done.returncode: raise RuntimeError(f'stage {i} failed')
		r['status']='PASS'
	except Exception:
		r['error']=traceback.format_exc()
	finally:
		child.publish(out/'RESULT.json',r)
		print(r['status'],out/'RESULT.json')
	return r['status']!='PASS'


if __name__=='__main__':sys.exit(main())
