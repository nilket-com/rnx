# Implementation plan 0258: Option constants and constant field access in an execution session

Status: Implemented

Paired ADR: ../adr/0258_option_constants_and_constant_field_access.md.

1. Pin dffb48c1. Read the call and field-access arms of the bounded
   evaluator, how a resolved constructor is identified in HIR, the Option
   form of the constant value representation, the constant lowering and
   the variant instruction, and the fixed-route inventory.
2. Before editing, send a short note: how the two constructors are
   identified, the validation in lowering, the measured legacy
   diagnostics and locations for the field forms, and the Miri tests. If
   any inventoried item would change, send that instead and stop.
3. Implement the constructors, the materialization and the two field
   forms.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, the required stages, preflight and the commit checker. Bring
   the packet with the class of each new row, the totals before and
   after, the selected stages and the per-stage durations. The candidate
   is frozen once the packet is sent.

Results are added here by the implementer.

## Implementation and focused evidence

Resolved canonical constructor hashes and arity admit Some and None without
calling a function. Ok and Err remain a named Result refusal. Payloads
share the evaluator budget; aggregate copying checks byte capacity before
allocation. Lowering validates Option family, variant and field count
before allocating slots or emitting the payload, then uses the existing
variant instruction. No runtime instruction, store schema, inventoried
item, manifest or policy changes.

Numbered fields admit tuple, vector and Option payloads; named fields read
objects. Selection uses logical children and the charged subtree copy.
The 13-case runtime comparison agreed on values, error messages and
primary locations. Full error text retains the existing execution-session
mapped-frame annotation; no full-stderr identity is claimed.

Development controls passed: 41 core constant tests, 21 session-parity
tests and all-feature Rune lib/test clippy. Controls cover constructor
aliases, shadowing, arity, nested containers, mutation of one materialized
use, release, malformed Option shapes, shared limits, field diagnostics
and failed admission preserving earlier bindings. The positive session
control explicitly checks Some(1) in both engines, independently of the
classification table. Constructor hashes remain runtime computations:
Hash::type_hash is not const in rune-core/src/hash.rs.

Only the existing Some row changes (to Equal), and three new measured
rows are Equal: None, tuple field and object field. Equal increases from
176 to 180, admission refusals fall from 3 to 2. Other classes and the two
remaining refusal reasons are unchanged. The generated table retains
one row per line and its existing size cap.

The committed constant_option_miri.json inventory covers nested independent
materialization and release, failure cleanup, shape validation and bounded
copying. Miri and ordinary final receipts, including selected stages and
durations, are reported in the publication packet after the signed run.
