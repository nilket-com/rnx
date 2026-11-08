"""ADR 0233: admit I64 adapters and one outlined per-call preparation frame."""
import argparse
import json
from pathlib import Path
import subprocess
from reuse_instances import evaluate as original, inventory

PREFIX = 'rune::experimental_core::column::run::predicate::'
ADAPTERS = {PREFIX + name for name in ('i64_comparison_scalar', 'i64_comparison_avx2')}

PREPARE = 'rune::experimental_core::column::plan::prepare'


def evaluate(before, after):
	filtered = after.copy()
	admitted = {}
	for name in ADAPTERS | {PREPARE}:
		if before[name] == 0 and after[name] == 1:
			admitted[name] = 1
			del filtered[name]
	reviewed = original(before, filtered)
	return dict(status=reviewed['status'] if after[PREPARE] == 1 else 'FAIL',
		raw=original(before, after), reviewed=reviewed, admitted=admitted,
		outlined_per_call={PREPARE: after[PREPARE]})


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--baseline', required=True, type=Path)
	p.add_argument('--candidate', required=True, type=Path)
	p.add_argument('--out', required=True, type=Path)
	a = p.parse_args()
	rows = [inventory(subprocess.check_output(['nm', '-C', str(b)], text=True))
		for b in (a.baseline, a.candidate)]
	assert all(rows), 'empty column inventory'
	r = evaluate(*rows)
	r.update(baseline=dict(rows[0]), candidate=dict(rows[1]))
	a.out.parent.mkdir(parents=True, exist_ok=True)
	a.out.write_text(json.dumps(r, indent=2) + '\n')
	print(r['status'], a.out)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
