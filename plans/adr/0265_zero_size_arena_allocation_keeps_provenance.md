# ADR 0265: a zero-size arena allocation keeps strict provenance

Status: Proposed

Base: published f064fcb989a66aaba1b9cfbe0b31aaa02a3344bf (ADR 0264).

## Problem

The arena that holds the compiler's intermediate representation answers
a request for zero bytes with a pointer made from an integer: the
alignment cast to a pointer. Miri under strict provenance refuses that
cast. ADR 0264 met it by accident: two tests that happened to contain an
empty text literal failed before reaching the code they were written
for, and were changed to use a non-empty text.

So a program with an empty text literal cannot be compiled under the
Miri configuration every record uses, and no committed Miri inventory
has ever compiled one. Whatever else is reached only through an empty
literal has never been checked either.

## Goal

Compiling a program that contains empty literals passes Miri under
strict provenance, and a committed inventory keeps it so.

## Required behavior

- The zero-size answer of the arena is a well-aligned dangling pointer
  built without an integer-to-pointer cast. Nothing else about the arena
  changes: the same address value, no allocation, the same result type.
- Before the fix, a committed Miri case that compiles and runs an empty
  text literal fails with the provenance diagnostic; that failure is
  recorded. After the fix it passes.
- The inventory covers, one compile per test: an empty text literal, an
  empty vector, an empty object, an empty tuple, an empty byte string if
  the compiler accepts one, an empty block and a function with no
  statements; each in the legacy compile path and through the execution
  engine where the form is admitted, and an empty text in a constant.
- Sweep: every integer-to-pointer cast in the crates of this repository
  that the two engines can reach is listed with its file and line and
  one of: fixed here because it is the same zero-size pattern, or left
  with the reason it is sound under strict provenance or unreachable
  under Miri. A cast that is neither is brought back before signing.
- The two tests ADR 0264 changed to avoid the fault go back to an empty
  text, and pass.
- No behavior a program can observe changes. No inventoried executor or
  policy item is edited; if the sweep finds a cast inside one, it is
  reported and not fixed here.

## Not in this record

Changing Miri flags, replacing the arena, the toolchain, or the
allocator traits.

## Acceptance

The failing-before receipt and the passing inventory; the sweep table in
the results; Miri on the 0264 inventory again, since two of its tests
change; a final in required mode, with the fixed-route bands, since the
arena is on every compile path.
