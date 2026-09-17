# 0062 gates 2–6: catalogue commands and real consumers

Status: implemented and ready for one Linux batch review. Gate 1 was accepted at
`075e95e` / bench `a649ac4`. No stop condition was reached and no plan decision
changed. This checkpoint adds the product commands, their documentation and the
remaining evidence; it does not implement a live `:dep` transition.

## Source and reproduction

The implementation is confined to `tools/project/`: a private catalogue,
manifest-publication code and two early CLI branches. Root source and public API,
all manifests, lockfiles and notices, adapters, kernel and server are unchanged.
No dependency or persisted-format version changes. Existing command behavior is
selected by the same launch/build paths after the new command branches.

Bench `probes/adapter-commands/` contains the drivers and reproduction commands;
`results/adapter-commands-0062/` contains raw outputs, traces, project inputs,
locks, receipts, generated-wrapper observations and all timing samples.
`source.json` identifies the **measured working tree** as `075e95e` plus the saved
`source.patch`, including new source files. This preserves recoverable source,
not merely a digest. Apply the patch and stage new files before reproducing
native fingerprints. Final evidence/status text is intentionally not part of
that snapshot. Such later repository edits make the measured project locks stale
under the existing whole-native-tree rule; relock/build for a new checkout.

The release project tool is SHA-256
`0ae5dcf164914ef60ab4de0434a6cb7a35fd09493d62facd94bb17d9250b5c55`.
The stock release is
`b953b97cab82e81e58a5b88776ca57c174b8d9e17d8ad64ae874321293a13801`.
Exact sizes, support-binary hash, Rust version and checked commands are in
`regression/conditions.json` and `regression/checks.json`.

## Gates 2 and 3: authoring and publication

`rnx-project adapters` reads the private table and prints its two entries without
opening a project or installing signal handlers. `add` parses names/options
before opening the project, validates the entire candidate under the existing
project lock, and publishes only that manifest. The accepted prototype's
serialize/reparse/semantic-difference check is the product authoring path.

The **18 contract groups** pass against the real test-support CLI:

| Group | Observed result |
| --- | --- |
| Listing/add without helpers | Deterministic listing, empty working/home/cache directories stay empty for listing; positive traps for Cargo, rustc, Git and network-capable helpers fire when invoked directly, never during either command. Syscall traces show one executable and no network calls. Metadata-only fake adapters have no runnable builder. |
| Relative add and no-op | Runtime-relative suffix retained; repeat preserves bytes, inode, mtime and permissions. |
| Argument admission | Unknown/case-mismatched/duplicate names, 33 names, absent/duplicate manifest option, unknown options and non-Unicode names refuse before `.rnx` exists. |
| Project kinds | Source-only, executable override and malformed manifest refuse unchanged. |
| Cargo layout/read bounds | Missing, mismatched, inherited, registry/workspace, wrong-runtime, malformed and oversized metadata refuse; manifest/metadata FIFOs refuse without blocking. |
| Input/candidate allowance | Over-limit input and valid at-limit input whose addition crosses 1 MiB both refuse unchanged. |
| Atomic validation/layout | One conflicting member prevents all additions; closed inline native table refuses. |
| Ordering and modes | Reversed selection appends polars before postgres; mode 0640 survives replacement. |
| Declaration/parser limits | A 257th native declaration, reserved name and non-UTF-8 source refuse unchanged. |
| Product spelling/append | Legal inline-child/dotted siblings and missing final newline work; quote/backslash/emoji paths serialize; symlink followed by parent component keeps filesystem semantics. |
| Protected state | Entry, lock pair, receipt, artifact and derived-map bytes/identity stay unchanged. |
| Manifest symlink | The canonical target is replaced; the user's symlink spelling remains a link. |
| Temporary admission | Existing regular, symlink and FIFO temporary paths refuse without following, overwriting or removing them. |
| Editor recheck | Changed bytes even with restored mtime, a replacement inode, and a symlink substitution refuse before publication. |
| Writer exclusion | A paused add holds the project lock; a second writer refuses. |
| Fault boundaries | Errors after temp sync and before rename leave the original; errors after rename and at directory sync leave the complete candidate with replacement/durability wording. No partial file or unpublished temporary remains. |
| Signals | SIGINT and SIGTERM before/after rename produce signal status, old/complete-new outcomes respectively, and remove unpublished temporary state. |
| No-op publication bypass | An armed publication failure is not reached by an identical declaration. |

The ordinary release also succeeds with authoring fault/pause variables armed:
those hooks are compiled out. Errors and pauses exist only in the tool's existing
test-support mechanism. Neither command runs an adapter or claims that structural
metadata proves its Rust builder or startup behavior.

Publication opens an exclusive private temporary, writes/sets ordinary mode/
syncs, rechecks original identity and bytes, rechecks the temporary, then renames
and syncs the parent. Rechecking is not an atomic compare-and-swap with arbitrary
editors. The project command lock coordinates tool writers. After rename, error
wording reports replacement with durability unconfirmed rather than claiming
rollback. Other filesystem attributes are not copied. These limits are in the
README alongside the stale-temporary recovery instruction.

## Gate 4: Polars from name to live session

The real fixture first locks/builds a base application and successfully evaluates
through it. `add polars` leaves the old public lock pair, receipt and binary
unchanged. Launch then refuses with the existing diagnostic:
“project declarations or source map changed; run lock”. Only explicit lock/build
creates the Polars assembly.

The generated native table is byte-equal to the independently written control
for the same runtime spelling. Two consumers with different source text lock to
the same key and executable. The second attaches in **514.7 ms** with compilation
trapped and trap positive controls asserted. Version observations are allowed;
Cargo build/metadata and rustc compilation are not. This is an attachment
observation, not a free or zero-validation claim. Targets were built offline
from already-cached registry sources; compilation timing is setup, not a launch
benchmark. The retained Polars entry occupies **1,570,086,912 bytes** here.

Both projects pass the real terminal journey: retain a frame, transform it across
inputs, recover from a missing-column result, preview, write/read Parquet with
equal previews, collect the original plan again, reset bindings and still use
Polars. Eval output/status/error bytes equal direct execution. Terminal type and
size are fixed at xterm-256color, 120×30; session processes are reaped.

A separate live-session control holds `retained = 42` in a Polars session while
another command adds postgres to its manifest. The binding and Polars remain
usable; `postgres::query` is still a missing item in that running process. Add
changes a declaration, not the executable or bindings of a live session.

## Gate 5: both adapters executed

One multi-name product add authors both shipped adapters, sorted, with postgres
using `lifecycle` and polars using `plain`. The real generated wrapper contains
both corresponding 0051 registration calls. **The combined artifact is compiled
and executed**, not just inspected as source.

On a private PostgreSQL 18 Unix-socket cluster, the application runs two typed
parameterized queries with the same bound URL and SQL, checks integer values and
text containing a quote, backslash and emoji, and also executes `polars::lit`.
Rows use the adapter's existing named-object contract. The monitor observes zero
tagged client backends after completion; the private postmaster is stopped and
reaped. No system cluster or user cache is used. `real/postgres.json` records the
wrapper, executable digest, observation and reaped PID.

## Gate 6: regression and everyday launch

All checks pass, with configurations run serially and one test thread:

| Check | Result |
| --- | --- |
| Root default / test-support / combined server+source-map+support | 375 / 418 / 437 passed, zero failed. |
| Tool default / test-support | 40 passed each; the two opt-in integrations also run explicitly and pass. |
| Formatting, tool all-target strict Clippy in both configurations | Pass. |
| Root/tool notices; fresh default root selfcheck | Pass. |
| Root default dependency tree versus gate 1 | Byte-identical after path normalization; one root workspace member. |
| Protected source/manifests/dependency graphs/independent packages | Unchanged. |
| Existing shared commands and legacy behavior | 13 command groups pass, including format-1/local and format-2/shared paths, overrides, malformed entries, interactive modes and verification. |
| Shared publication/concurrency replay | All 37 cases pass, including interrupted builders/waiters and publication boundaries. |
| Original project workflow | 16 workflow groups pass; real PostgreSQL/mapped-source workflow passes on another private cluster. |
| Ordinary shared-command build | Fault hooks absent; lock/build/eval pass. |
| Windows MSVC all-target support check | Pass; existing test-only `UNIX_EPOCH` unused-import warning remains. No Windows execution claimed. |

The replay scripts and their source hashes are saved; accepted historical
results are not overwritten. Listing remains platform-independent. Mutation
keeps the existing Unix-supervision refusal on Windows.

The unchanged 0061 timing design retains **720 observations**: two consumers,
three modes, direct/default/full-verify, two interleaved repeats of twenty each,
one pinned CPU and one Polars thread. Every sample validates its output. Builds
and regression compilation stop before measurement. Medians across consumers
and repeats:

| Mode | Direct | Default project launch | Full verify |
| --- | --- | --- | --- |
| Tiny pipeline, complete process | 10.80–10.89 ms | 31.14–31.30 ms | 97.28–97.50 ms |
| Eval, complete process | 6.54–6.61 ms | 26.78–26.83 ms | 92.80–92.98 ms |
| Session, spawn through first prompt | 5.29–5.38 ms | 25.48–25.62 ms | 91.37–91.73 ms |

Default-minus-direct is **20.11–20.49 ms** in every cell/repeat, below the unchanged
25 ms gate. No speedup is claimed. Catalogue authoring does not enter these
launches or alter the identity of an equivalent explicit manifest.

## Fixture corrections and remaining scope

Three fixture mistakes are retained with their logs and explicit resumption
scripts in `fixture-corrections.json`: expecting the word “stale” instead of the
existing complete diagnostic; counting every cache entry while the combined
application built concurrently instead of checking this assembly's key; and
indexing PostgreSQL's named row objects as arrays. The corrected assertions and
fresh-cluster query pass. None required a product change or weaker gate.

Gates 2–6 are ready for batch review. The next design remains session transition:
project/scratch ownership, bounded real-startup probing, settings and explicit
binding loss. Dynamic loading, fetching/catalogue registries, cache eviction,
relocation equivalence and Windows execution are outside this implementation.
