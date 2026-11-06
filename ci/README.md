# Commit preflight

Run `python3 -B ci/preflight.py` before builds, tests or measurements. The host
acceptance runner, engine prerequisite and instruction-count launcher invoke it
before expensive work. The engine's historical `records.py` command delegates
to this root policy. `ci/commit_subjects.py` produces the history receipt without
running controls. Both commands are read-only.

The checker reads signed committed objects. A dirty working tree is not evidence
for a missing plan or a valid signature. Subjects are `plan: adr NNNN summary`
or `impl: adr NNNN summary`, lowercase-starting printable ASCII with no scopes,
parentheses, delivery labels or body. Author and committer use the repository's
established identity. One plan precedes each implementation on first-parent
history; `MAX_IMPLEMENTATIONS` is the single cap, currently one. Plans contain
exactly their ADR and implementation-plan Markdown files. Decisions have one
opening `Status:` line and at most 400 lines. Implementations cannot rewrite
another record's plan. A pending plan is valid.

This is adapted from ket's `ci/commit_subjects.py` at
`9e7e3ad122f9ca6a3042bd12845f89ab9c1426ce`: metadata validation,
committed-tree inspection, subject grammar, plan lineage and decision-size
checks. rnx deliberately uses paired plan files, its 0206 numbering boundary,
and one implementation instead of ket's current five. The adoption cutoffs are
fixed per original repository. The historical 0206 host follower is recognized
by exact commit and reference; it does not establish a general missing-ADR
exception. The exact original import merge is verified, while unreviewed future
merges fail closed. Supporting another merge requires an explicit policy review;
second-parent commits cannot bypass validation. Shallow clones, replace refs and
grafts are rejected. Published history is never rewritten to satisfy this gate.

# Test concurrency

A failure that occurs only with parallel tests is a defect candidate, not a
flake. Passing alone does not exonerate it: isolation may remove its trigger.
Preserve every failed receipt. Reproduce under representative cross-process and
in-process contention, using an explicit bounded campaign and recorded thread
counts. Do not retry until green or automatically serialize the suite. An
exclusive-resource lane needs a concrete resource requirement and a documented
reason; fixing the race is preferred to hiding it.

Long jobs produce durable terminal receipts. Wait for exit notifications rather
than polling or running progress watchers. Resume only completed cases whose
identity and artifact hashes match; never overwrite a failed observation.

# Disk admission

Maintained column and supplemental-Miri launchers run `ci/disk_space.py` before
expensive work. Require 20 GiB available on the source/build and output
filesystems, including an explicit `CARGO_TARGET_DIR`. A nonexistent output
path is checked against its nearest existing ancestor. Remote acceptance checks
both the bundling host and the verifier. This is a minimum reserve, not a promise
that every full suite fits; preserve failures and reclaim only identified,
reproducible closed-record build artifacts. Never automatically delete source,
parked bundles or acceptance receipts to satisfy admission.
