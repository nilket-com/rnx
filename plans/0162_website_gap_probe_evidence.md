# rnx 0162 evidence: the website gap probe

**Status:** implemented, for review.

**Where it lives:** rnx-bench `5814f53` (amended after review), `probes/web-0162/`, with results in `results/web-0162/`. Product code is unchanged.

**Environment:**
- an i7-14700 on Linux 7.0.0-31-generic, with rustc 1.98.1;
- axum 0.8.9 (locked in `a/` and `b/`), Flask 3.1.3, Werkzeug 3.1.9 and gunicorn 26.2.0 (frozen in `c/requirements.txt`, Python 3.14.4), and oha 1.16.0;
- rnx at `71aff29` (plan), through a path dependency on this checkout.

## The site and its fixtures (R1)

`fixtures/make.py` is the specification. It writes the stylesheet, the 3,999-byte home page and 26 request and response cases:
- every route with GET and HEAD;
- 405 with `allow` and 404 with the escaped raw path;
- `/hello/{name}` percent-decoding (UTF-8, `+` kept, invalid UTF-8 → 400);
- form decoding (`+`, percent escapes, missing and duplicate fields, invalid UTF-8 → 400);
- hostile echoes (`<script>`, quotes, `&`, `</p>`).

`check.py` compares status, `content-type`, `content-length`, `cache-control` and `allow`, and the body. It ignores only `date`, `server`, `connection` and `keep-alive`, by name; any other header fails.

**All three implementations pass 26 of 26.** `bench.py` reruns the check on every server before timing and stops on any non-200 or error during load. All 45 timed runs were 200-only, with no errors.

**The work per implementation:**
- A and C load the home page and CSS once, at start-up.
- B assembles the home page in Rune on every request, and the CSS is a Rune constant.
- Nothing is read from disk per request.
- A and C use one manual dispatch, to match the specification exactly: idiomatic axum and Flask method routing format `allow` differently, and Flask adds `OPTIONS` and decodes invalid UTF-8 loosely.

## B keeps the lifecycle (R3)

`b/src/main.rs` (222 lines) is a prototype, not product code:
- an axum acceptor sends owned requests through bounded queues of 16 (all full: 503) to 2 worker threads;
- each worker has a current-thread runtime, a `LocalSet` and at most 4 active invocations;
- the request value is built on the worker's thread, then `prepare`d, `run`, converted to owned data and `close`d on every path;
- the budget is 10,000,000 instructions;
- there is no `unsafe Send`, and the program is compiled once at start-up.

`control.py` runs on one worker:

```
/ok: 200 in 12.6 ms
/panic: 500 in 7.5 ms
/ok: 200 in 5.1 ms
/loop: 500 in 88.1 ms
/ok: 200 in 3.6 ms
/refuse: 500 in 3.1 ms
/ok: 200 in 3.1 ms
{'event': 'handler_error', 'worker': 0, 'reason': 'vm: Panicked: handler failed on purpose', 'close': 'vm'}
{'event': 'handler_error', 'worker': 0, 'reason': 'vm: the budget of 10000000 instructions was exhausted', 'close': 'vm'}
{'event': 'handler_error', 'worker': 0, 'reason': 'status', 'close': 'closed'}
control ok: every failure returned 500 with its invocation closed, and the same worker served the next request
```

Each failure returns 500 with its invocation closed (a vm close for a panic or budget exhaustion; a normal close for an invalid response), and the same worker serves the next request.

**The phase cost** comes from the instrumented build (`PHASES=1`), on one worker, sequential requests over one keep-alive connection, 220 per route with the first 20 discarded. These numbers are kept apart from the HTTP timings below.

**After review (R1):** the server's phase log now goes to a file, never an undrained pipe; a full pipe had blocked the worker, so the old harness depended on the pipe's capacity. The sample is refused unless there are exactly 660 complete, finite phase records. The controls and the rerun sample (the medians are unchanged within a few µs):

```
pass: a complete stream is summarized
pass: a partial stream (one record short) is refused: Refused: 659 phase records, expected 660
pass: a stream with an extra record is refused: Refused: 661 phase records, expected 660
pass: a non-finite record is refused: Refused: an incomplete or non-finite phase record: {"event":"phases","prepare_us":1.0,"ru
pass: a record missing a phase is refused: Refused: an incomplete or non-finite phase record: {"event":"phases","prepare_us":1.0}
pass: an undrained 4,096-byte stderr pipe blocks the sample (the old harness's hazard): TimeoutError: timed out
/: median over 200 requests (µs): {'prepare_us': 2728.3, 'run_us': 79.8, 'convert_us': 2.2, 'close_us': 169.9}
/hello/world: median over 200 requests (µs): {'prepare_us': 2685.0, 'run_us': 22.3, 'convert_us': 1.6, 'close_us': 166.4}
/static/site.css: median over 200 requests (µs): {'prepare_us': 2695.1, 'run_us': 11.4, 'convert_us': 1.9, 'close_us': 166.4}
```

## HTTP timings (R2)

**The method:**
- placement: servers on CPUs 2 and 4 (two physical P-cores, siblings idle), oha on CPUs 8, 10, 12 and 14;
- workers: W = 2 for each, as follows:
  - **A:** 2 tokio workers;
  - **B:** 2 rnx workers plus the acceptor, all inside the same two CPUs;
  - **C:** 2 `gthread` workers × 4 threads, with `--keep-alive 5`;
- runs: HTTP/1.1, a 3 s warm-up (discarded) and 10 s measured, with 3 repetitions interleaved A, B, C;
- **Keep-alive:** at start-up, a wire control sends three HTTP/1.1 requests on one socket and reads three complete replies (reuse). Mid-run, `ss` counts established server connections, which must equal the concurrency (occupancy). Both now fail the benchmark if they don't hold (review R2).
- **Fail-closed gates (review R2):** a run is accepted only if `oha` exits 0 with complete JSON whose timing and count fields are present, finite and positive, its statuses are only 200, its errors are empty, and its connection sample equals the concurrency with keep-alive. Twelve stubbed controls (`bench.py --controls`) each get the stated verdict, including a missing p50, a NaN p99, a 500, an error, a missing summary, a short or missing connection count, and an `oha` that can't run or can't connect.
- **The saved 45 timed runs** (from before the gate change) pass every new check that can be applied to their retained fields: 200-only, no errors, positive timings, and all 36 keep-alive runs have connections equal to concurrency. The old harness didn't retain `oha`'s exit status, so that gate can't be established retrospectively for them; new runs enforce it. They weren't rerun. The wire reuse control passes against A, B and C (`wire-and-gates.txt`).
- RSS is summed over each server's whole process tree, mid-run in the first repetition. At rest: A 3,900 KiB, B 15,908 KiB, C 100,128 KiB.

The medians over 3 repetitions, with the range:

```
median over 3 repetitions (range):
A /             c=1  keepalive=True :     70539 req/s (70049-70634); p50 0.013 ms; p99 0.016 ms; connections 1; rss 3936 KiB
B /             c=1  keepalive=True :       321 req/s (315-321); p50 3.113 ms; p99 3.412 ms; connections 1; rss 15928 KiB
C /             c=1  keepalive=True :      6593 req/s (6574-6601); p50 0.148 ms; p99 0.187 ms; connections 1; rss 105952 KiB
A /             c=32 keepalive=True :    308591 req/s (307164-318389); p50 0.100 ms; p99 0.159 ms; connections 32; rss 5032 KiB
B /             c=32 keepalive=True :       669 req/s (668-671); p50 47.657 ms; p99 52.411 ms; connections 32; rss 17916 KiB
C /             c=32 keepalive=True :     13124 req/s (13108-13124); p50 2.426 ms; p99 7.511 ms; connections 32; rss 110792 KiB
A /hello/world  c=1  keepalive=True :     73470 req/s (73298-74314); p50 0.013 ms; p99 0.016 ms; connections 1; rss 4912 KiB
B /hello/world  c=1  keepalive=True :       325 req/s (321-329); p50 3.081 ms; p99 3.278 ms; connections 1; rss 17228 KiB
C /hello/world  c=1  keepalive=True :      6239 req/s (6211-6298); p50 0.157 ms; p99 0.182 ms; connections 1; rss 112660 KiB
A /hello/world  c=32 keepalive=True :    328649 req/s (322733-337548); p50 0.094 ms; p99 0.154 ms; connections 32; rss 5092 KiB
B /hello/world  c=32 keepalive=True :       686 req/s (683-687); p50 46.558 ms; p99 51.055 ms; connections 32; rss 17776 KiB
C /hello/world  c=32 keepalive=True :     12300 req/s (12090-12357); p50 2.606 ms; p99 8.312 ms; connections 32; rss 112704 KiB
A /hello/world  c=1  keepalive=False:     20753 req/s (20190-23132); p50 0.038 ms; p99 0.188 ms; connections 0; rss 4964 KiB
B /hello/world  c=1  keepalive=False:       340 req/s (339-340); p50 2.933 ms; p99 3.076 ms; connections 1; rss 18108 KiB
C /hello/world  c=1  keepalive=False:      4742 req/s (4726-4780); p50 0.203 ms; p99 0.276 ms; connections 1; rss 112904 KiB

```

All rows are this machine's numbers, not capacity claims. Per-run rows, with successful-request counts and status distributions, are in `results.jsonl`.

## Terseness

These are non-blank, non-comment lines and characters. "Comparable" Rune omits the CSS constant and the per-request home-page assembly, which A and C load prebuilt.

| code | lines | characters |
|---|---|---|
| A site (Rust, axum) | 113 | 3,815 |
| A host (`fn main`) | 13 | 688 |
| B site (Rune), comparable part | 82 | 3,298 |
| B site (Rune), whole file | 113 | 5,236 |
| B host (Rust prototype) | 222 | 6,902 |
| C site (Flask) | 62 | 3,228 |

## What the numbers say

- **These handlers' VM cost is low:** 10–80 µs per request in the instrumented sample, less than Flask's whole request at concurrency 1 (p50 0.15 ms). That's a statement about these measured handlers, not a conclusion about Rune performance in general.
- **What sinks B is the per-request context.** In the instrumented sample, `prepare` takes about 2.7 ms on every route, over 90% of the sum of the four measured phases, because it builds a fresh battery and extension context and runtime each time. `close` takes about 0.17 ms. The attribution is to the instrumented phases, not to the uninstrumented HTTP latency.
- **That caps B** at about 321 req/s per connection and about 680 req/s across both workers. That's 20× below Flask and about 450× below axum, with p50 latency at 32 connections of about 47 ms against Flask's 2.4.
- **B's memory** (about 16–18 MB) sits between axum's (4–5 MB) and Flask's (100–113 MB).
- **A hypothesis, not a measurement:** with a reused or cheaper context, B's per-request cost might approach run plus conversion plus host overhead. Whether any reuse is sound under 0054's ownership rules (fresh state per request, budget, lifecycle, cleanup) comes first, and is the next record's question.

## The ranked gaps

**Demonstrated by this skeleton:**
1. **The per-request context (performance blocker; measured).** `Program::prepare` rebuilds the whole context and runtime per request, about 2.7 ms, plus `close` at about 0.17 ms. Proposed record: a per-worker reusable invocation context, or a cheap per-request one, that keeps 0054's guarantees (fresh state per request, budget, lifecycle, cleanup). This decides whether "the performance of axum" is reachable at all.
2. **A product HTTP host (structural blocker).**
   - The only hosts are the `servers/http-postgres` example (fixed POST routes, no keep-alive) and this 222-line prototype.
   - Proposed record: an axum-based host in rnx (a command such as `rnx serve site.rn`, or a crate) with this probe's worker model, keep-alive, a bind policy behind a proxy, and request and response conversion.
   - A real site, in its own repository, should be its first user.
3. **Web helpers (terseness blocker; measured).** B's site needed six hand-written helpers: HTML escape, hex, percent-decoding, form parsing, field lookup and `join`. They're about 30 of the 82 comparable lines; without them B would be about Flask's length.
   - Proposed: a small web battery (HTML escape, URL decoding, form and query parsing) and the missing string pieces (`Vec::join`, `strip_prefix`, `find`).
4. **Routing in Rune (terseness).** The site routes with `if` chains. A route table with path parameters (`"/hello/{name}"`) and automatic HEAD, 405 and `allow` would remove most of `main`. This could live in the host record (2) or a small one after it.
5. **Rune and diagnostic papercuts:**
   - `return match …` doesn't parse, so it needs a binding first;
   - `Program::compile`'s failure printed through the host's `{e}` omits its location, so the host has to print it;
   - there is no `char::from_u32` or integer hex parsing.

**Not exercised here, kept separate:** templates, cookies and sessions, static files from disk, reload on edit, request logging, TLS and HTTP/2 (a proxy's job), and databases.

**Suggested order (review):** join 1 (the context cost) to 2 (the first usable host) in one arc that a real site uses, then 3 and 4 as the site needs them. This probe isn't expanded into unused web features.

## Gates

- No rnx product source changed.
- The probe's Rust and Python files are tab-indented.
- `git diff --check` is clean.
