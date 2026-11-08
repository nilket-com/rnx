"""ADR 0229: one reviewed sibling; preserve the raw ADR 0228 instance verdict."""
import argparse
import json
from pathlib import Path
import subprocess
from reuse_instances import evaluate as original, inventory

SIBLING = 'rune::experimental_core::column::run::masked_float::run::<&mut rnx_polars::column::script::never_canceled>'


def evaluate(before, after):
	raw = original(before, after)
	filtered = after.copy()
	allowed = before[SIBLING] == 0 and after[SIBLING] == 1
	if allowed:
		del filtered[SIBLING]
	reviewed = original(before, filtered)
	return dict(status='PASS' if allowed and reviewed['status'] == 'PASS' else 'FAIL',
		raw=raw, reviewed=reviewed, sibling=SIBLING, sibling_count=after[SIBLING],
		baseline_sibling_count=before[SIBLING])


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--baseline', type=Path, required=True)
	p.add_argument('--candidate', type=Path, required=True)
	p.add_argument('--out', type=Path, required=True)
	a = p.parse_args()
	rows = [inventory(subprocess.check_output(['nm', '-C', str(b)], text=True)) for b in (a.baseline, a.candidate)]
	assert all(rows), 'empty inventory'
	r = evaluate(*rows)
	r.update(baseline=dict(rows[0]), candidate=dict(rows[1]))
	a.out.parent.mkdir(parents=True, exist_ok=True)
	a.out.write_text(json.dumps(r, indent=2) + '\n')
	print(r['status'], a.out)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
