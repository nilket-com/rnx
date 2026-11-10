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
	# Compatibility entry for retained 0238 controllers. Execution/cache logic is
	# shared with the explicit-inventory runner used by new records.
	from miri_exact import run as exact_run
	m = json.loads((ROOT / 'rune/tools/xtask/manifests/miri.json').read_text())
	names = ['experimental_core::domain::tests::' + n for n in DOMAIN]
	names += ['experimental_core::execute::sliced::sliced_tests::' + n for n in SLICED]
	inventory = dict(schema=1, toolchain=m['toolchain'], features=m['features'],
		miriflags='-Zmiri-strict-provenance', tests=[dict(name=n, ignored=False, timeout_seconds=10800) for n in names])
	return exact_run(ROOT, 'rune', inventory, ROOT/'target/miri-exact-cache')
