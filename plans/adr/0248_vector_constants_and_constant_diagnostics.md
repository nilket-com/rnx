# ADR 0248: vector constants and constant diagnostics in a domain session

Status: Proposed

Base: published 220246ad7b0f0978ad1a2e23a9bfa42642b814ed (ADR 0247).

## Problem

The pinned classification records three faults around constants in a domain
session:

1. `const T = [1, 2]; T` works in the legacy session and is refused on the
   domain. It is the only source-form refusal in the corpus.
2. That refusal says "async input is not available in an execution session".
   Nothing in the input is asynchronous. The session reaches that text
   because it explains a refusal by trying the legacy async wrapper.
3. Redeclaring `const A = "x"` after `const A = 1` with a dependent
   `const C = A + 1` is rejected on both engines, but the domain says
   "restricted scalar constant" where legacy says "Expected type
   `std::string::String` but found `std::i64`". The row is marked
   domain_worse.

## Goal

A constant the legacy session accepts is accepted on the domain with the same
value, or refused with a diagnostic that names the real cause. No refusal is
ever attributed to async unless the input contains an asynchronous construct.

## Required behavior

- Constant initializers the legacy session evaluates (at least vectors,
  tuples, objects and strings of constants, nested, and arithmetic over
  other constants) are admitted on the domain, or the boundary note states
  exactly which are admitted and why the rest are not. A constant is
  immutable data: say how a use of it is materialized (fresh value per use
  or shared) and show that it matches what legacy observably does, including
  after a mutation through a binding initialized from the constant.
- The async explanation is produced only when the refused construct is
  asynchronous: async block or function, await, select. Any other refusal
  reports its own construct, input, line and column. This is checked for
  every refusal reason a session input can reach today, not only constants.
- The constant type error carries the expected and found types, at least as
  much as legacy.
- Failed redeclaration stays transactional, as pinned today.

## Out of scope

- Host enrollment, dynamic host inputs, bytes, async, native values.
- Constant evaluation changes in the legacy session or in file runs, except
  where the same frontend code serves both and the result is identical.

## Acceptance

- session_parity.json: the vector constant row becomes Equal; Refused at
  admission falls from 1 to 0, or to the count of forms the note leaves out,
  each with a true reason. The domain_worse count falls from 1 to 0. The
  "diagnostics to improve" group is empty or lists only what the note
  deferred.
- New transcript rows: each admitted constant form; mutation through a
  binding does not change the constant; redeclaration; one row per
  non-async refusal reason proving the text does not mention async.
- The fixed-route source condition passes; bands pass without exception. If
  the change needs the fixed operation loop or its inventory, stop and bring
  it to review first.
- Required stages follow the changed paths.
