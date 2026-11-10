"""Exact-test Miri shards for ADR 0238, using the durable ADR 0210 runner."""
import importlib.util
import json
from pathlib import Path
import sys
from concurrent.futures import ThreadPoolExecutor

ROOT = Path(__file__).resolve().parents[2]
HERE = ROOT / 'rune/tools/xtask/supplemental'
sys.path.insert(0, str(HERE))
_spec = importlib.util.spec_from_file_location('cross_program_miri_runner', HERE / 'run.py')
runner = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(runner)

DOMAIN = (
	'cancel_and_budget_leave_domain_reusable',
	'close_releases_programs_despite_outstanding_handles',
	'closure_calls_back_into_entry_program',
	'collection_allocation_failure_releases_no_programs',
	'collection_traces_captures_nested_collections_and_cycles',
	'context_epochs_fail_closed_but_old_snapshot_stays_compatible',
	'domain_brand_and_recycled_generation_reject_old_handles',
	'drain_failure_before_and_after_consumption_is_transactional',
	'dropped_handles_and_retained_aliases_drain_before_collect',
	'foreign_failure_borrows_its_defining_program',
	'foreign_sort_and_iterator_callbacks_obey_slices',
	'leases_are_reported_and_released_explicitly',
	'missing_program_fails_before_foreign_effects',
	'other_context_and_failed_install_invalidate_admission',
	'owner_publication_and_leave_failures_keep_stacks_coherent',
	'refused_start_keeps_a_releasable_lease',
	'returned_scalar_function_and_closure_keep_defining_program',
	'three_programs_and_recursive_foreign_calls',
)
SLICED = (
	'sliced_drop_and_cancel_release_continuations_and_allow_reuse',
	'sliced_error_keeps_location_chain_and_last_charge',
	'sliced_host_debits_restore_ambient_budget_on_success_panic_and_invalid_balance',
	'sliced_output_is_not_replayed_after_resume_or_failure',
	'sliced_partition_preserves_results_and_fuel',
	'sliced_pending_cancel_is_consumed_only_by_execution',
)


def run(out):
	m = json.loads((ROOT / 'rune/tools/xtask/manifests/miri.json').read_text())
	manifest = dict(stable='1.98.1', nightly=m['toolchain'], features={'core': m['features']}, subsets=[])
	for name, prefix, names in (
		('domain', 'experimental_core::domain::tests::', DOMAIN),
		('sliced', 'experimental_core::execute::sliced::sliced_tests::', SLICED),
	):
		inventory = [prefix + name for name in names]
		manifest['subsets'].append(dict(name=name, feature='core', selector=prefix,
			exact=False, inventory=inventory, shards=[[name] for name in inventory],
			ignored=[], timeout_seconds=10800))
	env = runner.environment()
	env['MIRIFLAGS'] = '-Zmiri-strict-provenance'
	identity = runner.source_identity(ROOT, manifest, env)
	# Existing 0210 admission: 4 GiB measured peak rounded +25% = 5 GiB/job.
	limits = runner.admit(*runner.resources(), 4 * 1024**3, requested=6)
	with runner.Run(out / 'shards', identity, False) as run:
		prepared = runner.stage(run, 'prepare', lambda path: runner.preparation(path, ROOT, manifest, env, 'core'))
		jobs = runner.selection(manifest, {'core': prepared})
		def job(spec):
			return runner.stage(run, spec['name'], lambda path: runner.semantic(path, ROOT, manifest, env, spec, prepared))
		with ThreadPoolExecutor(max_workers=limits['workers']) as pool:
			rows = list(pool.map(job, jobs))
		run.begin('prepare')  # Revalidate the prepared harness after all consumers exit.
		if runner.source_identity(ROOT, manifest, env) != identity:
			raise ValueError('source/toolchain changed')
	return dict(resources=limits, jobs=rows, covered_names=sorted(n for row in rows for n in row['names']),
		peak_interpreter_overlap=runner.overlap(rows))
