# 0062 gate 1: authoring before manifest mutation

Status: ready for Linux review. The plan is accepted at `969323c`; gates 2–6
remain open. The authoring prototype passes without a manifest-format change,
general Cargo resolver, or rewriting unrelated user text. No product source,
manifest, dependency, notice, adapter, kernel or server changed.

## Source and boundary

`rnx-bench/probes/adapter-authoring/` contains the standalone authoring source,
isolated build and fixture driver. `results/adapter-authoring-0062/` records all
inputs, candidate/control TOML, probe output, generated wrappers, Cargo locks,
canonical identities, binary hashes and outcomes. Product source is the accepted
plan revision `969323c`, whose implementation is unchanged from 0061.

The isolated build copies the real project tool and its lockfile. Its extra
binary imports the actual bounded input reader, manifest parser, generator and
supporting modules. Every imported Rust source is hash-recorded and asserted
byte-identical after formatting. The dependency lockfile is byte-identical too.
Formatting, offline release build and strict prototype Clippy pass.

The prototype takes a manifest and catalogue names, reads and structurally
validates the runtime and adapter Cargo files, and computes additions. It appends
tables with the existing TOML serializer, parses the entire candidate, and checks
that its semantic difference is exactly those additions. It prints candidate
text, declarations and both wrapper forms. It never writes a manifest, invokes
Cargo, compiles a package, or runs a builder. The fixture asserts original
manifest bytes, inode and mtime are unchanged after each call.

The fixture then explicitly materializes candidate/control projects and invokes
the **real product `lock --offline`** on each. This is where real Cargo metadata,
bounded native fingerprints and 0061 identity construction enter the proof;
there is no Python reconstruction of the assembly-key schema. Lock creates no
compiled entry and neither project receives a receipt. Compilation and live
registration are not claimed from metadata resolution.

## Fifteen controls

| Case | Observation |
| --- | --- |
| Relative runtime, both names requested in reverse order | Relative paths retained; additions sorted polars then postgres. |
| Absolute runtime, both names | Absolute form retained, not silently made relative. |
| Alternate relative spelling | Dot/trailing-separator spelling resolves correctly; canonical wrapper agrees with control. |
| No final newline | The original prefix is intact and appended tables parse. |
| Comments and unsorted existing tables | Existing z/a order and comments remain; postgres is appended after them. |
| Already-present declaration | A canonically equivalent path spelling is a full byte-preserving no-op. |
| Escaped path | Quote, backslash and Unicode survive TOML parsing/serialization and Cargo resolution. |
| Relocated relative layout | Identical manifest text works at the moved location; authored/manual keys agree there, but differ from the original location's key. |
| Symlink followed by parent component | Resolves through the link target before `..`; lexical simplification would have selected the wrong root. |
| Inline child table | Existing `native.postgres` expressed inline allows a new polars sibling. |
| Closed inline native table | `native = {}` refuses append, naming the incompatible layout; original bytes remain intact. |
| Dotted-key sibling | Existing dotted postgres fields permit a polars sibling; the parser accepts it without semantic changes. |
| Shipped Polars | Real package/path/plain-hook declaration and resolved Cargo graph. |
| Shipped PostgreSQL | Real package/path/lifecycle-hook declaration and resolved Cargo graph. |
| Both shipped adapters | Both appear in the real generated wrapper and resolved graph; this combined artifact is not compiled or executed in gate 1. |

Each of the **fourteen positive cases** compares exact candidate bytes with an
independently hand-written TOML control, native declaration equality, legacy
wrapper equality at the same manifest base, and canonical shared-wrapper
equality across the two fixture projects. Both real product locks contain
byte-identical canonical identity documents and equal assembly keys. The negative
case refuses rather than reformatting the file or changing the rule.

The relocation case copies the native layout and reuses exactly the same relative
declaration bytes. It deliberately does not claim old lock portability: the new
canonical native location changes identity, as 0061 requires. The symlink case
likewise proves filesystem resolution rather than inferring it from a lexical
path transformation. Neither case needs a new source-search rule.

TOML layout is not a blanket yes/no based on the presence of dotted keys. The
closed inline ancestor refuses with the parser's “cannot extend value of type
inline table with a dotted key” diagnostic. A legal dotted sibling succeeds.
Thus parsing and semantic comparison of the complete candidate remains the
correct admission rule in the plan; no special reformatter is needed.

## Scope and fixture corrections

Most path/layout cases use a tiny Git-tracked **metadata-only** runtime and
adapter tree. Its Rust files do not pretend to implement Rune or builders and
are never compiled. Three separate cases use the actual shipped packages.
All candidate projects and the private cache are temporary and removed at exit;
no user cache, manifest or running session is touched.

Early scaffolding failures are retained and labelled. The isolated binary first
omitted two required imported-module declarations. An unsorted-table fixture
then named the same Cargo package under two aliases, which Cargo correctly
refused; it now uses distinct custom metadata packages. The escaped-path manual
control initially assumed basic-string output while the TOML serializer chose a
valid literal string; the hand-written control now uses that spelling. Final
controls still demand exact bytes and identity equality. No product change or
gate relaxation followed from these fixture errors.

Gate 1 passes and returns for review before adding `adapters`/`add` to the
product CLI. Full refusals and helper traps, advisory-lock/atomic publication,
editor races/signals, actual session/query journeys, and regression are gates
2–6. No startup timing, Windows execution, live registration or publication
guarantee is inferred from this authoring-only prototype.
