# rnx 0050 evidence: a program is more than one file

Implemented on Linux on 2026-09-15, Rune 0.14.2, Rust 1.98.1. The plan is
`e6c56cd`; the before executable was built there with `cargo build --release
--locked`. Scripts, raw exports, toolchain, binary hashes and check logs are
preserved in rnx-bench commit `9d155a1`, under `probes/file-modules` and
`results/file-modules-0050`. Review notes are not part of either commit.

## Implementation

`program::Loader` follows the pinned candidate order and bounds actual reads
across the entry and modules. Its limit is 8,388,608 source bytes, plus one
byte solely for detecting overflow. It does not trust metadata. Exhaustion
is sticky and prevents further opens, including when the compiler continues
processing declarations. Invalid UTF-8 is refused and charged for bytes read.

The runner gives the entry its recorded path, retains sources through
execution, and resolves diagnostic source ids before selecting text. Compile
and runtime diagnostics retain the original formatting. Missing-method names
use only the faulting source. Field candidates include every loaded file and
remain checked against the returned value. Debug source prints each loaded
snapshot in load order. No other context or execution path gained a loader.

Two public-API details were resolved during implementation and recorded in
the plan. Rune's `ErrorKind` is private, so the missing-module text is matched
through `compile::Error::msg`, not the private `ModNotFound` variant. Rune's
source text accessor is private too, so rnx retains the bounded read beside
Rune's copy. This is a source-byte allowance, not a total-memory ceiling.

Source ids cannot be inferred from the order of successful loader calls:
Rune can load a duplicate declaration before rejecting its insertion.
`Loader::get` first resolves the actual id through `Sources`, then matches
its recorded path to the retained text. Repeated reads with different text
make attribution ambiguous and return no location. It never rereads the disk
for a diagnostic. A dedicated unit test covers a skipped duplicate insertion
and conflicting snapshots.

## Acceptance gates

| gate | evidence |
| --- | --- |
| Rune layout fidelity | `program::tests::every_layout_and_refusal_matches_the_pinned_loader` compares source paths and full compile messages/offsets against Rune on mixed, misplaced, missing and duplicate fixtures |
| anchoring and precedence | checked-in mixed fixture returns 21111; integration tests use entry-directory and different working directories, relative and absolute entry paths; the ignored `c.rn` returns a distinct value |
| nested diagnostics | `tests/modules.rs` checks exact file, line, column, source line and caret for compile and runtime errors in `a/b.rn`, plus three ordered debug-source headers |
| method origin | module-only candidate recovers its name; runner unit test proves a matching entry candidate exists, then withholds sources and requires the original hash; a foreign unit is also refused |
| returned declarations | imported struct and named enum variant retain their field names and existing renderer spacing |
| aggregate allowance | exact-fit mixed program runs; one byte less refuses the crossing file; further direct loads and queued compiler declarations cause no additional opens |
| changing size | an opened file reports one byte, is then appended to, and the 31-byte reader consumes exactly 32 bytes before refusing; a second read consumes nothing |
| real bound and UTF-8 | a module larger than 8 MiB is refused with its path and the real limit; invalid module bytes name the module |
| other entry points | eval, session before/after reset and worker refuse inline and external modules; config with a module warns and still admits the prompt |
| regressions | default suite 358 passed; test-support suite 399 passed; no failures |
| checks | formatting clean; notices current; release selfcheck passes; clippy matches inherited baseline |
| Windows | actual changed sources and unit tests type-check in the isolated probe; whole-root build stops at ring's missing MSVC `lib.exe`; no execution claimed |

The final focused loader run additionally checks queued declarations after
early exhaustion; it passes all four loader tests. The existing runner
entry-file diagnostic tests were not edited. The worker fixture adds module
refusals to its normal protocol/lifecycle gate. The config's no-op loader
remains explicit; with a memory source Rune refuses earlier for having no
associated URL, which is the observed warning in this gate.

The twelve single-file comparison cases preserve complete stdout, stderr and
exit status: bare value, arguments, compile error, runtime error, missing
method, returned error, struct, unit, streams, budget exhaustion, debug source
and debug source on a compile failure. Version, eval, bare run and the JSON
workload add four successful equivalence checks before timing is permitted.

Root clippy returns nonzero at both revisions on the existing denied
`non_octal_unix_permissions` findings in `tests/config.rs` and `tests/fs.rs`.
The multiset of all 35 code-bearing diagnostic levels, codes and messages
is identical. No suppression or unrelated repair was added. Native Windows
build failure and the narrower signature-stub probe are kept separate in
both the logs and claims.

## Cost

Pinned core 4, hyperfine 1.20.0, no shell, 10 warmups and 100 runs. Both
executables use the same locked release build conditions. The final release
hash matches the one in the timing export's conditions. Measurements are
sequential and include drift; none of the lower means is a speedup claim.

| whole process | before mean | after mean |
| --- | ---: | ---: |
| version | 0.553 ms | 0.543 ms |
| eval 42 | 4.081 ms | 4.028 ms |
| bare run | 3.733 ms | 3.672 ms |
| 10k JSON | 11.843 ms | 11.550 ms |

Binary: 15,095,392 to 15,111,848 bytes, an increase of 16,456 bytes.
No dependencies, features, notices or Jupyter package code changed. Raw
command-output whitespace is preserved in the bench logs.
