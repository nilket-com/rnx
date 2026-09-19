# 0067 gate 1: command boundary, coordinates and source documents

Status: ready for review. Production sources, manifests, lockfiles and notices
are unchanged from the accepted plan `9570b25`. The code is isolated in the bench.
This does not open the remaining implementation gates.

The [probe and replay instructions](https://github.com/nilket-com/rnx-bench/tree/859ecaa53bcb9424c187865a29b4196f6bc03ca3/probes/one-command-boundary)
and [retained results](https://github.com/nilket-com/rnx-bench/tree/859ecaa53bcb9424c187865a29b4196f6bc03ca3/results/one-command-boundary-0067)
include source bundles against published `94f5f3f`, the exact prototype diff,
fixed wire vectors and command outcomes. Fixture origins are explicitly local;
the published-origin journey remains gate 4 after review permits publication.

## Crate boundary

Actual `cargo install --git` builds the root plus internal management library
and installs only `rnx`. `rnx project adapters` dispatches into that library.
The library has no dependency on rnx, so there is no cycle. The independent
workspace needs an explicit root workspace exclusion, established by Cargo's
initial refusal and the subsequent successful installation.

A real defaults-off runner-only consumer has no management symbols and no
`rnx-project`, `blake3`, `toml` or `sha2` in its graph. Stock contains the dispatch
symbol as a positive control. A forced consumer rebuild succeeds with Git trapped
and networking disabled. Both adapter graphs and the server exclude management.
A real lock produces the combined wrapper with defaults disabled and explicit
allocation accounting/project sources; Cargo metadata confirms no management
feature unification. That combined wrapper is not compiled in this gate.

There is no embedded payload. The archived stock/consumer sizes are not a matched
performance comparison because their Cargo profile settings differ. Gate 5 owns
stock startup, size, notices and default-graph measurements. The fixture lock
was regenerated offline and has incidental compatible updates; the product port
should preserve unrelated pins while adding management dependencies.

## Coordinates

Fourteen real-build classifications pass: three Git installation observations
(fresh, second revision and cached first revision in one Cargo cache), clean
pushed path, dirty bytes hidden by a clean filter, staged addition/deletion,
mode-only change, restored clean, clean unpushed, unrelated outer repository,
absent administration, absent Git and removed acquisition evidence.

Local acquired evidence binds the canonical Cargo checkout to its Cargo Git DB,
the configured URL in FETCH_HEAD, and a fetched object's ancestry containing the
exact revision. It is trusted local evidence, not future reachability or protection
against forged Cargo administration. Missing/unrecognized evidence yields an
unverified notice. Neither clean state blocks consent. The pushed path build
acquires without an override; the unpushed one fails after consent with Cargo's
missing-revspec diagnostic retained.

Every description/decline and dirty/unknown refusal runs with a positive-controlled
Cargo trap, network tracing and no network namespace access, creating no request
directory. A separate real stock path build has zero Internet socket calls.
The gate-only decision driver is not the REPL bridge; session preservation,
quoted recovery for arbitrary paths, missing diagnostic hints and cancellation
remain gate 2 obligations.

Cargo's usual mtime rebuild tracking cannot detect chmod alone. The stock build
script therefore registers a missing private output to rescan on each stock
build, writing generated constants only when changed. This is build-time work,
not startup work. Defaults-off consumers return before any discovery.

## Fixed documents and compatibility

| Envelope | New | Original path reader |
|---|---:|---:|
| Declaration | 2 | 1 |
| Lock | 4 | 3 |
| Receipt | 5 | 4 |
| Identity / generator | 3 / 3 | 2 / 2 |
| Ready | 3 | 2 |

Fifteen fixed vectors, 42 refusal cases and 11 retained-context key changes pass.
New declarations select path OR Git URL/full lowercase revision. Pure Rune
sources remain paths. Git package associations retain name, package ID, URL,
revision, canonical checkout and repository-relative manifest. Only their tree
fingerprints are replaced. Context, wrapper/main, Cargo lock, path natives and
genuinely external inputs retain their identity roles. Mixed sources are pinned.
Git-owned inputs cannot leak into the external content audit.

Canonical JSON uses declaration-field order, ordered collections and no trailing
newline; BLAKE3 of those bytes remains the identity key. Unknown and duplicate
fields, invalid versions, malformed coordinates, ownership overlap and broken
bindings refuse. Existing declaration, lock, receipt and identity modules remain
byte-identical; the ready adapter calls the existing decode helper. The new
readers validate shape/binding, not live source truth. Relative declarations are
resolved by the later workflow. No new format is published by a product command.

## Validation and qualifications

Both isolated management copies pass formatting, strict clippy across all targets
and their suites in ordinary and test-support configurations. The integrated
copy reports 49 / 50 tests passed and two ignored; the schema copy 48 / 49 passed
and two ignored. Fixed-vector and external drivers supply the additional checks.
Root formatting passes. Root clippy reports existing warnings in unchanged
library code; the new entry/build script has no diagnostics. No root strict-clippy
pass is claimed, and this record does not clean up unrelated runner code.

The journal preserves scaffolding corrections: workspace exclusion, Cargo's
“revspec” wording, a probe helper's placement and an unused copied helper. A
collapsed equivalent build-script condition is archived as a separate fixture
revision and checked by another real acquired Git install, with management output
and eval equal to the earlier fixture. The original measured sources remain
recoverable in their bundles. No substantive behavior is inferred from that
formatting change.

Gate 2 ports management and the actual session protocol. Gate 3 owns Git/path
workflow integration, raw-object verification, compatibility and publication.
