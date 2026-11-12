# Implementation plan 0261: a final keeps no bulk of its own and can run exactly what is required

Status: Proposed

Paired ADR: ../adr/0261_finals_keep_no_bulk_and_select_what_is_required.md.

1. Pin 66e72772. Read gate selection and finalization, the driver, the
   Clippy comparison and suite stage producers and their baseline
   handling, the fixed-route gate with its anchor reuse and vector pool,
   and the fixed-route policy.
2. Before editing, send a note: how the required set is computed and
   whether policy can state when a complete final is needed; every
   directory a complete final leaves in its own directory with sizes and
   readers; how an anchor cache entry is keyed, what it refers to, how
   many entries exist and what they hold; what a miss costs; the proposed
   cache layout and bound; anything in the ADR the code makes wrong. If
   the fixed-route policy file, a band or an inventoried item would
   change, send that instead and stop.
3. Implement the required mode, ownership of baseline checkouts, the
   self-contained anchor cache and the release of fixed-route payload.
4. Add the controls of the ADR.
5. Run a final in required mode and a complete final, preflight and the
   commit checker. Bring the packet with both results, the size of each
   final's directory afterwards, free space before and after, and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Implementation and development evidence

Boundary accepted in seq30909. Ordinary final defaults to required mode;
complete and explicit selection remain separate, and the gate computes
publication eligibility. Required mode uses the existing required-stage rule
and prerequisite closure, not another path list. No complete-on-tooling-change
rule was introduced. accept.py explicitly uses complete/fresh mode.

Clippy producers put exports and tar archives in an assigned per-stage root;
private build targets remain below those exports. Shared build caches are
unchanged. The fixed-route payload likewise has an assigned root. Readers run
before snapshot/release; retained semantic receipts describe the payload's
lifetime and a release sidecar links the final manifest and release record.

The v2 anchor cache holds its global lock through lookup, verified publication
and scoring. Its ownership journal supports interrupted creation/publication/
release. A completed bundle copies the binary, profiles, fixtures and stdout;
all artifact paths are internal. Provenance uses digests, not a live reference
to an old final. Publication uses fsynced files/directories and atomic rename.
The bound is one completed entry plus one pending replacement, with recorded
ownership for removal. Missing/altered bytes produce a miss and rebuild;
root identity/symlink violations fail closed. The fixed-route result records
hit/miss/migrated, the miss reason and anchor build/total seconds. Measurement
recipes, policy, bands and inventoried source were not changed.

Development controls: 277 Python tests PASS (5.343 s), also under the final
supervision environment. Coverage includes required/complete/explicit scope,
producer closure, source-independent cache hits after deleting the producing
source, full scored-row equality on miss/hit, changed/missing payload and
metadata, replacement failure, one-entry eviction, interrupted publication,
lock exclusion through scoring, root identity/symlink refusal and early
baseline removal failing the reader. A development launcher initially omitted
the producer-group directory; creating that required directory repaired the
launcher without changing production code.

Authorized one-time baseline cleanup of 0260 FINAL-v1..v5 removed 22 explicitly
listed roots: 4,788,748,288 allocated bytes inventoried; filesystem free space
increased by 4,481,323,008 bytes. Each root was checked against cache references
and live process paths before deletion. Receipt:
target/0261/BASELINE-RELEASE.json. Fixed-route payload and all legacy cache
entries remain until matching-entry migration or rebuild has been verified;
legacy retirement will be listed separately. No shared Cargo or Miri cache was
removed.

Source review precedes signing. Both required and complete signed finals,
post-release revalidation, final directory sizes and cache before/after state
remain required for the packet.

Source review seq30914 accepted, with two changes now applied: stage bulk-root
requirements are declared beside prerequisites in accept.py and consumed by
the driver; a catalogue control checks assigned_root producer coverage. Cache
lookup reason and legacy-import outcome (including failure class) are separate
reporting fields. An explicit import that cannot verify now falls through to
a fresh capture for any ValueError, rather than only binary/recipe mismatch;
no invalid import is trusted. No second source review required.


Required-v1 passed on 8330e561; complete-v1 stopped at the existing async
string-classification interruption test: its sleep-based SIGINT arrived without
an observed readiness boundary, and stderr reported budget exhaustion. Its
failed receipt and release result remain recorded; no passed retry substitutes
for it. Review seq30920 authorized a test-only readiness repair.

The repaired test waits for output from the evaluated input before SIGINT,
checks the exact interruption diagnostic and a subsequent usable input, and
asserts the synchronous generated wrapper through :debug. The original input
without the readiness print was separately observed to generate the same
synchronous wrapper. Suppressing SIGINT in a one-off negative control fails
with "input became ready but was not interrupted". All 50 consecutive runs
with four CPU-load processes passed, without retries. Evidence:
target/0261/ASYNC-REPAIR-CONTROLS-v1/RESULT.json. Readiness and interruption
have separate bounded failure diagnostics; the outer test has a 20-second
limit. No runtime source changed.

The neighboring sleep-helper tests remain unchanged: ctrl_c_ends_a_run_pending_on_a_future,
ctrl_c_ends_an_eval_pending_on_a_future,
ctrl_c_ends_a_session_input_pending_on_a_future_and_the_next_input_runs,
after_an_interrupted_input_the_next_input_runs,
a_synchronous_session_input_in_a_loop_is_still_interrupted_within_a_slice,
native_heavy_session_work_is_interrupted_and_the_next_input_works, and
declaring_an_async_function_does_not_make_the_input_that_declares_it_async.
Legacy session clearing of an interrupt before arming remains a product-policy
follow-up; this repair neither changes that behavior nor claims a complete
causal proof from the failed test's output alone.
