# Implementation plan 0294: The cross launcher runs in Rust

Status: Implemented

Paired ADR: ../adr/0294_cross_launcher_runs_in_rust.md.

1. Pin 152b0c07 and generate the witnesses from the published Python,
   with controlled values, before any change.
2. The pin checks of `tools/engine_source.py` in Rust, with their
   controls.
3. The `cross` stage mode in its own module: entry points, sequence,
   owned target, terminal receipt and census evidence.
4. The verifier's launcher binding: the executable identity in the
   request and its check before and after; the wiring comparison on the
   launcher module.
5. Catalogue, trigger, producer lists, README; retire the Python files
   with tombstones and route updates.
6. The Rust suites, the verifier's own build, the Python discovery,
   preflight and the commit checker; sign; source review; one complete
   Rust final; Results.

## Results

`cross` runs `rnx-final stage cross --suite all --out PAYLOAD`, a producer
the child contract admits, on the same scheduler lanes and test threads.
`tools/final/src/cross.rs` keeps the published launcher's sequence: the
prerequisites, the pin consistency check, the private manifest and lock,
the metadata resolution and its check, the test build and the artifact
stream, the controller, then the host check whenever it left a bound
child, the result frozen, the owned target released and the terminal
receipt with the census evidence. The metadata, build, verifier and
host-check children run under the guardian, which drains each one's
whole group; the bwrap version and Git ignore checks are direct queries. Inside a final, the assigned payload is
checked first, then the stage's context. A standalone `--out` is resolved
where it really lands, following every link, before any boundary check
or directory is made. A pass is the controller's own regular-file PASS
result: a missing, non-regular, unreadable or failing result is a FAIL
receipt and a nonzero exit, and the entry is kept without being followed.
A standalone launch turns SIGINT and SIGTERM into a stop, then drains,
releases its target and publishes FAIL; inside a final, the controller
owns interruption. The request names the running executable, digested
through its open inode, at a path that must hold those bytes. The
verifier checks that file in its registration binding before and after,
compares `tools/final/src/cross.rs` between the two roots, and runs its
launcher controls at every controller start (`LAUNCHER-SELF-CHECK.json`).
The verifier's own unit tests have never had a runner; that is a
separate backlog record.

Retired: `tools/verify.py`, `tools/test_verify_pins.py`,
`tools/engine_source.py` and `tools/test_engine_source.py`, with the
`pin-controls` and `engine-source-controls` stages; their checks are the
Rust tests in `tools/final/tests/cross.rs`. The Python list holds 303
files. The witnesses at 152b0c07 cover 44 pin-check cases, 12 refusals
and 7 intercepted runs. The catalogue, selection, scheduling and required
fixtures record the changes as reviewed successors.

The source review held the record twice. The first hold was for a linked
`--out` checked in one place and made in another, a missing controller
result that still exited 0, the launcher digested by pathname, and no
cleanup on a standalone interrupt. The second was for a frozen result
read through a link or as a directory. Real-stage controls
cover the output path, missing and invalid results, and interrupted
cleanup. The request and standalone evidence record the launcher digest,
and the verifier's self-check rejects mismatched launcher identities. Two standalone runs passed the full
suite in 6 min 14 s and 6 min 18 s.

The complete Rust final passed on the third run, in 37.3 minutes, with
no memory.high, memory.max or OOM events. The first run stopped at disk
admission before any stage (99.3 GB free, 104.3 GB needed; development
build caches freed). The second failed four unchanged `producer.rs`
tests: their fixture commits signed through the user manager's GNOME
keyring agent, which refused. The third run named the working agent.
That run reused the second run's builds, so its time is not comparable
with a cold final's. All 61 Rust stage contexts carry the controller
digest the manifest records. The `cross` stage took 337 s (381 s in
0293), and 101 stages ran (103 in 0293), without the two retired ones.
