# ADR 0268: the cost of a session input does not depend on live data

Status: Proposed

Base: published 2b1b1c43b0f41b724e3db22387debc54d8ef25d1 (ADR 0267).

## Problem

After every input, an execution session asks its program domain which
compiled programs are still needed. The domain answers by walking every
value reachable from every binding and looking for function values and
closures. The walk visits each element of each vector.

ADR 0267 measured the consequence while attributing its own costs: with
one binding holding a vector of 5,000,000 integers, 100 inputs that each
evaluate a scalar took about 57 seconds on the published commit, in an
unoptimized build. The inputs never touch the vector. The walk also
keeps a visited set that grows with the number of heap values.

A session exists to hold data between inputs. A cost per input that
grows with the data held makes the engine unusable for exactly the
sessions it is for. The note measures the same sessions on the legacy
engine for comparison.

## Goal

The work and the memory an input needs beyond its own evaluation depend
on what the input does, on the number of bindings and on the number of
retained programs. They do not depend on the number of live values.

This record reaches that goal for a bounded part of the problem and
says where it stops. A walk of live data is allowed only while some
program that could have put its own code into a value is neither
leased nor yet found, and the walk ends as soon as every such program
is found. So a session whose programs cannot produce a function value
of their own never walks, a live closure is found once per collection
and the walk stops there, and a full walk happens only to prove such a
program dead, after which it is released and not looked for again.
What remains is that the walk may pass through unrelated data before
it finds a live program. Removing that needs reachability metadata in
the store and is a separate record.

## Required behavior

- A program is never released while a function value, a closure, a
  suspended frame or a lease can still reach it. This is the safety
  property and nothing in this record may weaken it. Every existing
  collection control keeps passing unchanged.
- A program that nothing can reach is still released without waiting
  for the session to close. If release is no longer immediate, the
  ADR results state the bound on how long and how many such programs
  can wait, and a control shows the bound holding over a long session.
  A session that rebinds the same name with a new closure 10,000 times
  retains a bounded number of programs.
- Source maps kept per revision follow the programs, as they do today.
- Function values are plain bits that can be copied into any container,
  captured, returned from a host callback, and overwritten in place.
  The note lists every way a function value or closure enters, moves
  within and leaves the store, and shows the design is right for each.
- A store that never holds a function value or closure in data pays
  nothing in its ordinary operations. The fixed-route bands show it,
  with no exception and no policy edit.
- The cost that remains per input is measured and written down for
  three sessions: scalars only; one 5,000,000-element vector of
  integers; the same size holding function values. The second must be
  within noise of the first. The third is stated honestly, whatever it
  is. Two more are measured: the integer vector beside a binding that
  holds a live closure, in both binding orders, and the input after
  that closure's last reference is dropped.
- Whether a program can put its own code into a value is decided once
  when the program is compiled, from all of its functions. Anything
  not proved is treated as able to.
- A failed or cancelled input, and an allocation failure during
  collection, leave the session usable with its earlier bindings and
  release nothing that is still reachable.
- No inventoried executor or policy item is edited; if the design
  needs one, the note says so and work stops there.

## Not in this record

Reclaiming data values, which reference counting already does. The
cost of compiling an input. The per-input walk of the bindings
themselves, which is bounded by their number. Changing when the legacy
session releases anything.
