# ADR 0282: the complete final stops repeating work

Status: Proposed

Base: published bbf0a941b6eac2cf21c55b04346b56bcecc39358 (ADR 0281).

## Problem

The last complete final ran 83 stages in 136 minutes. Most of that is
repeated work, not testing:

- rnx's build script deletes its own rerun marker on every build with
  the default features, so every Cargo invocation that builds rnx
  compiles rnx and everything above it again. A Polars invocation spends
  four to five minutes on that before running a test.
- Every `cargo test` stage runs Cargo twice, once to discover the tests
  and once to run them, and each pays the compile above.
- `polars-suite` and `suite-adapter-polars` build the same package with
  the same features and run the same 310 tests. Together they take 26
  minutes, most of it compiling.
- The Rune suite runs its 50 native contract cases one after another,
  about 23 of its 33 minutes, although only the cases the manifest marks
  `exclusive` need the machine to themselves.
- Every Cargo child is capped at four jobs on a 28-thread host, including
  the lint stages that build a baseline tree and the candidate (about 17
  minutes together) and the cross-engine build.
- A final is bound to one commit, so a change to a plan's text reruns
  every stage although no stage but the commit checks reads the plans.
- Stages run one at a time. With the repeated work above removed, a
  complete final still keeps about three of the host's 28 threads busy
  on average and runs past an hour.

## Decision

1. A build with no source change compiles nothing. rnx's build scripts
   rerun when the checkout's revision or a tracked file changes, and at
   no other time. Cargo watches files by modification time, so a new
   untracked file or a mode-only edit reaches the dirty-state identity at
   the next such change rather than at once. Inside a final the tree is
   clean and fixed, so there the identity is exact; outside one, the
   blind spot is a dirty marker that can lag a build behind.
2. A test stage compiles once: its discovery and run invocations share
   one build, and the second compiles nothing.
3. The Polars package suite runs once. The stage that remains carries
   every check either stage made.
4. The native contract cases run concurrently under one CPU and memory
   budget derived from the detected host. A case the manifest marks
   `exclusive` runs with no other case running. Every case runs whatever
   another returns, so every failure is reported. Every check the serial
   runner makes stays: the case catalogue, selected and passed counts,
   contract row markers, empty stderr where required, and the
   row-differences file. One runner executes the manifest.
5. No build in the final sets a fixed job count: Cargo's jobs are the
   smaller of the detected cores and available memory over a per-job
   allowance. The count stays out of each stage's recorded command, so
   revalidation does not depend on the memory free at the time.
6. A docs-only amend does not rerun the final. A docs successor takes a
   passed complete final, checks that it still revalidates, and that the
   amend has the same parent and changes only files under `plans/`. It
   then runs only the stages that read the plans (commit discipline) and
   records both. Revalidating the successor revalidates the earlier
   final, the plans run and the docs-only diff.
7. The complete final fails when its wall time exceeds 60 minutes on
   this host.
8. Stages run concurrently. Each stage declares what it writes: every
   Cargo target directory and profile its builds use, resolved from its
   manifests and any target override, and the builds its tests start. A
   stage starts once its declared producers have passed and every
   earlier stage that shares one of those has finished, so stages that share a build directory keep selection
   order. Commit discipline and the
   fetches run before any other stage. A stage that is not declared is
   refused at selection. Each stage runs in its own worker process,
   whose main thread supervises its child; the controller assigns the
   stage's owned roots and fixes its environment before the worker
   starts. The first failure stops admission: running stages finish and
   keep their receipts, and the first failure stays the cause; a stage
   that has already failed is collected before the next one starts.
   Receipts are merged in selection order.
9. One CPU and memory budget covers the whole final: a jobserver holding
   the final's job count in tokens. That count is decision 5's with a
   quarter of the final's memory held back, since page cache, test
   children and the controllers share its control group, and a group
   pushed to its high limit is throttled until deadline-bounded tests
   fail. The acceptance receipt records the token count and the control
   group's memory events before and after. The controller takes a
   stage's tokens before it starts, all at once or none: one, or as many
   as the widest test run the stage makes, since test threads and their
   children use CPUs and memory as builds do. A test command that names
   no thread count runs four. A stage whose test width exceeds the whole
   budget is refused before any stage starts, never run on fewer tokens
   than its threads. The first token covers the stage's first
   Cargo job; Cargo and rustc take further jobs from the same pool. The
   native contract cases run on the stage's own tokens, idle once its
   builds are done, and take more from the pool; an `exclusive` case
   holds every token, so no build job or other case runs with it. Python
   test suites run outside the pool, so their fake stages and cases
   never wait on it.
10. The fixed-route screen runs alone: it starts when no other stage is
    running, and no stage starts until it ends. Its paired build keeps
    its pinned recipe outside the jobserver.
11. No stage writes tracked source. The Polars suite writes its oracle
    report to an owned path, which its declared-output check validates;
    the tracked report stays unchanged. No git command of the controller
    or a stage refreshes the checkout's index while stages run, since
    some stages fingerprint `.git`.
12. Admission checks only what the selected stages use, before any stage
    starts: Valgrind when the fixed-route screen or the column probe is
    selected, the Jupyter client when the kernel stage is. Disk
    admission requires a floor plus the declared bulk of every selected
    stage and the growth of every Cargo target directory they write: a
    share of a warm one, a fixed allowance for a missing one. These are
    estimates from the measured warm configuration, not a bound on a cold
    first build. Before each stage starts, the same sum over the stages
    not yet finished must fit, or the final fails there. The
    receipt records the estimate, the binding filesystem and the lowest
    free space seen.
13. A test that interrupts a pending await sends its signal once the
    await reports, from inside the operation, that it is running, never
    after a fixed delay: on a busy host startup can outlast the delay, so
    the signal would land before the operation it tests. Startup has its
    own bound and its own diagnostic.

## Constraints

- No test, check or contract row is dropped, sampled or narrowed. The
  stage list changes only by removing the duplicate Polars run.
- No real environment or argv in a committed fixture.

## Controls

- Two consecutive builds of rnx with the default features: the second
  compiles nothing, and after one tracked file is touched exactly one
  build recompiles rnx (shown in the packet). No build script deletes a
  file (committed control, with a planted negative control).
- The complete final passes on the candidate under 60 minutes; the
  packet shows each stage's duration against the last full pass.
- A failing contract case while others run: every case still runs and
  the failure is reported (committed control). An `exclusive` case never
  overlaps another, and the others do overlap (committed control from
  the cases' timeline).
- A docs successor accepts an amend that changes only `plans/` and
  refuses one that changes any other path, one with a different parent,
  and an unchanged commit (committed controls).
- A planted 61-minute wall time fails the final (committed control).
- Scheduling: stages that share no resource overlap; stages that share
  one never overlap and keep selection order; a declared producer ends
  before its consumer starts; the fixed-route screen overlaps nothing; an
  undeclared stage is refused. After a failure no
  stage starts, a running stage finishes with its receipt, and the
  first failure is the reported cause; a failure already returned is
  seen before the next admission. A signal to the controller while two
  stages run drains both and releases (committed controls).
- The final's job count on a unit with 48 GiB of headroom is 12, where
  a single build's is 15 (committed control).
- The jobserver: a Cargo build of four slow build scripts under the
  final's pool runs them one at a time while the controller holds every
  token, and overlaps them when it does not, by their recorded times; the controller
  takes a stage's tokens whole or not at all; an `exclusive` case starts
  only when it holds every token, whatever its stage holds; a one-token
  pool finishes the contract cases; two case workers sharing the stage's
  slot pipe each get a token without blocking on it; a test width above
  the budget is refused before any stage; fake case tests finish while
  an inherited pool is exhausted (committed controls).
- A stage that writes a tracked file while another runs fails the
  other's source check (planted control); the Polars suite leaves the
  tracked oracle report unchanged and its archived report validates
  (committed control); git in the final runs without optional locks, and
  a status that would refresh a stale index leaves it unchanged
  (committed controls).
- The session interrupt tests pass fifteen times beside 26 busy threads;
  an input that never reaches the pending await fails before any signal
  (committed control).
- Admission: a docs selection needs neither Valgrind nor the Jupyter
  client; a selection with the kernel stage and no client fails before
  any stage; disk short of the estimate fails before the first stage,
  and short of the remaining estimate fails before the next; a missing
  target directory counts its full allowance (committed controls).

## Out of scope

Reusing stage results across code changes (each stage keyed by the
files it reads), and porting the controller to Rust; each follows.
