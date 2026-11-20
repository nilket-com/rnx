# Implementation plan 0318: A separately linked engine runs a file

Status: Implemented

Paired ADR: ../adr/0318_a_separately_linked_engine_runs_a_file.md.

1. Boundary: Codex's 0318 proposal at 1a8d5187. Record any deviation
   before editing.
2. Add the engine package, header, gated file-execution shim, ABI v1 with
   pre-effect validation and bounded diagnostics, and the
   thread-scoped host callback.
3. Add the C launchers with digest binding and owning-mapping
   verification, and the TLS budget slot.
4. Add the callback, refusal and TLS controls. Capture the split pair
   through the 0317 gate and report the stock-to-sibling boundary cost
   separately.
5. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

On the GNU/Linux x86-64 host, a single release build of the engine gave
both separately compiled C launchers SHA-256
`7fe7c415ec4c605ce2693f2fa93304e1388755033ed409956c4ef68fc27a2aef`
for the shared object. The same file returned 42 and 43. Each debug
invocation reported the execution route and one host callback. The
relocated pair, refusals, failures after a callback, bounded diagnostic
writes, concurrent callers and late-provider TLS controls passed.

The ordinary eight-kernel capture recorded both launcher roles at sizes
1 and 5 with native code off and on. All 32 per-record W/T comparisons
passed. The only whole-invocation delta was native-on Permute T, down 36
instructions at both sizes. The image contributed the engine instructions.
The launchers execute outside the measured invocation on these kernels.

The separate comparison with the stock 0317 final passed every execution
check. Eight native setup comparisons exceeded the stock per-change
limits. At size 5, the additional instructions and corresponding limits
were:

| Kernel | Count | Increase | Limit |
| --- | --- | ---: | ---: |
| loops | W | 1,115 | 37 |
| numeric_calls | W | 1,081 | 185 |
| shared_rows | W | 198 | 37 |
| shared_rows | T | 404 | 11 |
| awfy_sieve | W | 2,284 | 314 |
| awfy_permute | T | 238,349 | 213,276 |
| vec_iter_i64 | W | 3,331 | 233 |
| vec_iter_bool | W | 2,283 | 199 |

W counts product and unclassified work. T includes system and dynamic
work. These stock-to-sibling figures are diagnostic under decision 4.

The first signed required run failed cross verification. The workspace
metadata included the image-only feature on the stock rnx package while
the stock test executable omitted it. The image package now explicitly
enables its private dependencies with `--features linked`. Cargo builds
its named cdylib with `cargo build --locked --release -p rnx-engine-image
--example engine_image --features linked`. The feature sets of the stock
verification target and its metadata agree. The rebuilt image passed
the C controls and the complete sibling matrix again.

The second required run passed feature binding and failed the stock
host inventory because the root shim registered the sibling's callback.
The image package now constructs its own context and registers
`host::apply` beside the scoped trampoline. The shim receives the context
constructor. The stock inventory passed with its existing policy. The
table above uses a fresh capture of this image.

Required runs three and five timed out on stock census case 0007. In
diagnostic run 18, action three stalled while the journey fixture opened
its state file for truncation. Under matched buffered writes, the stock
case on ext4 took 9.018 s and timed out at 30.003 s. Both snapshots
identified the state file and a kernel folio wait. Paired tmpfs cases
finished in 0.442 s and 0.417 s with identical completed outputs.
Each observer now gets private, size-capped tmpfs work and report
directories: 64 MiB for work and 4 MiB for reports. The harness retains
their evidence after the watchdog exits. Four concurrent writers
buffered 48 GiB in total. All four were active from case 0007's first
command to its last event. That interval was 0.354 s on stock and
0.350 s on fresh, and both runs completed all 19 actions with the
reference outputs.

Diagnostic run 12 separately hit ETXTBSY in the jobs test fixture.
Its repair is on the backlog.

The isolated cross stage and fourth required run passed case 0007.
The fourth run then exposed a stale
scheduling witness. The controller fixtures now include the three
image stages and the image manifest found by the checkout pin check.
In the local controller tests Cargo's loader path selected a stale
development image ahead of the planted image. The observer fixture now
clears that path for its child, so each planted runner selects its own
image. The wrong-mapping control still exercises the observer's refusal.

On 16e7d252, with the host power profile set to performance, a required
final passed all 82 selected stages in 41.1 minutes. The complete final
on the same commit then passed all 104 stages, with 36.5 minutes of
accepted run and 37.1 minutes end to end. Both fixed-route bands held,
and there were no memory.high, memory.max or OOM events. After the host
restarted in the power-saver profile, Rune contract cases 381 to 383
exceeded their 15 CPU-second limits. In the performance profile they
used 9.66 to 9.80 CPU seconds.
