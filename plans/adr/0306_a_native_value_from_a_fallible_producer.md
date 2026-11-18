# ADR 0306: A native value from a fallible producer

Status: Proposed

Base: published 8b634f2494b261f25477d3648d78d4098f5cb47f (fix after ADR 0305).

## Problem

Real adapter code makes native values from inputs, and the call can
fail. The sales notebook loads data with
`polars::read_csv(path, [(name, dtype), ...])?`, whose binding is
`fn read_csv(path: &str, schema: rune::Value) -> Result<DataFrame, String>`
(a raw dispatcher in `adapters/polars/src/lib.rs`). Candle's tensor
constructors have the same shape: they take arguments and return
`Result<Tensor, String>`.

The new engine cannot carry that shape. ADR 0305's checked opaque path
admits only a zero-argument producer of a bare owned value:
`HostSignature::valid` requires arity zero for `OpaqueOwned`
(`function/host.rs`), the host bridge imports that output only as a bare
native value (`execute/host_bridge.rs`), and no checked
`Result<native, String>` output exists. The dynamic importer for
`ResultDynamicString` rejects native leaves. An audit for a first CSV
slice hit this stop before any edit. So the next adapter step is a
narrow interface change, not another set of annotations. The user
authorized it, as a protected host-bridge change, on the same
stop-on-failure terms as the Bytes work.

## Decision

1. A checked opaque producer may take bounded owned inputs and return
   `Result<T, String>`, where `T` is an enrolled native type:
   - the inputs use the existing checked input contracts (scalars,
     strings and owned trees), bounded as those contracts already are;
   - `Ok` imports a uniquely owned native value with its type's charge;
   - `Err` is the script-visible `Err(String)`, the same value and text
     the old engine produces, so `?` behaves as it does today;
   - enrollment is still carried into the binding's sole registration
     before its signature is erased (0305's rule), with the typed bounds
     checked at compile time.
2. The first use is `polars::read_csv(path, schema)`, under its existing
   name. The compiler binds one checked arity per item, so the checked
   binding is the two-argument form, `(&str, OwnedTreeInput)`. The
   one-argument inferred form keeps the old route, and in a sticky
   execution session it gets a located pre-effect `Arity` refusal. The
   old engine keeps both arities and its exact diagnostics. The checked
   and legacy registrations share one builder, using the existing
   alternate-host install pattern. The witness reads it back with the already
   enrolled `width()`. The Polars error type that `height()` returns is
   a separate question, kept out of this record.
3. On the new route, `read_csv` is bounded: the file's bytes and the
   parse's work and transient allocation are charged or capped before
   and while reading, and a breach is the new engine's existing
   limit error, not an unbounded read. The old route is unchanged.
4. Everything else keeps its route and its typed pre-effect refusal.
   Sessions only; file runs, `rnx serve` and embedding stay on the old
   engine.

## Constraints

- The protected change is limited to the host bridge's cold output path
  (`host_output` and a new cold helper; `Executor::host_call` stays
  byte-identical in source) and the signature check, plus a narrow
  extension of the cold frontend proof. That extension adds a
  Result-of-native fact for an enrolled factory call that `?` projects to
  the native value. The wrapped `Result` stays out of containers,
  closures, host arguments and retained bindings; the unwrapped value may
  enter the checked delta.
- A cap breach reports through a narrow typed path in the cold output
  transport, at the same call site, as the engine's existing
  resource-limit error kind. It does not reuse `budget_exhausted`'s
  current generic-host projection, and no panic string is an ABI
  discriminator. No interpreter, opcode or Store layout
  change; any inventoried item changed gets an exact source-condition
  transition reviewed before its screen.
- An early fixed-route screen gates the record. A band failure stops for
  the user's ruling, with no tuning loop, as with the Bytes records.
- No new crate. Tabs.

## Controls

- In the real `rnx-polars repl --engine=execution`, against explicit
  legacy, with stderr equal:
  - `read_csv` of a pinned CSV fixture in one input, then `width()` on
    the retained frame in a later input;
  - a bad path, a bad schema and a parse error, each as `Err(String)`
    with legacy's text, and `?` propagating it;
  - reset clears the frame.
- Bounds: an over-limit file and an over-limit parse each give the
  limit error before unbounded work, with nothing retained.
- A tight `ProgramDomain` charge witness for the loaded frame, with
  release and live counts back to baseline.
- Rejected shapes: a borrowed or mutable input, an unbounded input, a
  raw handler, a non-enrolled `T`, and a non-`String` error type.
- An unenrolled method on the loaded frame refuses before effects. File
  routing for these names stays on the old route with one effect.
- The inventory admits exactly the new sites, each with a live witness.
- A mutation per rule fails its control.
- One complete final.
