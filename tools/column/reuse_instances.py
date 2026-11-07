"""ADR 0228: forbid duplicate column instances and new hot helpers from the reuse entry."""
import argparse
from collections import Counter
import json
from pathlib import Path
import subprocess

OLD = 'rnx_polars::column::script::map::{closure#0}::{closure#2}'
NEW = 'rnx_polars::column::script::never_canceled'
ALLOWED = ('::allocate_batch_reusing', '::execute_reusing', '::attempt_reusing')


def inventory(text):
	result = Counter()
	for line in text.splitlines():
		parts = line.split(maxsplit=2)
		if len(parts) != 3 or parts[1] not in {'t', 'T', 'w', 'W'}:
			continue
		name = parts[2]
		if 'rune::experimental_core::column::' not in name:
			continue
		# Only the explicitly reviewed cancellation type rename is normalized.
		name = name.replace(OLD, NEW)
		result[name] += 1
	return result


def family(name):
	# Drop glue is per concrete owner type, not a cancellation-generic driver.
	return name if name.startswith('core::ptr::drop_glue::<') else name.split('::<', 1)[0]


def per_call(name):
	return name == 'core::ptr::drop_glue::<rune::experimental_core::column::run::flat::Output>' or family(name) in {
		'<rune::experimental_core::column::PreparedColumn>::allocate_batch',
		'<rune::experimental_core::column::run::flat::FlatBatch>::run_flat',
		'core::ptr::drop_glue',
	}


def reuse_entry(name):
	return any(family(name).endswith(entry) for entry in ALLOWED)


def evaluate(before, after):
	added = after - before
	frames = {name: count for name, count in added.items() if per_call(name)}
	new_helpers = {name: count for name, count in added.items()
		if not per_call(name) and not reuse_entry(name)}
	# Compare all generic argument tuples, not just symbol addresses: the linker
	# may give two instantiations one address while their caller sets still differ.
	groups = []
	for inventory in (before, after):
		counts = Counter()
		for name, count in inventory.items():
			if '::<' in name and not reuse_entry(name):
				counts[family(name)] += count
		groups.append(counts)
	duplicates = {name: {'baseline': groups[0][name], 'candidate': count}
		for name, count in groups[1].items()
		if count > groups[0][name] and not (name == 'core::ptr::drop_glue::<rune::experimental_core::column::run::flat::Output>' and count == groups[0][name] + 1) and not (groups[0][name] == 0 and count == 1
			and any(family(f) == name for f in frames))}
	return {'status': 'FAIL' if new_helpers or duplicates else 'PASS',
		'new_helpers': new_helpers, 'duplicate_instances': duplicates,
		'outlined_per_call': frames, 'removed': dict(before - after)}


def main():
	p = argparse.ArgumentParser(description=__doc__)
	p.add_argument('--baseline', required=True, type=Path)
	p.add_argument('--candidate', required=True, type=Path)
	p.add_argument('--out', required=True, type=Path)
	a = p.parse_args()
	lists = [inventory(subprocess.check_output(['nm', '-C', str(b)], text=True)) for b in (a.baseline, a.candidate)]
	assert all(lists), 'empty column inventory'
	r = evaluate(*lists)
	r.update(baseline=dict(lists[0]), candidate=dict(lists[1]))
	a.out.parent.mkdir(parents=True, exist_ok=True)
	a.out.write_text(json.dumps(r, indent=2) + '\n')
	print(r['status'], a.out)
	return r['status'] != 'PASS'


if __name__ == '__main__':
	raise SystemExit(main())
