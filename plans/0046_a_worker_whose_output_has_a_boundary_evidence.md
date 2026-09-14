# rnx 0046 evidence: a worker whose output has a boundary

Measured by Codex on nano, Linux, 2026-09-14, Rust 1.98.1, Rune 0.14.2.
Before: 428d93d, release preserved as `/tmp/rnx-0046/before`. The plan was
committed as ae102b4 before integration. The transport probe preceded production
edits and is committed in rnx-bench at fd69599; its follow-up results are committed at 071505c, under `probes/worker-boundary` and `results/worker-boundary-0046`.
No evcxr source was copied. No Jupyter implementation or dependencies are added.

## The first gate

The Unix parent uses Python `pass_fds` for precisely the child control ends.
The worker marks both FD_CLOEXEC before constructing any session or spawning
anything. The isolation fixture launches a real rnx evaluation which invokes
`process::run` on a three-second sleeping executable. After the worker exits:

| control inheritance | control EOF | sleeping descendant |
| --- | --- | --- |
| deliberately left inheritable | held for 3.003 s | survives worker exit |
| FD_CLOEXEC before spawning | observed after 1.1 ms | independently confirmed alive |

This negative control matters: ordinary Unix Command does not universally close
inheritable non-CLOEXEC descriptors. The output is in `probe.jsonl`.

The Windows parent uses an explicit STARTUPINFOEX handle list. The worker checks
pipe type and access rights through GetFileType and NtQueryInformationFile, then
clears HANDLE_FLAG_INHERIT. Standard-stream aliases are refused by Unix
device/inode identity and Windows CompareObjectHandles. This adds a windows-sys feature, not a new package.
Both the probe and the actual production transport source type-check against
x86_64-pc-windows-msvc. Neither Windows transport nor its Python launcher has
executed. Type checking is not a claim about successful Windows inheritance,
linking, console delivery, or execution.

## Integration gates

`cargo test --locked --offline`: **344 passed**, zero failures.
`cargo test --locked --offline --features test-support`: **385 passed**, zero
failures. The full suites ran sequentially. Targeted worker runs also passed separately in both builds during fixture
tightening; both full suites then passed again after the final alias refusal. The Python fixture requires
Python 3; its harness-wide 120 s watchdog is not a worker execution timeout.

The regular worker integration gate drives the real binary through inherited
control pipes, with concurrent byte collectors. It covers:

- Binding persistence through compile errors, Rune panic, budget exhaustion,
  and interruption; retained closure origin; reset epoch and input numbering.
- Source-cap and sampled-ceiling refusal with no armed message or admitted
  index; the real two-billion-instruction budget and subsequent successful input.
- Null stdin's one-read rule surviving reset; host exit's existing session refusal.
- No-newline output delivered at the boundary, raw script ANSI, stderr, output
  caps with discarded counts and continued draining, followed by another input.
- Malformed/invalid-UTF-8/oversized control frames, invalid IDs/nonces/unknown
  keys, invalid startup arguments/endpoints and a duplicated stdout endpoint, and wrong or missing acknowledgements.
- Queued interruption delivered after armed, synchronous loop, pending await,
  active process call, and subsequent successful execution. An async CPU loop
  demonstrates the retained limitation and is hard-killed and reaped separately.
- Closed control during evaluation, acknowledged shutdown, independent hard
  termination, and a blocked output handoff whose independent five-second timer
  kills the worker without waiting for the consumer to resume. No ack is sent.
- Test-support rendezvous before stdout's barrier, stderr's barrier and the
  settled control reply: kill/reap at each point yields no completed boundary.
- Config read count exactly zero, no history, no startup presentation bytes
  despite `--color=always`. Script print remains raw. Control survives stdin reads.

Unit gates bound diagnostic formatting at character boundaries, reject unknown
keys, and inject a flush failure before marker emission. Source review establishes
that the same fallible reset clears state and returns HTTP cleanup failure; the
REPL wrapper retains its old printing behavior, while the worker retires on that
failure. No new HTTP-cleanup failure injection was added by this record.

The standalone scanner gates exercise all 89 marker splits, single-byte reads,
87 incomplete-marker EOF positions, NUL/non-UTF-8 data, false prefixes and stale
nonces. Concurrent streams each retain 2,097,152 bytes while discarding the excess
and still finding their barriers. The real worker fixture separately checks its
retained prefix lengths and exact discarded counts.

`late.jsonl` records the limit, not a cure: the delayed writer is unassociated
between operations and attributed to interval 2 during interval 2, although it
was created by operation 1. `worker-transcript.jsonl` preserves a small actual
worker conversation, including source origins, reset and shutdown.

## Ordinary commands and cost

Six ordinary commands (version, help, eval, file run, JSON workload, diagnostic)
and an eight-input piped session compare byte-for-byte by exit status, stdout and
stderr. `equivalence.json` preserves the bytes. Config/history are isolated for
the session comparison. No existing transcript test was changed.

Hyperfine, no shell, 10 warmups and 100 runs, the entire measurement process
pinned to CPU 4. Means and standard deviations, milliseconds:

| command | before | after |
| --- | ---: | ---: |
| version | 0.549 ± 0.021 | 0.535 ± 0.014 |
| eval 42 | 4.086 ± 0.038 | 4.016 ± 0.026 |
| bare file | 3.740 ± 0.060 | 3.673 ± 0.018 |
| JSON workload | 11.600 ± 0.069 | 11.745 ± 1.142 |

The startup rows show no detected regression; they do not establish a speedup
or its cause. The final JSON run is noisier after the change and its higher mean
is not evidence of a resolved workload-cost difference. Binary size:
15,018,320 → 15,102,752 bytes (+84,432).
The raw export is `startup.json`. The preliminary export
`startup-including-taskset.json` is retained separately: it mistakenly included
a taskset launch in each timed command and is not the table's measurement. `startup-before-alias-check.json` is the
earlier integration binary, before the final standard-stream alias refusal.

`cargo fmt --check`, `git diff --check` and the third-party-notices check pass.
Notices remain unchanged (124 packages). Final formatting restored hard tabs in
the production transport after formatting the probe followed its external module
path; no behavior changed in that formatting correction.

## Still open

Windows execution, the upstream cooperative interruption gap for async CPU work,
and arbitrary-background-writer causal attribution remain open as the plan states.
This is an evaluation worker and a bounded parent fixture, not an installable
notebook kernel. The next layer must implement Jupyter protocol and output delivery
without weakening the control/barrier/acknowledgement boundary.
