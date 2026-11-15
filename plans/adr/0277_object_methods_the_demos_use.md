# ADR 0277: the object methods the project's own scripts use

Status: Accepted

Base: published 09c47eeb95bc7ec5bc559983a08ccd4f4dd1ec58 (ADR 0276).

## Problem

The removal inventory of 2026-11-14 shows that 87 of the 92 census
inputs the old engine accepts are accepted by the new one. Two of the
remaining five are `journey.rn`, the project's own walkthrough, as a
file and as a wrapped input. Since ADR 0275 enrolled `process::run`
they are refused on the next thing they meet, a method call on an
object:

	event.get("id")
	cell.get("name")

So the script the project shows first still runs on the old engine.
The census names the first refusal only; what else in that script and
in the other demo scripts would be refused after it is not known.

## Decision

The new engine accepts the methods on objects, and on the other
built-in collection and text types, that `journey.rn` and the
project's demo scripts call and that are refused today, with the
semantics the old engine gives them.

The implementer first measures the set: every method call in those
scripts that the new engine refuses, found by compiling each script
past each refusal in turn, not by reading for spellings. For each:
the receiver type, the legacy semantics including the absent-key and
wrong-type cases, what is returned (a copy, a shared handle, an
Option), and how the new engine can express it. The record then
closes that measured set, or a stated cut of it if one member needs
something larger.

## Constraints

- No inventoried executor item or policy file changes. A provider
  may be added as a cold helper beside the existing object methods
  and reached through the existing method-call route, if that changes
  no inventoried item token for token. If it cannot, send that and
  stop.
- A receiver proved at compile time keeps today's direct route. A
  receiver not proved at compile time may be dispatched on its
  run-time type only when the compiler shows the dispatch is total
  for this compilation: every type a value can have in this engine
  either has the method certified here with legacy semantics, or has
  no such method in legacy and gets legacy's run-time error. A user
  impl or a registered host method of the same name that is not
  certified defeats totality, and the named compile-time refusal
  stays. The engine never refuses at run time and never falls back
  after it starts.
- The measured set is one name, `get`. Its providers in legacy are
  Object, Vec, Tuple (including unit) and String, each with its own
  argument forms and results. All four are certified here, each with
  its measured semantics, or the record stops at the providers and
  leaves the dispatch for the next one.
- Sharing and moves are as measured from legacy: reading an element
  does not copy a heap value unless legacy does, and a moved receiver
  or element is the same error.
- Ordinary lowering frames do not grow, measured beside the base.
- Parity rows are observed and classified by measurement. Census rows
  change only by observation with the bound executable.

## Controls

- Each admitted method: present, absent, wrong argument type, moved
  receiver, shared element mutated through the result where legacy
  shares, on a literal and on a value from a host call.
- `journey.rn` runs on the new engine with no fallback and the same
  output as the old one, file and wrapped; the census rows 0007 and
  0072 are re-observed and move to executed comparison if they admit.
- Every other demo script: the first refusal it now meets, recorded.
- Each provider on a proved receiver beside legacy: every argument
  form, bounds and text-boundary cases that return None, the error
  cases, sharing of returned elements, independence of a returned
  substring, cost independent of the receiver's size where legacy's
  is.
- Each representable receiver kind through an unproved path
  (parameter, parsed value, field, element) beside legacy, including
  the kinds with no such method; receiver and argument evaluated once
  each, in order, also when the call ends in the error.
- A program with its own impl of the name, and a context with a host
  method of the name, keep the compile-time refusal before effects.
- Miri over any new ownership path at small sizes.

## Out of scope

Adapter methods (Polars, Candle, Postgres), iterator pipelines beyond
what the measured set contains, and bytes.
