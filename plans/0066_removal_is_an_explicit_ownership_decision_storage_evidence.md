# 0066 gate 3: real storage and surviving-consumer evidence

Status: ready for review. The product is byte-identical to accepted gate 2,
`06a295e`. This checkpoint adds only plan/evidence text in rnx, with the fixture
and results in rnx-bench `39467e8`, `probes/removal-storage` and
`results/removal-storage-0066`. Gate 4 remains open.

## Assemblies and provenance

The fixture installs the `7cd3205` runtime with its genuine old tool, renames the
fixture checkout, and builds an authentic SHA-256 Polars assembly plus a small
adapter that reads a retained OUT_DIR file. Its first assembly cache is empty.
The accepted current tool authenticates migration into a BLAKE3 runtime ID,
leaving the old entry unchanged. No user installation or cache is used.

All ordinary commands use the frozen gate-2 ordinary tool; the interrupted-removal sequence
uses the frozen test-support binary to pause, inspect pending data and execute
its printed resume command. `checks.json` pins both tools,
the launcher and fixture sources and verifies unchanged product source. The
old tool hash and initial build provenance are in `old-setup.json`. The source
of that old tool and runtime remains recoverable at its published revision.

The accepted session-dogfood driver runs against the migrated installed default,
without RNX_DEP_RUNTIME. Its only adaptations are the installed runtime path,
an initial allowance for the retained old key and a runtime-notice assertion.
Both new-format keys require real cold-target compilation; the registry sources
are already available offline. The second scratch and relative combined project
attach with compiler invocation traps. All four frame/error/recovery/Parquet and
typed PostgreSQL assertions remain unchanged. The effective driver and logs are
retained, including phase observations, not reconstructed after the run.

## Old-key kernel and explicit stopping

An old session and an actual Jupyter kernel installed against the old artifact
read its retained output before inspection. In two rounds the fixture changes
that output, takes both writer locks, and runs list and annotated dry-run against
the cache and runtime store. The annotations name the old and current projects.
The session retains its binding and the kernel executes another cell reading
the new marker after each round. Inspection changes no cache metadata, runtime
bytes, old project files or kernelspec. No reader lease is inferred from this.

Both processes are explicitly shut down and reaped before mutation. The old-key
kernelspec is retained and unchanged through removal, illustrating the future
reference that removal does not repair. The fixture removes that temporary
notebook environment only during its final cleanup.

## Interruption, kept data and recovery

The old assembly (1,711,506,417 logical file bytes) is renamed to pending and removal is killed with
SIGKILL during unlink, not only before deletion begins. The visible entry is
absent and the pending entry is inspectable. Running the exact printed resume
command finishes only pending deletion. The old unselected runtime is then
removed. Every file in the kept assemblies is compared by content and metadata;
the current runtime, selection, project locks/receipts, kernelspec and permanent
lock inodes remain unchanged.

Corrupt owned data remains removable. In addition to a damaged installation copy
in the main journey, a supplementary private store contains a clone under its
authentic ID and document. Appending bytes to a loose object makes runtime select
refuse as corrupt, without selecting it; explicit removal succeeds. The real
selected store stays unchanged. This does not relax install authentication.

The fixture deliberately removes the current combined assembly too. Ordinary
eval refuses, with exec tracing showing no Cargo/compiler invocation. An explicit
offline build takes about 112.5 seconds and reconstructs the same key from retained installed source. Finally a
fresh stock session runs :dep for Polars and PostgreSQL, attaches to that combined
entry, evaluates Polars and a typed query against a new private cluster, and exits.
The selected runtime and its source remain unchanged.

## Fixture correction

The first run passed the four new session journeys, live-kernel inspection and
old-entry removal, then failed while creating a corruption control: the copied
Git loose object retained its read-only mode. The fixture now gives only that
copied object owner-write permission before damaging it. The failed attempt is
retained under `fixture-readonly-attempt`, and the complete journey was rerun
from fresh storage with unchanged product binaries. The trace check also matches
executable basenames, avoiding false positives from Git searches under `.cargo`.

## Limits and next gate

The checks assert no fixture processes or mounts remain. Tool formatting and
notices pass; all source and binary hashes match gate 2. There is no production
change to justify repeating broad suites here. Build durations in the logs are
single-run fixture observations, not the gate-4 cost measurement. Retained
reference consequences are deliberate; no complete reference discovery, safe
version sweep or process-lifetime guarantee is claimed.

Gate 4 still owns real node/byte/free-space and wall-time observations, matched
zero-to-three launch measurements, full regressions and documentation. It also
carries the reviewer's repeatedly reproduced oversized-handshake-test timing
flake: make that test deterministic under load without changing the bounded
production handshake contract.
