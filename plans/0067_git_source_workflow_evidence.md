# 0067 gate 3: Git-source workflow and retained path behavior

Implementation checkpoint, ready for review. Gates 4–6 remain open. This extends
the accepted management boundary at `1f7ca46`; root runner code, Cargo manifests,
lockfiles, notices, adapters, server and kernel are unchanged.

## Acquisition decision

The initial candidate exposed Cargo's own repair boundary: with an edited tracked
file and a present `.cargo-ok`, metadata retained the edit and product lock
refused. With that marker absent, Cargo restored committed bytes before the
verifier ran. Both direct Cargo and product controls are retained, together with
the exact candidate patch. This was a policy decision, not an accepted silent
verification repair.

The user approved Cargo repair during explicit lock, with two conditions now in
Decision 5: report reacquisition and authenticate **after** Cargo, before lock-pair
publication. The lock-only observer records existing checkout HEAD/marker state;
it does not authenticate content, implement acquisition or select a checkout.
Cargo's resulting graph identifies the relevant directories. Missing-marker and
changed-HEAD reacquisitions print the path, revision and discarded-edit warning.
A mutation injected after Cargo's successful reset refuses publication. A fresh
marker is not authentication: if Cargo leaves an edit in place, lock refuses too.
The README says to restore that edit before retrying. Build, attachment and
`--verify` refuse without resetting, and name the quoted project lock/build
commands. There is no source-repair path in the verifier.

## Product integration

Format-2 declarations feed a new lock-4/receipt-5/identity-3/ready-3 workflow.
The shared publisher is reused through a private identity interface; the old
path readers remain separate. The legacy generator's exact pre-management
wrapper spelling is accepted for retained identity-2 envelopes. Explicit relock
uses today's generator and never promotes the old artifact.

The raw verifier deduplicates repositories, authenticates the commit/tree and
referenced blob identities, and compares working bytes, executable bits and the
index against raw objects with filters/routing overrides disabled. Its batched
Git children use the existing cancellation/process-group supervisor, bounded
pipes and joined input writers. Native Git and path reads share an allowance.
The root regular, bounded `.cargo-ok` is the only untracked exception; ignored
output limitations and non-atomic observation remain stated.

Everyday Git launch checks canonical directory structure and recorded context,
not source bytes. Owned manifests are excluded from the external audit on that
path; external Cargo/config/toolchain inputs remain audited, including absent
candidates. Path inputs still receive full BLAKE3 inventory, including mixed
projects. Source maps still reach run alone. Session preparation uses the same
lock/receipt/artifact validation, then the existing real startup probe and
commitment protocol. The current manager issues the replacement association.

Read-only cache/runtime annotations recognize the new envelopes and report a
Git runtime as Cargo-owned, without implying an installed runtime reference.
Code review caught the initial annotation manifest reader still using format 1;
its focused correction and refusal-before-fix are retained in the bench.

## Evidence

Bench evidence: [rnx-bench `61faa17`](https://github.com/nilket-com/rnx-bench/tree/61faa17/results/git-source-workflow-0067), `probes/git-source-workflow/README.md` and
`results/git-source-workflow-0067/`. Product patches, base revisions, private
fixture bundle, tool digests, actual project lock pairs/receipts, effective replay
scripts and journals preserve the tested sources and graph, not hashes alone.
Development results remain separate from the final fresh target.

- All 15 fixed schema vectors, 42 rejection vectors and 11 retained identity
  dimensions pass through the product readers.
- Thirteen command-contract groups cover real lock/build/attachment, marker,
  mode, symlink/FIFO, index, untracked and edited-source cases. Twelve additional
  raw-verifier controls include hostile-filter positive control, pre-read size
  refusal, missing/corrupt/forged blob, tree and commit objects, and a symlinked
  path component. Six acquisition cases cover reporting and refuse-only behavior.
- Seven actual Git-workflow publication cases cover failures before build,
  after build, before ready, after temporary ready, after ready and before
  attachment, plus a source mutation after compilation. Two real Git compiler
  cases kill the builder or waiting consumer. The original 37-case shared
  publication matrix is also replayed with production modules, with its original
  assertion set retained; its small driver does not stand in for product receipts.
- Mixed Git/path fixtures prove source policy by kind, including restored-mtime
  path refusal and external-audit invalidation. Genuine old-tool lock/receipt
  creation proves unchanged legacy launches and fresh construction after relock.
  An old tool genuinely recompiles the same key after removal is interrupted;
  resume leaves the rebuilt visible entry byte and inode identical.
- The 30-case authenticated installation migration and 42-case removal matrix
  pass, including the six actual bubblewrap mount shapes. The latter uses
  synthetic control documents to test filesystem ownership, not source identity.
- Preparation (44), startup (17) and commitment (8) matrices pass through both
  stock and compatibility frontends. These retain started work across precommit
  refusals and preserve the irreversible cleanup boundary.
- A real one-program Git installation from the private fixture origin runs
  stock `:dep polars`, fetches from an initially empty consumer Git cache, builds,
  probes and restarts with the same PID. The second stock session attaches with
  networking disabled and compilation trapped with positive controls. Both keep
  the caller's working directory, lose old bindings and get live Polars frames.
- Actual combined Polars/PostgreSQL assemblies build through both Git and path
  manifests. Each runs the shipped CSV-to-Parquet pipeline and exposes the
  PostgreSQL query function. Typed database transactions remain gate 4. Actual
  Polars ordinary launch is traced: zero native source-content opens, zero
  Git/Cargo/rustc execs and zero Internet connects. Missing/symlinked checkout
  directories refuse.

The private origin changes the package repository URL only. It proves real
Cargo acquisition and installed self-delegation, not the later published-origin
journey. Compilations overlapped independent correctness checks; elapsed times
are recorded but are not performance claims. The first final Polars journey took
about 561 seconds on its restricted CPU set under that load; the offline second
consumer took about 6.4 seconds with the debug/verification work included. Gate 5
owns matched release timing. An overwritten development cold transcript is not
claimed as retained evidence.

## Checks and scope

Root suites pass serially: default **376**, test-support **419**, runner-only
count-allocations/project-sources **389**, zero failures. Tool ordinary/support
suites pass **51/52**, with two intentionally ignored integration tests in each
(the support suite also reports its nested child separately).
Formatting, strict clippy in both configurations and root/tool notices pass.
Final annotation-specific checks are recorded separately from the preceding
workflow snapshot; they alter no launch or acquisition code. No fixture process
or mount remains running.

No public registry packaging, published Git fresh-user claim, startup regression
claim, Windows support or stronger checkout trust is asserted by this gate.
