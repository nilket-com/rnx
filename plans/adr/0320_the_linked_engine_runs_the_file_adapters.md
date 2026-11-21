# ADR 0320: The linked engine runs the file adapters

Status: Proposed

Base: published d7c042620a4fea3d9912cb23183fa02973ff9ca7 (ADR 0319).

## Problem

ADR 0319 gave the separately linked engine handles and one synthetic
provider: one create, read and release table, reading an `i64`. Real
adapters take structured inputs, return fallible and structured
results, and have several payload types. The synchronous file adapter
surface from ADRs 0305 to 0315 still reaches the engine only through
stock linking. Until it runs through the engine image's callbacks, adding
an adapter still changes the engine.

This is record 4 of the six isolation records. Packaging is record 5.
Record 6 changes the installed default.

## Decision

1. Registration.
   - A versioned registration surface sits alongside v2. It reuses the
     sealed context, invocation and release machinery from 0319.
   - The v2 table is not extended in place, because that would make a v2
     caller read past its own table.
2. Ownership.
   - One adapter-side owner covers the linked Polars, interchange and
     Candle code. It owns the concrete `DataFrame`, `Dense`, `Tensor`,
     `Mlp` and any enrolled typed error payloads.
   - So Polars can produce a `Dense` that Candle borrows without copying
     its numeric buffer, and without interpreting a foreign Rust
     pointer.
   - The engine owns script values and checked admission. It sees only
     validated provider, kind and payload tokens.
   - Operations and charging use the existing implementations. Neither
     CSV or model loading nor the unary operation list is duplicated.
3. Inventory and descriptors.
   - An inventory is frozen from the actual checked-file installer:
     name, receiver, argument and result shape, feature and policy.
   - Descriptors for those shapes cross the versioned interface.
     Missing, conflicting or incompatible declarations are rejected
     before any effect.
   - Family names and implementations stay on the adapter side,
     including 0315's single declaration of 19 unary operations.
   - The file, session and panic-policy distinctions are preserved. A
     session-only registration gets a refusal witness here. For example,
     `DataFrame.height()?` is refused with `OpaqueOperation` on a checked
     file, as 0312 showed.
   - The inventory is taken from actual checked-file admission. The 0314
     score reader stays test-only.
   - A context owns its admitted names, and binds each one to an
     operation ID. Opening and closing a context leaks no names.
4. Mapping.
   - Before production edits, each shape is mapped to concrete C fields
     and ownership:
     - path and schema;
     - column names and dtype;
     - shared operands;
     - typed `Result`, scalar and tuple outputs;
     - bounded diagnostics;
     - existing observable output.
   - Variable-size callback output is transferred exactly once, even when
     the caller's storage is too short.
   - A borrowed span lasts one synchronous callback.
   - A successfully transferred payload is placed under a provisional drop
     guard before its kind, peak credit or store insertion is checked.
   - Alias charging and exactly-once destruction are preserved. Memory is
     allocated and freed on the same side.
   - The published CSV path reader and the ordinary file allowance stay
     as they are. The `Result`, execution-error and pre-effect refusal
     channels stay distinct.
5. Not in this record:
   - packaging (record 5);
   - the default change and the cutover (record 6).
   No part of the cutover setup allowance is spent here. That allowance
   was accepted by the user on 2026-11-21 under 0317's bounded rule.

## Constraints

- Target: x86_64-unknown-linux-gnu.
- No new third-party crate.
- ABI v1 and v2 callers keep working unchanged.
- None of the 40 inventoried items move.
- The adapters' registration and ownership code lives with the adapters,
  not in root `src`. The stock host inventory and cross stay unchanged.
- `final.sh --required` passes on the signed candidate before the
  complete final. A failure keeps its evidence and stops the sequence.
  Tabs.

## Controls

1. Isolation.
   - Both runners and the loaded engine image are bound by hash.
   - A real adapter callback changes an observable result between two
     provider builds, with the engine image byte-identical.
   - A family gains a member with no engine change. Using one build of
     the image, 0315's fixture-only twentieth unary member:
     - is unavailable with the ordinary provider;
     - is registered by the extended provider through the same family
       declaration;
     - executes, with a checked numeric witness.
     This rules out an engine-side list of names or trampolines. No
     production API grows, and no twentieth production member is added.
   - The engine image has no dependency on an adapter library.
   - An admitted script never enters the stock VM.
   - The new boundary is screened before broad integration.
2. Parity. The published file-admission matrix is replayed on stock
   checked files and on the sibling. It covers:
   - the CSV over 2 GiB;
   - an explicit schema, and invalid inputs;
   - a `Dense` crossing from Polars to Candle;
   - rank and count;
   - model load and dims;
   - all 64 forward scores;
   - all 19 unary methods;
   - the full demo's first unsupported call.
   The comparison covers script output, error channel, location and
   effects. Real output is inspected, not just callback counts.
3. Cap and cleanup paths, under a finite-credit fixture:
   - a missing provider;
   - a wrong token kind or owner;
   - aliases;
   - failure after provisional creation.
4. Measurement, with two separate comparisons.
   - The gating sibling comparison is frozen before counts. It uses two
     equally adapter-linked launchers with the same engine image,
     recipe, environment and path layout, differing only in the declared
     provider variation.
   - Across these siblings, the ordinary per-change bands must pass for
     native setup and for execution, on W and T, at both level and
     growth. The ordinary native-off checks stay.
   - Both ELFs are attributed by the published gate.
   - The stock-to-image comparison is diagnostic only here. The cutover
     allowance is spent only in record 6.
   - The complete final's stock gate applies as usual.
5. Finals. A signed `--required` run, then one complete final.
