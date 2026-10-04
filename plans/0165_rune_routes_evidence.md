# rnx 0165: Rune route tables — evidence

**Status:** implemented locally, submitted to Claude before publication. Plan
d87c446; measured implementation 678504a (before this evidence-only amend).
The separate bench commit is 2da8ba8, `probes/web-0165` and `results/web-0165`.
Application adoption follows acceptance and publication in its own repository.

## 1. Result and boundary

Stock `rnx serve` now accepts `pub fn routes()` returning bounded
`(method, pattern, handler_name)` tuples. Named sync/async Rune handlers receive
decoded `params`; optional reserved 404/405/400 hooks preserve custom responses.
GET supplies HEAD, explicit HEAD wins, and computed Allow belongs to the host.
Literal precedence is determined among full path matches, before method choice,
at the first differing literal/parameter segment. Neither row order nor a partial
match decides precedence. Capture decoding happens once, after raw slash splitting.

Programs without a compiled routes function retain their exact five-field
main-mode request and do not execute a startup table invocation. The stock-only
extension restriction, HTTP transport, slots, ownership, cleanup, deadlines,
retirement and bounded best-effort logger remain 0164's. No public embedding API
or dependencies changed. Web helpers, adapters, middleware and reload are deferred.

The table executes once on a disposable invocation before bind/workers. Public
Rune 0.14.2 `DebugArgs::Named`, instruction-function entries and `lookup_function`
prove compiled handler existence and arity without calling a handler. Function
names and bounded Rust-owned strings survive; no Rune value crosses workers.
Borrowed vector/tuple/string decoding checks counts and lengths before copying.
The retained-text cap conservatively charges pattern bytes twice (pattern and
segments), plus method/handler; metadata is bounded separately by row/segment caps.
Table validation identifies the row and routes function source location: the
first annotated instruction within that function, not a dynamically built row
literal. Startup trusted native operations can allocate or have side effects;
cooperative deadlines cannot preempt a synchronous native poll.

## 2. Correctness and refusal controls

Root test-support suites: **500 passed**, retained `root-tests.txt`. Counted
feature-off/server-runtime suites: **433 passed**, `feature-off-tests.txt`.
New controls cover metadata/qualified/async functions; nonexistent, native,
constructor and wrong-arity refusals; reserved hooks; legacy requests; borrowed
input survival; pattern/text/count caps; full-match specificity in both orders;
path-before-method; raw path/query preservation; slash/case/empty segments;
Unicode, plus, encoded slash and double decoding; malformed escape/UTF-8;
HEAD/Allow and required hook status; startup budget/timeout/unwind/cleanup;
named-handler retirement/replacement. An actual worker control retires on timeout,
creates a replacement and successfully reuses the same selected handler name.

The actual-command `host_controls.py` confirms startup refusal before listening,
table side effects occur once (including replacement), routes+main selects routes,
unused-main startup event, absent hooks, wrong-status hook, transport behavior and
legacy request shape. The inherited 0164 command controls also pass on the final
release binary (`legacy-host-controls.txt`). Both HTTP programs return the same
**26 wire fixtures and three complete responses on one keep-alive connection**.

Returned-table allocation measurements run alone after inputs already exist:
131,072 shared rows refused at 52 bytes; 10 MiB handler at 213 bytes; 10 MiB pattern
at 214 bytes. All are below the declared 64 KiB decoder-control ceiling. These
measure decoding, not allocation inside the trusted routes function.

During implementation, controls caught and fixed two diagnostic/lifecycle details:
the entry Allocate instruction has no source annotation, so attribution now uses
the first annotated instruction inside the function; a timeout's expected close
cancellation must preserve the primary timeout, while genuine cleanup failure
overrides it and stops startup. No weakened metadata lookup was needed.

Formatting and diff checks pass. Whole all-target clippy initially stopped on an
inherited `tests/fs.rs:308` non-octal permission literal. With only that lint
allowed (`-A clippy::non_octal_unix_permissions`), it completes; retained output
still contains inherited warnings. Touched production code has no new warning.
The broad suites preceded only formatting/lint cleanup and the ignored matcher
measurement addition; production semantics were unchanged. Timed runs used the
clean committed final release build, with no concurrent builds or test runs.

## 3. Terseness, measured separately

`count.py` asserts the helper/content prefix is byte-identical. Physical /
nonblank / nonblank-noncomment counts are:

| Source section | Physical | Nonblank | Code |
|---|---:|---:|---:|
| main-mode full source | 137 | 123 | 113 |
| routes-mode full source | 127 | 113 | 104 |
| unchanged helpers/content | 102 | 88 | 79 |
| main-mode dispatch | 35 | 35 | 34 |
| route declarations/handlers | 25 | 25 | 25 |
| retained Flask full source | 81 | 66 | 62 |

Flask's leading module docstring is excluded from code lines only, reproducing
0162's 62-line reference. Presentation content differs between full Rune and
Flask sources, so these totals do not claim an exact like-for-like terseness
ratio. The supported reduction here is **nine dispatch code lines**, with every
manual web helper unchanged. This is not yet Flask-level terseness.

## 4. Dispatch and startup measurements

Same final stock binary (SHA-256
`4ad1790fc9cc7c3d33d7c0af7f2565df5b92d005e062c79addcac1be0b78d0b5`),
two workers on CPUs 2,4; oha 1.16.0 on 8,10,12,14; request logs off in both paths.
Three interleaved repetitions, 3-second warmups and 10-second measured samples.
All **30 measured samples and 60 complete warm/measured artifacts** pass exit,
duration, status, error, latency, throughput, expected connection occupancy,
command and row-binding checks. Thirteen saved-evidence corruptions are refused;
unmodified evidence passes. Sources, binary sizes/hashes, Cargo.lock and tool
versions are retained in provenance.json; exact commands and raw JSON are retained.

| Condition | Main median req/s | Routes median req/s | Routes/main change |
|---|---:|---:|---:|
| home, c1, keep-alive | 11,305 | 11,356 | +0.45% |
| home, c32, keep-alive | 16,031 | 16,702 | +4.19% |
| hello, c1, keep-alive | 29,815 | 31,374 | +5.23% |
| hello, c32, keep-alive | 69,188 | 77,147 | +11.50% |
| hello, c1, no keep-alive | 15,930 | 16,040 | +0.69% |

The predeclared ≥0.95 ratio gate passes in every condition: negligible measured
dispatch overhead on this machine/workload. No axum equivalence or Flask
throughput claim is made by this record. Ranges, p50/p99 and RSS are retained in
matched/summary.json and results.jsonl. The gain includes replacing Rune dispatch
work with bounded Rust matching; matcher-only costs are not HTTP performance.

The ignored release matcher measurement, 100,000 decoded-parameter matches ×5:
one route median **135.5 ns** (135.2–257.7); 256 routes median **1,182.3 ns**
(1,181.8–1,346.8). It is a microbenchmark, separate from transport/VM costs.

0068 stock startup gate compares same-environment release builds at 2a06a81 and
678504a. Two randomized interleaved repeats, version/eval/JSON/prompt/persistent
cell: gate **PASS**, no reproducible >5% increase. Medians changed −0.065 to
+0.002 ms; full samples/commands/hash bindings retained under startup/. Version,
eval, JSON and prompt are slightly lower in both repeats; persistent-cell noise
changes sign. Binary size: 19,919,976 → 20,037,000 bytes (+117,024).

## 5. Next adoption step

After Claude accepts and the generic changes are pushed, fetch/install stock rnx
at the accepted full revision (`cargo install rnx --git ... --rev ... --locked`)
into a private root. Separately port the private application's routing with its
current copy unchanged and rerun its checker. That local application commit is
reviewed before push; neither deployment nor a visibility change is part of 0165.
