# Implementation plan 0269: admitting a session entry does not scan its arguments' data

Status: Proposed

Paired ADR: ../adr/0269_entry_admission_cost_independent_of_argument_size.md.

1. Pin b251e1b1. Read the formatting certificate and what it permits,
   the admission walk and its caller in the executor, both places the
   domain prepares an entry, how a session passes bindings as
   arguments, every producer of struct values in the store, and the
   fixed-route inventory.
2. Before editing, send a note with: what the check protects; a phase
   timing that splits the two preparations for the measured inputs;
   whether the second preparation is redundant and why; the
   alternatives considered with their cost on ordinary heap operations
   (for example a fact kept per store, a fact kept per value, or a
   proof from how the values were produced); the one proposed; the
   list of ways a struct becomes reachable that the ADR asks for; and
   every inventoried item touched, by name. If any is touched, send
   the note and stop.
3. Implement the design.
4. Add the controls and measurements of the ADR. Large fixtures run
   natively; Miri covers the ownership paths at small sizes, with no
   single case near the time limit.
5. Run Miri, a required final (complete if gate tooling changes),
   preflight and the commit checker. Bring the packet with the
   measurements before and after, the fixed-route result, the three
   slowest Miri cases and the per-stage durations. The candidate is
   frozen once the packet is sent.

Results are added here by the implementer.


## Implementation results (source review; final gates pending)

The accepted step-2 design is the Store-local census and one complete
preparation. No inventoried executor item or policy changes. The two
inventoried executor files are byte-identical to b251e1b1, which covers
all 40 source selectors.

### What admission protects and what is kept

A nominal with an untrusted effective native `DISPLAY_FMT` key cannot
be rendered with the fresh structural formatter as if it were the
legacy formatter. Refusal must precede effects. The unchanged exact
walk checks each reachable Struct's canonical hash (the parent hash for
a variant) against the program certificate. Builtin provider checks,
entry policy, host identities and absence, runtime certificate equality,
arity, Store identity and root generation checks remain independent.
Opaque `Host` identities are not script Structs.

`Store::list` and `Store::record` record nominal ids on their existing
Struct arms. The census is a lazily allocated bitset bounded by the
interner's single canonical capacity constant. At 4096 ids its bits use
512 bytes; the full census, with used-word bound and unknown flag, is
528 bytes on this host. A Store holds only an optional pointer until
its first Struct attempt. The existing no-Struct allocation-budget
control still passes, and new zero-budget Vec/Tuple/Object controls
allocate no census. Census allocation failure is an Allocation error,
not a hidden incomplete proof. Both Store bookkeeping queries include
the owned census allocation. Close/Drop frees it.

Recording is monotone, including failed payload attempts and dead
Structs. Every reachable Struct must first have been constructed in
this Store. Thus a certificate that permits the census permits every
reachable subset. Otherwise the original exact walk decides; an
unrelated denied type is not itself a refusal. No argument-result
cache or mutation invalidation is used. Unpublished/provisional ids
force that exact walk. No Value, Root, Handle, Type, Entry or Slot
layout changes.

The ordinary Vec/Tuple/Object arms perform no census read or write.
Push, set, insert, field mutation, retain/release, sorting, iterators,
native snapshot/commit and ownership/drop paths do not maintain the
census. The Object constructor still uses its existing valid-tag
branch, now separated from the cold Struct arm; its emitted cost must
pass the unchanged fixed-route bands before the full gates.

A test/debug-only assertion in the common admission transaction rejects
a Struct payload whose nominal was not recorded. The whole heap/domain
test suite exercises it. A negative control bypasses each tagged
payload constructor and requires that assertion before publication.
The fixed-route gate builds the ordinary rnx binary with `cargo build
--release`: neither cfg(test) nor debug_assertions is enabled, so the
assertion is absent from that gated binary. The candidate phase
measurements use engine-dev, whose debug assertions are enabled; its
assertion is present and paid for in those numbers. The baseline phase
export predates this assertion. The clone path admits
only copies of existing same-Store tagged payloads. Capture transfer
preserves an existing tag. Controls cover cloned and moved payloads,
including the old `Moved(Struct)` tag and explicit invalidation.

Construction, alias push/field replacement/object insertion, closures,
callback output from older domain members, host root construction,
changed certificates, separate Stores, unknown ids, enum-parent
identity, sharing/cycles, lazy allocation failure and pre-effect
refusals are controlled. Typed/dynamic host output produces builtin or
sealed Host values, not a new uncensused script Struct. Native commits
preserve existing tags or write scalar/builtin values. No producer of a
new script Struct bypasses the two original constructor paths.

### The single preparation and caller inventory

Before the move, `Executor::start_cross` had exactly one repository
caller: `ProgramDomain::start` in domain.rs. That caller already ran a
complete `Executor::prepare`, dropped its token, consumed the entry
lease and then called `start_cross`, which repeated preparation.
Nothing could execute or mutate arguments/certificate between them.

Baseline repository callers of `ProgramDomain::start` were:

- src/session/domain_session.rs: `DomainSession::eval_inner`, the sole
  product caller. It relied on before-effect admission, borrowed
  caller roots, terminal runtime errors and no retry on another engine.
- domain.rs tests: `tests::call` (the shared domain invocation helper),
  `typed_host_rejects_function_input_without_releasing_its_program`,
  `foreign_failure_borrows_its_defining_program`,
  `cancel_and_budget_leave_domain_case`,
  `missing_program_fails_before_foreign_effects`,
  `owner_publication_and_leave_failure`, and
  `refused_start_keeps_a_releasable_lease`. Each relied on the same
  complete entry admission before a token or effect; the explicit
  refused-start test also relied on retaining the lease on refusal.
- domain.rs `host_session_tests::call`,
  `restored_host_schema_is_checked_before_prepare_and_cannot_claim_plain`,
  and `a_host_value_cannot_enter_a_non_session_program`. These also
  relied on the domain's envelope/opaque checks preceding preparation.

No other repository caller was found. `ProgramDomain::start` remains
the public admission boundary for external users too. Added controls
call it through their `finish` helper and directly for success,
allocation failure and deep formatter refusal.

Now the non-inventoried cross setup takes members mutably, selects the
entry and performs exactly one complete, unchanged prepare. Only then
does a narrow Member method consume the lease. All-member host/epoch
validation, reservations and enter follow as before. The setup returns
one shared program-slice reborrow for both the sliced invocation and
its diagnostic owner, avoiding overlapping mutable/shared borrows.
No unchecked prepared flag or copied validator was introduced.
Refused/invalid preparation keeps the lease and arguments; later setup
has the existing post-admission failure semantics. Controls count one
preparation, require zero effects on refusal, retain the lease/roots,
retry once successfully and cover older-member callbacks. Existing
cancellation, owner/unwind and host-session controls still pass.

### Native phase measurements

These are quiet, serial, unoptimized engine-dev phase measurements,
not release-speed claims. The same committed measurement harness was
copied into isolated baseline/current exports; only scratch exports
contain phase timers. There are even small/large AB/BA rounds, equal
warmups and ten measured observations per layout/size/input. A push's
extra capacity is reserved outside timing. The held Dense is included
as in 0268; opaque native identities do not poison the Struct census.

Both complete preparations were first timed separately on the
published baseline. For `v.len()` with 5M integers they took 69.618 and
68.281 ms; nested formatting walks took 69.612 and 68.277 ms. For
`v[0]+1` they took 69.554 and 68.340 ms. The call is redundant, not an
independent proof.

After this change, medians for a whole input are:

| Input | 16 integers ms | 5M integers ms | 16 Structs ms | 1M Structs ms |
| --- | ---: | ---: | ---: | ---: |
| len | 0.710 | 0.722 | 0.779 | 0.792 |
| index + 1 | 0.720 | 0.717 | 0.803 | 0.815 |
| push one | 0.730 | 0.729 | 0.869 | 0.869 |
| older-input function | 0.767 | 0.758 | 0.837 | 0.837 |

Every small/large observed range overlaps its counterpart. All four
integer preparations take about 0.0022–0.0023 ms at either size.
Struct preparation takes 0.0034–0.0039 ms at either size, depending on
the input. Baseline large-input totals were 141.8–141.9 ms for the
integers and 1390–1394 ms for the Structs. The candidate's separately
measured collection phase is 0.017–0.018 ms, and restoration plus
writeback 0.021–0.023 ms. The old data walk was not moved into either
phase. The raw observations retain per-input ranges and setup costs.

### Remaining exact-walk limitation

A Store that has ever attempted a Struct type whose hash is denied by
the incoming certificate still walks every later argument graph, even
after that Struct is dead. Such denial comes from a non-default native
DISPLAY_FMT effective key, including a real protocol override or an
actual-key alias. This is not a claim of universal constant-time
admission when the census cannot prove safety.

The session compiler rejects statically known construction of that
denied type. The residual was therefore measured at the actual public
Executor/Store embedding boundary: the same `v.len()` entry and same
held vector, before and after host construction and disposal of an
issued, denied named nominal. With 5M integers, preparation changes
from 0.000417 ms (no walk) to 69.770 ms (one walk), while execution is
0.001276 and 0.006399 ms. With 16 integers it changes from 0.000331 to
0.002009 ms. No claim that this embedding case is fixed is made.
The exact fallback still accepts safe arguments and refuses a denied
Struct deep inside them before effects.

### Validation at source review

Native admission, domain and whole-heap controls pass, including the
constructor assertion, existing admission controls and cleanup tests.
Rune Clippy with warnings denied passes on the current source.
The measurement fixtures pass at small sizes. Both native phase roles
and the embedding fallback measurement pass. The new exact Miri
inventory has 19 small cases, including the four unchanged admission
controls; no large fixture is selected for Miri.

An extra exploratory root Clippy invocation (not a required-final stage)
reported existing style lints in unchanged legacy/root files. The
failed diagnostic and byte-identity proof for those files are retained;
no unrelated lint edits or allowances were added.

The paired fixed-route screen is next after source approval/signing,
before any full gates, with no exception or policy edit. Miri, required
final, preflight, checker and the final publication packet remain
pending. No final-gate or publication pass is claimed here.


Source review accepted the runtime boundary (seq 31151). The final
constructor edit binds the lazy box through match/Option::insert,
without a user-reachable unwrap. The focused admission/domain controls
and Rune Clippy pass after that edit. Native phase tables above were
collected on the reviewed source immediately before this non-panicking
binding adjustment; the committed candidate is measured again for the
final packet. The fixed-route screen must pass before Miri/full gates.

The native fallback fixture also puts a denied Struct behind a tuple
and object in the last element of the same 16- and 5M-element vectors.
It invokes an entry with a host effect first and requires Entry refusal,
zero effects, and the full four-node heap path. This closes the explicit
large negative control; it is separate from the safe-after-disposal
residual timings. The first final was intentionally interrupted to add
this omitted test-only case, and its owned payload release passed.
The runtime source and early release screen are unchanged. Actual-key
Miri and the ordinary required final run again on the amended commit;
no earlier test pass substitutes for either final gate.
