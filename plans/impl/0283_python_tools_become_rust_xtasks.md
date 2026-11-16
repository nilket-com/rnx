# Implementation plan 0283: Python tools become Rust xtasks

Status: Implemented

Paired ADR: ../adr/0283_python_tools_become_rust_xtasks.md.

1. Pin 515e5211. Read the final's controller (`tools/column/final.py`,
   `final_evidence.py`, `stage_scheduler.py`, `gate_policy.py`,
   `accept.py`, `required_stages.py`, `package_gates.py`,
   `stage_outputs.py`, `host_budget.py`, `failure_detail.py`,
   `child_result.py`) and its tests, `rune/tools/xtask`'s
   `scheduler.rs`, and the receipts of a passed complete final.
2. Before production edits, bring two boundaries for review: the budget
   library's interface and child protocol (decision 4), and the typed
   receipts with their serialization against the Python receipts
   (decision 2).
3. The budget library: move `rune/tools/xtask`'s budget into it, leaving
   that command's behavior and tests unchanged; add the named-fifo
   protocol, `RNX_FINAL_STAGE_TOKENS`, whole-width acquisition and the
   memory policy. Process controls with the unchanged Python client and
   Cargo, including a one-token budget.
4. The command: a workspace member at the rnx root with a Cargo alias,
   only crates already in `Cargo.lock`, the bootstrap of decision 5 and
   the guardian of decision 6.
   Port selection and the stage list, then the scheduler and admission,
   then supervision, failure and signal handling, release and the wall
   budget, then revalidation and the docs successor. Each step ports the
   Python controls it covers, with the planted negatives, and adds their
   rows to the committed pairing table.
5. Cross-controller revalidation controls on fixture finals (passed,
   failed, tampered, incomplete release) in both directions.
6. The Python list from 515e5211's tracked `.py` paths, with
   dispositions; its Rust check and planted negatives. Register
   `final-controls` and `python-list` in both controllers' stage lists.
7. The parity comparison and its planted mismatches.
8. Sign the candidate. Run its complete final under the Python
   controller, then under the Rust one, never together; compare them,
   and revalidate each with the other controller. Bring each stage's
   duration beside its duration in both finals and in ADR 0282's. Then
   preflight and the commit checker. No exploratory complete final
   before this: focused controls find fixture, serialization and
   protocol mismatches first. Results state what the code does and carry
   no paths under target/ and no development history.

## Results

`tools/final/final.sh` starts a final: it takes the start time and the
candidate's commit, lock file and toolchain, requires a clean tree, disk
for the final's floor and a cold target directory, and memory under every
enclosing control group for one build job, then builds `rnx-final` with
one job into `target/final-controller`, a persistent Cargo cache, and
hands it a single-use record of all of it. The controller deletes the
record as it reads it, checks the times are finite, ordered and past,
that its running binary has the record's digest, and that the commit,
tree, lock file and toolchain are unchanged, and refuses a record from
fixture memory sources. A complete final without a record, or outside
a systemd unit that holds only itself, is refused. Its wall time is
measured from the entry's start, so the build counts.

The controller (`tools/final`, crate `rnx-final`) runs the complete,
required, explicit and docs scopes, revalidation and the docs successor
with the arguments of `tools/column/final.py`, and writes the same
receipts, byte for byte as Python's `json` writes them: sorted keys,
escaped non-ASCII text and Python's float representation. Selection, the
stage list, the package recipes, the required-stage rules, resource
lanes, weights, bulk roots and stage environments are its own copies;
committed controls compare them, and the whole selection receipt but the
controller's policy digest, with the Python controller's for every scope.
Stages run concurrently on one named-fifo jobserver from `job-budget`
(`rune/tools/budget`), which also holds the anonymous-pipe budget that
`rune/tools/xtask` uses unchanged and the memory policy; a lease of
tokens belongs to its pool, is returned when dropped, and a failed
return is reported when the pool closes. The unchanged Python stage
clients and Cargo share that pool.

Each stage runs under a guardian, a mode of the same binary in its own
session: the controller records the guardian before releasing it, and
the guardian records the stage's process before releasing it, so no
record names an unidentified process. When the stage ends, is stopped
or times out, or the controller dies, the guardian kills every
remaining descendant, detached ones included, as their subreaper, and
records its completion. Recovery stops a live guardian and waits for it,
drains the Python stage programs' own recorded groups, and releases a
final's roots only after every guardian's completion record or the end
of that final's systemd unit; otherwise it refuses. Its owner ledgers
are schema 2, which the Python recovery refuses rather than misreads.
SIGINT and SIGTERM stop every running stage, which keeps its receipt,
and the final releases. Every git command runs without optional locks.

The Python controller's edits are registrations, made the same in the
Rust controller's catalogue, and the parity record: it retains
owned-root producer receipts before release and writes the same
environment digests, bound into its terminal records. Its stage list gains `python-list`, and the
two new crates' test, format and `clippy -D warnings` stages
(`budget-controls`, `format-budget`, `clippy-budget`, `final-controls`,
`format-final`, `clippy-final`), which, having no baseline to compare
lints with, deny warnings as the Jupyter package's do. Its package table
gains those two crates, and package checks for the four packages a
change to the root lock file reaches that had none (`ci/execute_guard`,
`rune/tools/builder`, `rune/tools/generate`, `rune/tools/site`), each
with its format and lint stages. Its required stages gain `python-list`,
and a graph change that reaches `tools/final` adds that package's
quality stages. Its controls that named the required set or used
`ci/execute_guard` as an unregistered package now name `python-list`
and a package that does not exist. Nothing else in it changes.
`tools/python-files.txt` lists the 494 Python files tracked at
515e5211 as port (341), delete (151) or jupyter (2); `rnx-final
python-list` fails when a tracked file is unlisted, a listed file is not
tracked, or the list gains a path its parent lacked.

Every receipt the controller writes or reads goes through a declared
type (selection, stage context and validation, producer binding,
acceptance, manifest, release, result, successor) with its required
fields, its status, scope and phase as enumerations, unknown fields
refused, and a field Python either omits or writes as null kept apart
from one it writes with a value; only the stage programs' own receipts
stay opaque. A published final's receipts all read back as these types
and re-encode to their stored bytes. Before release each controller
copies any producer receipt that lives in an owned root into the final's
`retained` directory, checked against its binding, and records each
stage's declared environment as one digest per variable, the final's own
paths and job count substituted (`ENVIRONMENTS.json`). `rnx-final
compare` checks two finals of one candidate: it reads every receipt as
its type, requires each stage's validation receipt to match its
acceptance binding and each producer's receipt, or its retained copy, to
match its digest, and compares all of them whole, the selection, the
verdict, the environments and every producer receipt, after substituting
each final's own directory, owner and cross output; only the
run-specific places it names, times, log digests and this run's
bindings, are left out. A stage's wait for its guardian wakes on a stop
request and gives a stopped guardian a bounded time to drain; recovery
accepts a unit's end only from its own accounting; a token return gives
up after thirty seconds and reports the loss. A stage refused its whole
width holds no token while it waits, so a running stage's clients, an
exclusive contract case among them, can always take the rest; the
controller tries again when a stage ends or a signal arrives, and polls
nothing. The environment record is bound into the final's terminal
records, and the comparison verifies every binding, revalidates a
passed final, and requires each final's record to cover exactly the
stages it started, each with the keys its context declared. The cross
verifier's run-specific places are excused for its receipt alone, as
are the fixed-route screen's build and profile digests. That screen's
counts are checked by their own rule: each screen's band rows, verdicts,
dynamic rows and status must follow from its counts; the counts must
agree exactly between finals whose directory paths have one length,
since callgrind counts the startup over that path; and between finals
of different lengths, a passed witness of the same candidate, run by one
final's controller at the other's length, must agree with that other
final exactly. One case, numeric_calls with the JIT on at n = 1, may
vary by at most 16 instructions, as measured under both controllers.
Cleanup records keep their count, with only an owned build's fresh
identifier in its own directory normalized; the Polars oracle report is
compared row by row, an unordered row's output as a multiset of complete
rows with its shape and columns, and any other form raw; and each
nested Jupyter check by its own receipt after its digest is verified.

`tools/final/CONTROLS.txt` pairs the 130 controls of the Python
controller's modules: 115 with the Rust test that ports them, 13 that
stay with the Python stage programs (OwnedBuild, the native contract
runner, the cross-program gate, remote acceptance, owned-root reading,
run_process's thread rule), and 2 that patch Python internals to show a
paired Rust test would catch a loss; a committed control fails when a
Python control has no row or a named Rust test does not exist.

The record's two complete finals ran on its candidate before the
comparator's last change, one under each controller, and passed in 46.9
and 47.1 minutes, under the 60-minute limit; each controller revalidated
the other's. The comparator then changed alone: the unordered-row,
cleanup-entry and fixed-route count rules, and its command's witness
argument. It was validated by its own controls and by comparing those
two finals, with a Python fixed-route run at the Rust final's path
length as the witness, and accepted on review as an extension of their
evidence, not as a complete final of its own commit; every other file
the finals ran is byte-identical. The anchor evidence that comparison
read was hashed when it was compared, not bound by the finals; the
counts the finals bound constrain it, and the comparison checks it
against them.
