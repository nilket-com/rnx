# Record 0057 gate 2: manifests and the explicit source handoff

2026-09-16. Gate 1 is accepted and pushed. Gate 2 is submitted for review;
gates 3–6 remain open. Reproduction and raw results are in rnx-bench
`probes/package-inputs/` and `results/package-inputs-0057/`.

## Product boundary

The opt-in project-sources feature now exposes the early CLI command
`project-source-version`, replying with `{"format":1}` before context creation,
extension installation or configuration reads. Extra arguments refuse. The
feature adds `run --source-map MAP ENTRY`, parsed only before the entry path,
composable with the existing budget/debug flags. After the path the spelling
is just a script argument. Repeated flags or missing values refuse.

The source map is a regular file read through the existing handle-kind check
and Unix nonblocking open. Its read is capped at 16 MiB plus one detection byte.
Typed decoding rejects duplicate or unknown fields, unsupported versions,
non-absolute roots/entry, an entry different from the run's, invalid prefixes
and duplicate mounts. Deserialization caps mount/prefix arrays before adding
the crossing element to their vectors. The validated map constructs the same
bounded Loader accepted in gate 1. Runner::run delegates to a shared run_loaded;
no VM, budget, return or diagnostic policy was copied or changed.

Only that run branch supplies a map. Eval/session/worker/config/server context
construction is untouched. The map is explicit filesystem selection, not a
verified package lock or an ambient environment input.

Root Serde is optional behind project-sources. Its packages already existed in
the lock; the only root Cargo.lock edit adds the local rnx dependency edge.
The default normal/build dependency tree **and dependency feature sets** compare
byte-identically with c8fb102 after normalizing the checkout path. Stock rnx gains
no TOML, Cargo orchestration or Tokio-process dependency. Root notices remain
current without edits. The kernel, adapter and server package are unchanged.

## Independent tool inputs

The private tool core now parses strict TOML manifests into application/source
forms, validates the runtime/override distinction, aliases, native registration
names and builder paths, and drives the accepted graph core from real files.
Reads are capped at 1 MiB and use a regular-file handle check (Unix FIFO refusal
is tested). Duplicate keys, unknown fields, conflicting forms, invalid paths,
source/native/battery collisions and unsupported hooks refuse. Each native or
source declaration table has an explicit 256-entry cap; graph bounds remain
64 distinct manifests, depth 16 and 256 expanded mounts. Dependency manifests
must be source packages. The application is not re-read during expansion.

Wrapper generation sorts native names, assigns stable Cargo aliases and emits
plain/lifecycle calls with the explicit project-sources feature on rnx. Cargo
paths are serialized as TOML data; builder text is restricted to ordinary ASCII
Rust identifier paths, never expressions. Equivalent reordered declarations
produce identical Cargo.toml/main.rs. Tests parse the generated TOML and pin
both call forms. Building a project through this generated workflow remains a
later gate; this checkpoint does not claim a generated native project was built.

JSON lock documents and maps have typed, versioned fields, semantic shape checks,
a 16 MiB input cap and a capped output writer (including escaping). Inventories
validate relative paths, hash spelling, duplicate entries and their entry/byte
bounds. Lock documents distinguish generated identities from executable hashes.
The normalized declaration maps also reject duplicate keys during JSON decoding.
These are schema checks, **not content verification**. There is no fingerprint,
ancestor Cargo audit, receipt, atomic write workflow or product lock/build/run
command yet. Internal unused-code allowances explicitly mark that staging.

The private executable-capability helper owns one direct child, null stdin and
concurrent bounded stdout/stderr readers. It accepts only the version-one reply
with successful exit and empty stderr. Unsupported/malformed replies, duplicate
fields, 4096-byte overflow or a one-second timeout refuse naming the executable;
on refusal it kills/reaps the direct child before returning. This does not claim
containment of arbitrary descendants or a bound on an OS spawn stall.

The tool has a separate lockfile, resolved all-platform graph and notices:
93 registry packages, 53 distinct license texts, six unavailable texts listed
explicitly. Notice rendering normalizes line endings, while provenance hashes
refer to original bytes. notices.py --check passes. Its public dependency on
pinned Rune is used for identifier validation, not a private rnx API.

## Executed checks

| Check | Result |
| --- | --- |
| Default root suite | 375 passed |
| Test-support root suite | 418 passed |
| Project-sources + server-runtime + test-support | 437 passed |
| Tool ordinary tests | 14 passed; one explicit integration test ignored |
| Explicit manifest-to-runner integration | 1 passed, separately invoked |
| Root project CLI tests | 3 passed |
| Tool clippy, warnings denied | passed |
| Root and tool formatting | passed |
| Root and tool notices | current |
| Tool Windows build | type-checked only |
| Root clippy | inherited status 101; diagnostic headlines match gate 1 |

Full suites ran serially with one test thread under TERM=xterm. Afterwards clippy
identified a literal-format warning in the new capability println; its equivalent
escaped-literal spelling removed that warning. The three CLI tests, assembled
capability fixture, explicit integration and clippy were rerun after that one-line
cleanup. The full-suite lib.rs hash is retained separately; no broad-suite rerun
or new performance claim is implied. Root clippy still has the same two denied
non-octal-permissions lints and existing warnings, with none from the new code.

The CLI probes observe real transitive code, source-accurate runtime/compile
errors and caret, debug-source headers, unchanged script arguments, refused map
inputs before a valid script can print, Unix FIFO refusal, and continuing
module refusal in eval/session. The typed decoder exercises exact byte and array
limits. The tool tests additionally exercise strict lock/manifest round-trips,
conflicts, code-injection spellings, inventory limits and capability child reaping.

The explicitly invoked integration parses three on-disk manifests, expands their
transitive mounts, serializes the actual tool handoff, checks the real runner,
and runs from another working directory. The result is `[42, ["sentinel"]]`.

A separate executable assembled through the public Extensions API has a builder
that writes a marker. Its capability command leaves that marker absent and the
config-open counter at zero; eval returns 42 and writes the marker as the positive
control. An accepted older executable without project-sources refuses the command.
Both executable hashes and outcomes are in capability.json. This is a protocol
and entry-boundary gate, not the native build/receipt gate.

## Remaining work

Gate 3 owns actual source fingerprinting, source-tree traversal policy and ancestor
Cargo inputs. Gate 4 owns generated project builds and PostgreSQL integration;
gate 5 owns publication of locks/receipts and interruption; gate 6 owns the matched
stock performance comparison and final regression. Toolchain changes invalidate
the planned build identity, as documented in the tool README. Notebook mapped
sources, mapped server compilation and non-Linux execution remain outside scope.
