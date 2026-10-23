# rnx 0191: thin LTO for the release build

The user decided on 2026-10-23: "thin lto it is". Rune's shipped-profile study
(P10) measured the fresh-core harness with `lto = "thin"` against Cargo's default
release profile (16 codegen units, no cross-crate LTO):

- all ten canonical execution cells use fewer instructions (core -0.13% to
  -1.37%), and so do all four controls;
- startup (K1) uses about 3.4% fewer instructions;
- a clean root build takes about 5% longer (nano 52.2 s -> 55.0 s; strix
  37.5 s -> 39.3 s);
- rnx demo and AWFY wall times are unchanged within noise.

These are harness-scope instruction counts, not a measured end-to-end CLI
speedup. Thin LTO does not stabilize codegen placement; the Rune milestone
exception process stays as it is.

## Change

Add `[profile.release] lto = "thin"` to the root manifest. That covers the
binary `cargo install ... rnx` builds. The adapter assemblies that `:dep`
generates are a separate follow-up, because their generated manifest feeds the
assembly cache key.

## Validation

A release build with the committed lock, a check that rustc receives
`-C lto=thin`, the three root demos, and the root suite.
