# rnx 0169 evidence: standing runtime suite and complete startup attribution

**Status: measured baseline and complete registration attribution; review pending.**
The deciding directory is `results/rune-runtime-0169-attempt5b`. Its main analysis
passed; the original run.sh exited 1 at the final cross-filesystem corruption-control
clone. A reviewed read-only control replay passed all 17 corruptions, recorded as
`COMPLETE-AFTER-CONTROL-REPLAY`, not as a successful original command exit.
Codex implements, Claude cross-reviews. Shipping Rune remains 0.14.2. The fork
base remains bb8e69372353c50e271c9f115bc771c77aa6b83e; only an isolated scratch
worktree has diagnostic instrumentation.

## Controlled attempts before deciding measurements

The bounded standing command and its repair commits are in rnx-bench's
`probes/rune-runtime`. Every attempt has a fresh output directory and preserves
command, process-group, admission, exit, source and binary records. No earlier
result is overwritten.

- Attempt 1: source 2524a46. Builds and primary semantic/robustness checks ran;
  the diagnostic hierarchy check stopped on a label collision. The module
  factory interval and the install stage for constructs were both named
  `construct`; the verifier incorrectly added the outer factory to the install
  stage sum. No deciding timing ran.
- Repair reviewed by Claude: factory renamed `module-construction`, install
  stage unchanged. The exact source patch is reproducible from its generator.
  A self-audit also completed the full corpus in diagnostic capture on/off,
  including async and errors, and three primary reused-call processes for each
  of answer/numeric/fib, with 20 checked calls each. No bound was relaxed.
- Attempt 2: source 4018863. Full corpus and actual private-module omission
  controls passed. Native-clock preflight stopped before subject samples:
  `/bin/true` native/Hyperfine medians 0.560/0.261 ms, cached Rust 42
  0.593/0.341 ms. Both violate the frozen 0.15 ms agreement gate.
- A bounded launch-mode diagnostic was pre-registered with Claude: identical
  clock and target commands under ordinary subprocess launch, a new session,
  an owned process group without a new session, and the durable job journal.
  Its hypothesis concerns launch/session scheduling and logging, not Rune speed.
  Any repair and replay keeps the clock gate unchanged.

## Stock context source audit

The accepted plan's rationale for stdio-false needs qualification. At the pinned
stock source b240937, `src/lib.rs`'s `serve` actually constructs
`Context::with_default_modules()` (stdio enabled), then installs core and further
host APIs. `src/io.rs` explicitly supplies `::io::stdin` and `::io::eprint`,
**distinct from** Rune's `::std::io` printing module. Only the pure configuration
evaluator in `src/config.rs` uses `Context::with_config(false)`.

Consequently both diagnostic modes are retained as explicit engine comparisons;
stdio-false is not called stock eval's context, and neither mode is the entire
rnx host context. Main-engine registration results do not validate the blocked
main migration or remove the shipped old-engine context and host work.

## Further calibration diagnostics and attempt 3

The launch-mode test did not isolate setsid as the cause: all three bare modes
passed while the journaled path failed. Subsequent matched observer blocks
(core 4 versus core 0; clock/target core 4) failed; core 0 was substantially worse.
No group-creation or observer-placement repair was applied.

Hyperfine defaults to null output whereas the native clock captures pipes.
Three reversed matched-I/O blocks gave six passing pipe pairs, native minus
Hyperfine +0.110 to +0.145 ms, a systematic positive offset rather than noise.
Claude approved only changing the calibration reference to
`hyperfine -N --output=pipe`, preserving the 0.15 ms bound and every other stop.
This does not establish timing precision below that bound or explain the entire
observer effect. Sub-ms cross-record comparisons require the offset caveat.

Attempt 3 (bench source 0698e2f, plan 421b35a) repeated all controls. They passed,
but full-runner calibration FAILED before any deciding subject samples:

| Reference | Native median ms | Hyperfine median ms | Difference ms |
|---|---:|---:|---:|
| true | 0.5082715 | 0.2796055 | +0.228666 |
| cached Rust 42 | 0.5988225 | 0.3646445 | +0.234178 |

The fresh directory `results/rune-runtime-0169-attempt3` and all earlier partial
attempts are retained. No deciding baseline or registration attribution is
accepted. A new diagnostic proposal separates durable job-start journal writes
from the clock interval using an explicit READY/GO handshake; it awaits review.
The original rustc-42 preflight uses bare subprocess.run, whereas this runner
fsyncs a job-start record immediately after spawning the clock. Possible overlap
is a hypothesis, not an established causal explanation.

## READY/GO and resident-observer diagnostics

The reviewed READY/GO diagnostic retained the original target interval and
fsynced job-start before GO. The marker control passed, but all six handshake
calibration pairs failed: true differences +0.197/+0.191/+0.242 ms and cached
+0.208/+0.208/+0.247 ms. GO-to-helper-start medians were 35–38 us and
helper-end-to-controller-reap approximately 1.18–1.20 ms, outside the timer.
No handshake repair was applied. A later lifecycle-control rerun passed;
one mistakenly outer-locked invocation was terminated because that control
intentionally acquires the same lock internally (not measurement evidence).

The next reviewed diagnostic used one resident measuring helper per block,
with each target still a fresh Command::output invocation and exactly the same
Instant interval. Results are in `results/rune-runtime-0169-resident-blocks`,
source 71d7735. First the exact committed rustc-42 preflight at 82e1886 ran with
copies of its original retained binaries: true 0.3430845 ms and cached 0.386225 ms,
both within its original 0.15 ms gate against historical references.

Three reversed current/resident blocks used contemporaneous Hyperfine pipe
capture. All six resident pairs passed, native minus Hyperfine −0.019 to
−0.031 ms. All six current pairs also passed in this diagnostic, +0.119 to
+0.147 ms; the current path had failed in the full standing attempt. The
resident architecture has more observed calibration headroom here. This does
not establish a particular fsync, exec, cache, scheduler or frequency cause.
The old positive offset must not be subtracted from resident measurements.

A concrete resident-driver repair preserving the exact seeded shuffled sample
sequence, fresh subjects, matrix, gates and retained per-observation raw records
has been sent for review. No deciding full-suite replay has run yet.

## Deciding attempt and final controls

The resident repair preserves the original seeded per-sample shuffle, fresh
subject processes, 36 identities, 41 groups, 2,265 observations, per-record raw
outputs, budgets and every clock/baseline/overhead stop. Each command plan is
preconstructed; actual executed argv/index order and its hash are checked.
Control targets have distinct pids; missing/extra/reordered records are refused;
a timeout kills and reaps an actual descendant. All controls run before timing.

Attempt 4 collected the complete matrix and passed the instrumentation-overhead
gate, but the first analysis stopped on the late unpinned controls: old/main's
selector chose compile-only answer before run answer. Their empty output was
correctly rejected by the analysis's declared print-42 expectation. The pinned
matrix was unaffected; no report was accepted from attempt 4. Claude approved
a fresh full replay with the corrected selector and a permanent uniqueness,
mode and output control. An initial attempt 5 was interrupted during semantic
controls to add that permanent control, before any deciding timing; its partial
ledger is retained. No data was reused from either attempt.

Attempt 5b ran from clean committed tooling **2f58d0b**. All source/binary/fixture
pins, semantic/robustness/private-module omission controls passed. The late
selector control refuses missing-run, duplicate-run and wrong expected output.
The main analysis validated:

- all 2,265 raw timing records and declared/executed plans;
- contemporaneous native/Hyperfine pipe calibration within the unchanged 0.15 ms;
- historical baseline reproduction with **zero warnings** under its original rule;
- the complete 34-factory/eight-stage hierarchy, inventories and stdio modes;
- 84 positive, >=99%-running PMU windows, 28 allocator and 28 maximum-RSS passes;
- three fresh processes x 20 reused calls for each answer/numeric/fib and engine;
- diagnostic overhead ratio **0.9917**, below the unchanged 1.20 stop.

The original driver then exited 1: its read-only corruption controls tried to
hard-link clones from /home to /tmp and failed EXDEV before mutations. Claude
approved replaying only the unchanged analysis_controls.py with TMPDIR on the
results filesystem. `control-replay/ledger.jsonl` records command, environment,
status and approval; **17/17 corruptions were refused**, the real input passed.
Original failed logs and deciding data are unchanged. The separately labelled
completion artifact states this qualification. Future invocations must set
TMPDIR on the results filesystem (documented); no benchmark or threshold was
changed to repair clone placement.

## Measured baseline and scope

| Subject / operation | Median ms | Scope |
|---|---:|---|
| stock `rnx eval 42` | 4.173 | Literal expression, normal product context/display |
| stock `rnx run` print-42 | 3.855 | Script path, explicit 1e9 budget |
| old engine fresh print-42 | 3.746 | Engine harness, not the stock product |
| main engine fresh print-42 | 4.275 | Engine harness, blocked migration remains blocked |
| Lua 5.4 print-42 | 0.436 | Fresh process under resident native observer |
| LuaJIT print-42 | 0.464 | Fresh process, default JIT behavior |

Calibration alongside the sub-ms rows: native true **0.257468 ms** versus pipe
Hyperfine **0.282602 ms** (−0.025134 ms); cached Rust **0.3536245** versus
**0.368636 ms** (−0.0150115 ms). No offset is subtracted. The 0.15 ms agreement
gate is not a precision claim below that bound. Earlier per-invocation observer
offsets belong to that architecture, not a correction to these resident rows.
`CLOCK-AUDIT.md` accompanies the generated `REPORT.md`. Later notes in the
historical probe READMEs qualify their native observer scope without rewriting
or remeasuring their historical conclusions.

The separately labelled unpinned observations are highly variable (for example
stock eval median 12.643 ms versus pinned 4.173 ms). They are not pooled with
pinned rows or used to explain a mechanism. Reused calls measure compile-once
subject calls, not CLI startup: the main answer's three 20-call average intervals
were 4.11/3.78/4.55 us per call; old 7.78/3.61/3.41 us. These are per-process
average intervals around the subject call (returned-value black_box/drop follows
the timed interval), not a
claim that a fresh rnx invocation is microseconds.

## Complete registration attribution and next experiment

For main with stdio true, paired module construction+installation medians:
iter **0.504 ms**, ops **0.477**, string **0.188**, hash_set **0.184**, hash_map
**0.140**. Private collection modules are included; none is omitted from the
constructor attribution. Stdio false is reported separately, with similar order.

Disjoint installation stages, summed per context before summarizing:
trait_impls **1.503 ms**, associated **0.296**, types **0.096**, items **0.052**;
the remaining module/traits/reexports/construct stages are retained in JSON.
The constructor's matched unmodified median is **2.323 ms**; enabled diagnostic
median **2.304 ms**. The slight negative overhead observation is noise, not a
speed improvement claim. Stage medians must not be added to their inclusive
installation/module parents. Source-site vectors over parent install intervals
are **2,531 metadata insertions, 172 trait implementations, 1,717 native-function
registration attempts** with stdio true (2,528/172/1,717 without stdio). These
are event-site counts, not Arc allocation counts. Allocation interval snapshots
and running process peaks are separate in profile-allocation.json.

The unmodified context-mode allocator passes report old **30,067 calls /
3,520,045 allocated bytes**, main **32,304 / 3,562,363**. These whole context-mode
observations include teardown and harness work; they do not prove which source
site owns each allocation or that main's entire rnx migration works.

The next W2 experiment should target trait-implementation installation first,
with associated-function preparation nested inside it. Claude's read-only
source notes identify per-function ContextType cloning, eager INTO_TYPE_NAME
formatting, and per-type default-method handler construction as hypotheses.
Start with the narrow borrow-versus-clone audit if it can preserve exact error,
ordering, metadata and ownership behavior; pre-register its predicted saving,
correctness controls and all-workload regression gates before editing the fork.
The measured 1.503 ms is the whole trait-installation stage, **not** a proven
removable bound for any single proposed change. Lazy constants/handler sharing
and snapshots remain separate design proposals. No optimization, fork-main move
or shipped dependency change is made by 0169.

## Retention and replay

All failed attempts and calibration diagnostics are retained byte-for-byte in
`results/rune-runtime-0169-raw.tar.xz`; readable deciding summaries and diagnostic
JSON also remain plain files under results/rune-runtime-0169-*. The archive is
2.73 MB; its 13,098 content files were independently checked against
`rune-runtime-0169-SHA256SUMS`. Copied historical executable binaries are excluded
from the archive and Git, with their hashes retained. Extract/replay instructions
are in `results/rune-runtime-0169-README.md`. Integration maps bind recorded pre-rebase aliases
to byte-identical probe sources after serial integration of 0170/0171. Copied
historical rustc-42 diagnostic executables and rebuilt subjects are local build
artifacts, not committed binaries; their hashes and original source pins are
retained. Reanalysis uses the pinned producer code/subjects. The final control
replay changed only TMPDIR, never observations or the producer source.
