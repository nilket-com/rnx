# rnx 0059: a project run is an everyday launch

Status: plan accepted 2026-09-17. Stage one (gates 1–2) is ready for review
in `_stage_one_evidence.md`; gates 3–6 remain open. The attribution probe is
accepted and pushed in rnx-bench 95dbf04.
The fifty-ninth record follows the accepted Polars application in 0058. Its
customer is the person running an application, not the person running tests.

## Context

The same tiny CSV-to-Parquet application takes about 11 ms launched directly
and 155 ms through rnx-project run. The accepted instrumented/control probe
retains 240 checked observations, two interleaved repeats on one pinned CPU:

| Work in the project tool | Approximate median |
| --- | ---: |
| Executable fingerprint | 123 ms |
| Native source and Cargo inventory | 16.8 ms |
| Source-map publication | 2.8 ms |
| Lock, receipt and small checks | less than 1 ms |

The script is unchanged between launches. These are ordinary end-to-end product
launches, not test-suite timings or notebook-cell timings. Native inventory
includes Git enumeration, ancestor/config audit and hashing; its components have
not yet been separately timed. The tiny Rune source inventory is about 0.05 ms.

fingerprint::one currently delegates to hash_files, which updates both the tree
and content digest for each chunk, then discards the tree. The reviewer measured
one versus two digests at about 67 versus 125 ms for the 107 MB artifact. Those
measurements motivate a single-file path; the product gate must confirm the
saving. Avoiding a redundant hash alone does not remove the per-launch binary
size cost. Map publication currently writes/syncs/renames even identical bytes.

The user has chosen the default: check source/dependency contents for stale
builds, and use cheap artifact metadata in the user's project directory. Full
artifact hashing belongs at build/receipt establishment and under --verify.
This explicitly revises 0057's every-run artifact content check. It is not an
assertion that metadata proves content identity or that another build tool has
exactly this policy. The directory and native adapters remain trusted.

## Decisions

### 1. Preserve verification while removing avoidable work first

Give fingerprint::one a single-content-digest path. Share the bounded regular
file reader/checks with tree hashing rather than copying a weaker reader. Keep
its returned file record identical: name, executable bit, length and SHA-256.
Retain non-blocking/no-follow opens, pre/post checks, early EOF and growth
refusals, shared entry/byte accounting and the detection-byte rule. Tree digests
keep their versioned encoding and two required digests; no lock hash changes.

For a content-addressed source map, compare the bounded existing bytes with the
canonical bytes generated from the verified lock. An identical regular file is
reused without write, rename or fsync. Missing maps are atomically published as
before. Different regular-file bytes are replaced from the authoritative lock,
as today's unconditional publication would do. Unreadable/oversized maps,
symlinks and special files refuse without writing through them. Derived maps
are not public locks: this never repairs a project/Cargo lock mismatch. Keep the
command's decoded-map/entry check and capability check where they apply.

Measure these changes with full artifact hashing still enabled before proceeding
to the new default. Keep that intermediate result in the evidence, not as a
second user-facing mode or an optimisation to the benchmark fixture.

### 2. Default artifact check: a recorded metadata stamp

After unchanged lock/input/wrapper/receipt validation, a normal run checks the
artifact's regular-file metadata against the stamp established by a full check.
The stamp records byte length, filesystem modification time at its available
precision, executable-mode boolean, and on Linux device and inode. Its path is
already fixed by the generated content-addressed location or the locked absolute
override path; never take a launch path from an unvalidated receipt. Record
mtime as seconds plus nanoseconds without floating-point or millisecond rounding;
handle pre-epoch values explicitly. Do not use access time. Do not add ctime as
an unstated coverage claim. Refuse metadata that cannot be represented reliably,
rather than interpreting its absence as a match.

Open the regular file without following a final symlink and without blocking on
a FIFO, and use handle metadata for the check. Keep existing path/name/size bounds
and executable-mode checks. Metadata lookup reads no artifact payload on a match.
As today, the later path-based exec is not atomic against external replacement.
Do not describe the file descriptor check as a sandbox or race-free execution.

On a stamp mismatch, hash the artifact fully against the already-recorded digest.
If it matches, atomically refresh the stamp and launch; if it does not, refuse
before any script runs, naming the artifact and build/relock recovery as
appropriate. Thus touch or replacement with identical bytes incurs one full check
without requiring compilation. Missing or non-regular artifacts refuse; a hash
failure never establishes a stamp. No mismatch retries a script or silently
updates the expected digest. Fast runs are silent.

The cheap check catches ordinary writes through changed mtime, size changes,
mode changes and replacement through changed identity. It can miss a same-size
in-place write whose mtime is restored or indistinguishable at filesystem
precision. Inode reuse and externally restored/manipulated metadata are further
limits. A person able to edit the receipt can defeat its assurances too, as
0057's trusted-directory model already allowed. State these limits beside the
run example in the README. They are accepted default coverage, not a test gap.

### 3. --verify requests full artifact verification on this launch

Syntax: `rnx-project run --manifest app/rnx.toml --verify -- script-args`.
Accept --verify once in either order with --manifest, only before the -- script
boundary. Reject duplicates and its use with lock/build; after -- it reaches the
script unchanged. Do not swallow runner options or add --no-verify. Help/usage
must explain the default and this opt-in in ordinary user language.

--verify always reads and hashes the artifact, even on a matching stamp. It
retains all input checks, not a special artifact-only launch. On success it may
refresh the stamp under the same publication rules; on failure it refuses and
leaves the old receipt unchanged. The flag is a content check for that moment,
not cryptographic authentication of a publisher or atomic verify-and-exec.

Build always hashes its output and verifies the copied artifact before publishing
a receipt, regardless of launch mode. The optimisation in decision 1 must also
avoid redundant digests here. --verify never builds, resolves, runs Cargo/rustc,
repairs public locks or needs a network connection.

### 4. Receipt migration and executable overrides are explicit

Use version 2 of the private .rnx/receipt.json, keeping the lock digest and
executable digest and adding the artifact stamp. Public manifest/project-lock/
Cargo-lock formats do not change. All new fields are bounded and validated;
unknown versions/fields, malformed digests and lock disagreement refuse.

A generated build establishes the stamp for the installed artifact, not the
pre-copy Cargo output. Capture metadata around its full verification and ensure
publication still refers to that verified identity. Publish the version-2 receipt
last with the existing atomic/fsync path, only after the input and lock checks
succeed. Do not turn a changed-during-hash file into a fresh trusted stamp.

A valid legacy version-1 generated receipt is supported: its next run performs
one full hash and publishes version 2 before launching. It does not require
rebuilding a known-good executable. A missing or malformed generated receipt
continues to refuse and ask for build; migration is not a bypass for a failed
build that removed the receipt.

Executable overrides use the same policy. Lock still hashes the named executable
into the public lock and performs the required capability check. Overrides need
no receipt today, so a run with no private receipt performs a full check against
that locked digest, then establishes a version-2 receipt/stamp. A well-formed
receipt for an older lock may be replaced this way for an override, after the
current lock and full hash succeed. A malformed receipt refuses; removing it is
safe for an override because absence leads to full verification, never a fast
acceptance. Generated lock disagreement still means build is required.

Centralise artifact validation so overrides are not hashed once in verify_inputs
and again in command construction. Input validation must not incidentally retain
an every-run executable hash. Both forms pass the same validated artifact result
to command construction; do not make an unchecked launch an accidental second
product API. Existing private helper tests should reflect this ownership split.

Receipt refresh uses the command lock and existing atomic publication. Publication
failure refuses that launch; a subsequent run can reverify. Interruptions may
leave either the previous complete receipt or the new complete receipt, never
an accepted partial one. Files remain beside the manifest only where 0057 already
permits them: rnx.lock and, for generated assembly, rnx.Cargo.lock. All metadata
and derived outputs remain within the ignored .rnx directory.

### 5. Source/dependency contents still decide whether a build is stale

Default and --verify both retain source/native working-tree content checks, Git
inventory checks, ancestor/config presence and contents, declarations, the locked
Cargo pair and generated wrapper identity. Same-size source edits with restored
mtime must still be refused. This record does not replace source fingerprints
with timestamps or narrow the tracked rnx repository input set.

Subdivide the native-inventory interval into Git enumeration, native tree reads/
hashes, and ancestor/config audit before optimising it. Report overlapping work
as such; use disjoint groups or an explicit parent/child breakdown. Optimisations
in this record must preserve the exact identities, error cases and shared bounds.
If further latency targets require a source cache or reduced coverage, stop for
a separate decision rather than hiding it behind the artifact policy.

### 6. Everyday launch is the performance gate

Compare the unchanged tool at 48831ec, intermediate full-verification build,
new default, new --verify and generated-direct, using the same generated artifact
and example. Relock/build outside timing when tracked input changes require it;
record each tree and artifact identity. Time init and the entire tiny pipeline,
not a hashing microbenchmark alone. Include actual filesystem writes, receipt
reads and exec overhead paid by the user.

Use gate 0058/0059's pinned CPU, one Polars thread set before spawn, fixed-seed
interleaved warm runs, two repeats with at least twenty samples per cell, every
output/status checked and all samples retained. Retain instrumented/control
comparisons for attribution; ordinary product launches decide acceptance. Report
first launch/legacy migration/override establishment and metadata-mismatch full
verification separately from unchanged everyday launches. No migration cost is
quietly included in warm-up and then called nonexistent.

Acceptance target on this fixture/host: the unchanged-artifact default median is
at least 70% lower than the old verified pipeline launch and no more than 25 ms
above generated-direct in each repeat. This is deliberately an everyday end-to-end
target, not a universal machine SLA. Native inventory already costs about 17 ms;
this record does not promise a few milliseconds above direct while retaining it.
If the target fails, preserve results and return to review; do not weaken source
checks, remove samples or change engine versions to meet it. Show --verify's
remaining cost equally clearly. Improving test runtime is not an objective.

## Gates and stop points

1. Single-file hashing returns the old file identity and allowance effects on
   empty, normal, boundary-size, executable, non-Unicode, early-EOF, growth,
   symlink and special-file cases. Tree fixture digests/locks stay byte-identical.
   Use meaningful existing-reader fixtures rather than assertions mirroring code.
   Real-tool full-verification timing confirms the optimisation's effect.
2. Map reuse checks bytes. Correct existing map causes zero publication operations;
   missing and different regular maps produce the canonical map. Malformed,
   oversized, unreadable and non-regular cases follow decision 1. Read/publication
   counters are test-only, with a real launch proving output and entry checking.
3. Default and --verify through both generated and override forms. A test-only
   artifact read counter proves zero payload reads on a matching default and a
   full read for --verify, migration and metadata mismatch. Observe both actual
   in-place changes and replacement: changed mtime, changed size/mode, identical
   replacement, differing replacement with copied size/mtime but different inode,
   and same-inode same-size edit with restored mtime. The last case demonstrates
   default acceptance and --verify refusal. Use an inert fixture artifact/read
   harness for intentional undetected corruption, never execute malformed code.
   Positive controls confirm the modified bytes and metadata before assertions.
4. Receipt v1/v2, missing, malformed, stale and unknown forms follow decision 4;
   generated versus override differences are covered. Pause/fault gates cover
   hash-to-stamp observation, receipt publication failure and interrupted refresh.
   A hash or input failure publishes no fresh acceptance. Flags in both orders,
   duplicates, wrong commands and forwarding after -- are tested end to end.
5. The original source/native dirty-edit, restored-mtime edit, untracked/index,
   ancestor/config, lock-pair, generator, failed-build and interruption gates still
   hold. Run the real generated Polars workflow and a private PostgreSQL/override
   workflow through the product CLI. No preexisting user directories are touched.
   Check publication counters and native-inventory subdivision without changing
   the product environment or weakening its checks for the fixture.
6. Everyday-launch measurements meet decision 6 with the unchanged/control modes
   retained. Tool tests in both configurations, formatting, strict Clippy and
   notices pass; manifests/locks/default rnx graph and public root API stay
   unchanged. No new dependency or root execution change without review. Root
   packaged README checks run if its links change; no unrelated root speed work.
   Windows type-check is separate from execution; its existing product-command
   refusal stays until Windows supervision is implemented.

## Guardrails, risks and later work

No new hashing algorithm, source cache, package discovery, daemon, background
verification, runtime worker protocol or notebook launch change. No test-only
fast path is counted as the product result. Old tools can refuse a newer private
receipt; a build with that tool regenerates its own format. No cross-version
private-receipt compatibility beyond the explicitly supported migration.

Network/unusual filesystems may have coarse timestamps, unstable identity or
expensive stat/read/fsync. Document available metadata precision and do not
promise detection stronger than it supplies. The artifact policy is intentionally
about ordinary user-owned build output, not arbitrary hostile edits. --verify
remains available when the user wants a content check.

Do not confuse the expected savings with a guaranteed result. The next review
point is this draft, then staged implementation with accepted attribution and
user-visible timing as the evidence. Future source-cache or stronger immutable
artifact designs need their own coverage decisions.
