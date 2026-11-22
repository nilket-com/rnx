# Implementation plan 0324: Files run on the installed engine image

Status: Implemented

Paired ADR: ../adr/0324_files_run_on_the_installed_engine_image.md.

1. Boundary: Codex's 6/6 proposal and the split rulings, reconciled
   against published 05283812. Record any deviation before editing.
2. Embed the stock bundle on the CLI side. Add content-addressed atomic
   materialization of the image and the file helper, the pre-exec
   ownership and identity checks, and startup linking through the pinned
   descriptor. Measure full verification first.
3. Carry structured refusals across the image boundary. Keep earlier
   ABI callers.
4. Make the installed image the default for `rnx run FILE`, with the
   counted compatibility route. Remove the opt-in split and the
   in-process new-engine file path.
5. Add the install, cache-safety, route and refusal controls. Run the
   cutover packet against frozen 05283812 and the cost measurements.
6. fmt, Clippy, preflight and the targeted checks, following the
   pre-sign coverage rule. Sign. Source review, one complete final,
   Results.

## Results

The locked install ran from a relocated executable with the checkout hidden
and the network disabled. Both the empty-cache and warm-cache launches ran
the file on the engine image with `glibc.rtld.nns=1`. The cutover packet
matched frozen 05283812 on all 244 census files. Both runners completed
all 19 journey actions. The separate file controls covered refusals,
explicit legacy, argv, exits, SIGINT and an 8 MiB read.

Median launch times in milliseconds were 5.63 for stock, 14.63 for a
warm installed launch with full verification, 6.56 for a warm launch
with a valid verification stamp, and 43.89 for cold materialization.

Setup instructions for frozen 05283812 and the installed candidate
follow. W counts rnx and other instructions. T counts all instructions.
Sizes 1 and 5 ran in separate processes and gave identical setup counts.
Stock used a single process. The additional pre-exec window for the
installed dispatcher is a bookkeeping delta, not a claim that stock
startup is free.

| Kernel | Native | Size | Stock W | Installed W | Δ W | Stock T | Installed T | Δ T |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| fib | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| fib | on | 1, 5 | 45,282,382 | 45,298,034 | 15,652 | 48,470,922 | 49,286,713 | 815,791 |
| loops | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| loops | on | 1, 5 | 4,693,524 | 4,709,532 | 16,008 | 5,177,797 | 6,033,730 | 855,933 |
| numeric_calls | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| numeric_calls | on | 1, 5 | 6,840,829 | 6,856,743 | 15,914 | 7,588,047 | 8,442,524 | 854,477 |
| shared_rows | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| shared_rows | on | 1, 5 | 120,667 | 136,548 | 15,881 | 128,514 | 989,923 | 861,409 |
| awfy_sieve | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| awfy_sieve | on | 1, 5 | 15,578,803 | 15,594,715 | 15,912 | 17,136,287 | 17,977,049 | 840,762 |
| awfy_permute | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| awfy_permute | on | 1, 5 | 190,870,813 | 190,886,307 | 15,494 | 200,804,062 | 201,616,392 | 812,330 |
| vec_iter_i64 | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| vec_iter_i64 | on | 1, 5 | 9,434,776 | 9,451,045 | 16,269 | 10,582,086 | 11,429,287 | 847,201 |
| vec_iter_bool | off | 1, 5 | 52 | 16,052 | 16,000 | 52 | 862,248 | 862,196 |
| vec_iter_bool | on | 1, 5 | 8,565,489 | 8,581,710 | 16,221 | 9,533,775 | 10,378,083 | 844,308 |

The table above is the accepted freeze on ed6b1a07. The reviewed cache
repair keeps those references. In 31 interleaved installed launches, the
warm median was 6.29 ms against 6.39 ms before the repair. Copying the
embedded pair into sealed memory files took 16.22 ms end to end. This
path requires permission to execute memory files. The native-off outer
dispatcher measured 15,834 W and 860,861 T, and native-on measured
15,817 W and 860,830 T. Both fell by 166 W and 1,335 T against the freeze.

The installed-file tests exercised a read-only cache base, an unusable
XDG path and an absent home with a simulated missing passwd entry.
Each emitted the execution route with memory-file storage. An ordinary
unsealed pair was refused on that handoff. The upgrade control also ran
against the two installed executables. Publication removed the prior
generation while the script mapped from it finished with exit 0.

FINAL-v1 exposed a descriptor collision in the helper-rename fixture.
FINAL-v2 exposed a product regression. After exec, the C helper entered
the engine without Rust process startup. A write to a closed pipe killed
the process with SIGPIPE. The installed image now runs the compiler's
process entry against its own copy of std after the helper verifies the
mapping. This restores the stock SIGPIPE disposition, main-thread identity,
stack-overflow guard and diagnostic, argv initialization and exit cleanup,
including the final stdout flush. Rust also checks descriptors 0–2 again;
the outer CLI already sanitized them and the handoff uses higher descriptors.

Both pipe controls killed the v2 candidate with signal 13. On the repaired
install, closed stdout returned 1 with the same located broken-pipe error
as frozen 05283812. Sending 2 MiB to a child that closed stdin before
reading returned the child's output and status 7 to the script. The stock
and installed stdout, stderr and process status matched in both cases.
A main-thread stack-overflow probe produced the stock diagnostic and
SIGABRT; v2 had died with SIGSEGV and empty stderr.

FINAL-v3 failed the execute-guard Clippy comparison. Its baseline and
candidate each emitted the same four warnings. Cargo's stderr progress
prefix interrupted a JSON line in the combined baseline log, and the
parser skipped that warning. All 21 differential lint stages already
had distinct baseline and candidate cache targets. As a deviation in
this record, the suite stage now captures Cargo's JSON stdout and its
human-readable stderr in separate receipt artifacts. The comparisons
report malformed JSON with its log path and line number. Controls cover
the split write and compare identical crates in independent cold and
warm targets with both warning occurrences required.

The complete final on 2e437037 passed all 107 stages, with 51.7 minutes
of accepted run and 52.1 minutes end to end. Both fixed-route bands
held, there were no memory.high, memory.max or OOM events, and the host
was in the performance profile.
