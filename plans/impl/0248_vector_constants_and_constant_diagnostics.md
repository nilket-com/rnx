# Implementation plan 0248: vector constants and constant diagnostics

Status: Proposed

Paired ADR: ../adr/0248_vector_constants_and_constant_diagnostics.md.

1. Pin 220246ad. Read constant handling in the execution frontend and
   lowering, the refusal-explanation path in src/session/domain_session.rs,
   and the three rows in tests/fixtures/session_parity.json.
2. Bring a short note before editing: which constant forms legacy accepts
   and the domain refuses (probe, do not guess), how each will be admitted
   and materialized, what is left out, and why the async text is reached.
3. Fix the refusal explanation first; it is independent of admission.
4. Admit the agreed constant forms. Fix the type diagnostic.
5. Also add the assertion carried from ADR 0247: every stock registration
   the host inventory marks refused has a specific wait reason, and no
   enrolled function is in the reason table.
6. Regenerate the pinned table, run the required stages, preflight and the
   commit checker. Bring the packet with before and after totals.

Results are added here by the implementer.

## Implementation

The admitted cut is constant vectors, tuples, anonymous objects, strings,
nested combinations and references to those values, plus the existing scalar
arithmetic and string concatenation. The frontend builds immutable constant
nodes. Lowering emits existing collection/string/literal instructions at each
read, so mutable runtime values are fresh recursively. Neither executor loop,
the runtime arithmetic routines nor the fixed-route inventory changes.

Deferred forms each have a named admission refusal: indexing, blocks, if,
constant functions, option construction/calls and bytes. Successful legacy
async-wrapper compilation no longer determines the explanation. Async text
comes only from the four async Refusal kinds or structured compiler diagnostics
for an await or async block in a forbidden context. A test enumerates all
current Refusal kinds; source controls distinguish actual async syntax from
async words in strings and comments.

The evaluator bounds expanded nodes to 65,536 and nesting to 128 (the existing
constant-tree limits), and bounds cumulative string/key copying to 16 MiB.
References count as their expanded trees. Nested string/key copies are charged
again at aggregate boundaries; this deliberately bounds compilation work as
well as the final data size. Bound failures name the limit. The expansion test
uses four levels of sixteen-way references, remaining below the independent
frontend item-dependency limit while exceeding the constant-node limit.

The expected/found type diagnostic now matches legacy and preserves its input
location. Failed redeclarations leave existing bindings intact. The late ADR
0247 host-policy/reason-table assertion is included here, as planned.

## Development evidence

- Constant source/ownership controls pass, including independent nested reads,
  repeated references, deferred-form explanations and failed replacement.
- The core materialization control returns true and verifies zero live store
  entries after execution. Its exact Miri inventory is
  tools/column/constant_data_miri.json; Miri acceptance is still required.
- Root library tests pass: 281 passed, 3 ignored. All four ordinary Rune Clippy
  feature configurations pass. Ordinary final stages remain required.
- The measured parity pin changes Equal from 125 to 131, Wording from 21 to
  20, and Refused at admission from 1 to 6. The latter reflects six newly
  enumerated deferred forms, not a regression in previously admitted behavior.
  Both constant diagnostic defects disappear. Host, notebook and ledger
  sections are unchanged.

## Follow-up

Remedy text still names `rnx repl` when a notebook displays it. A later record
should select the remedy for the actual front end; this record preserves the
previously accepted wording.

The refusal coverage list is explicit. The production `refused_construct`
match is exhaustive and has no wildcard arm, so adding a Refusal variant fails
compilation until its explanation is assigned. Source review accepted this
coverage condition before signing.
