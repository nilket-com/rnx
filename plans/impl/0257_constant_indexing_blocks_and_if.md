# Implementation plan 0257: constant indexing, blocks and if in an execution session

Status: Implemented

Paired ADR: ../adr/0257_constant_indexing_blocks_and_if.md.

1. Pin 84560a7c. Read the bounded constant evaluator and its limits in
   the execution core, the constant query path and its guards, the
   materialization in lowering, the legacy evaluator for the three forms,
   and the fixed-route inventory.
2. The design was reviewed before this plan was committed: evaluator
   state with locals keyed by variable identity and restored on every
   exit, the measured legacy index kinds and diagnostics, first true
   branch only, one shared budget charged before allocation, inline
   constant blocks through the same evaluator, no materialization change.
   Send a note only if the code forces a departure from it or an
   inventoried item would change.
3. Implement the three forms.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, the required stages, preflight and the commit checker. Bring
   the packet with the before and after class totals, the selected stages
   and the per-stage durations. The candidate is frozen once the packet is
   sent.

Results are added here by the implementer.

## Implementation and focused controls

The evaluator keeps a fallible local environment keyed by HIR variable
identity. Blocks truncate it on success or error. Local reads, constant
references and indexed subtrees pass through one copy helper which checks
expanded nodes, height and text/key bytes before cloning. Aggregate copies
and string construction also check the remaining byte budget before
allocation. Blocks charge structural entry; all evaluated expressions and
local copies share the existing budget. Untaken branches are not evaluated.

Indexing uses logical fields, not raw node offsets: vector/tuple signed or
unsigned indexes and object string keys. Ordinary blocks and inline const
blocks enter the same evaluator. Const-function/call/byte refusals and
query cycle/cache handling remain unchanged. No inventoried item or policy
is edited and no runtime operation or materialization route is added.

Focused core controls cover nested selection, independent repeated
materialization, shadowing, failure cleanup, inline blocks, no-else unit,
composed block/if containers and dead branches. Node and byte refusals are
exercised through the new forms; the large lexical-depth fixture reaches
the existing HIR MaxConstDepth guard first. A separate small copy control
checks the evaluator's own depth boundary and exact node/byte boundaries,
including unchanged budget state on refusal and cumulative copy charging.
The same oversized node expansion succeeds in an untaken branch.

Session differential controls pass for values, failed admission retaining
prior bindings, and twelve exact diagnostic/message/location pairs. Cycles
are refused through each new form. Unsupported patterns, assignment, loops
and pattern conditions have named refusals. Formatting and Rune clippy
pass. The three measured parity rows are Equal: Equal 173 -> 176, admission
refusals 6 -> 3. Every other row and reason is unchanged.

The committed constant_structure_miri inventory selects independent
materialization, partial-local failure cleanup and small exact copy-budget
controls. Large budget fixtures stay in ordinary tests. Miri and ordinary
final results are recorded in the publication packet after source review.

Review correction: a pattern condition is refused only if reached. Controls
cover both skipped and reached later pattern conditions, including no final
else. The non-Boolean condition is one of the twelve exact diagnostic and
location comparisons: both engines report input 1, line 1, column 9 for
`const A=if 1 {2}else{3};A`, so the existing whole-if span is retained.

The first final stopped at a stale domain-session test expecting the three
newly admitted forms to be refused. Those three cases now assert their
values in the existing positive test; the deferred-form test retains const
functions, Option construction and bytes. This is a test-only repair. The
complete ordinary final is rerun after amending the implementation.
