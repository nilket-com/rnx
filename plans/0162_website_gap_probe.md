# rnx 0162: a website gap probe

**Status:** plan.

**The user (2026-10-03):** rnx should be able to serve a public website, "as simple and terse as python flask, but with the perf of axum", with Codex closing the gaps in rnx and a real site validating the result. The site lives in its own, separate repository; everything added to rnx stays generic.

**The cut:** a measured probe, not product code. It answers two questions with evidence, and ends with a ranked list of the records that close the gaps:
1. **What does a small real site need that rnx lacks?**
2. **What does the Rune path cost** against an axum ceiling and a Flask reference?

**Out of this record:** any rnx, server or adapter change; the site's real content and deployment; TLS (a reverse proxy's job); and databases. The 0054 PostgreSQL pool exists but isn't needed for a first site.

## 1. What exists today

- **`rnx::server`** (record 0056, `server-runtime`) is an embedding API:
  - `Program::compile` or `compile_source` (0107) compiles once;
  - each request `prepare`s a fresh context and invocation (with per-request ownership, a budget and a lifecycle), then `run`s and `close`s it.
- **The only HTTP host is the example `servers/http-postgres`:** hyper 1.11, hyper-util and tokio, about 900 lines of Rust.
  - It routes exactly `POST /healthy`, `/await`, `/cpu`, `/fail` and `/db` in Rust to `main(request)`.
  - One request per connection (no keep-alive), HTTP/1 only, loopback only, 1 MiB bodies.
  - Its own README says it's "not a public HTTP/TLS deployment or a configurable web framework", and 0054 that it's "not a performance claim for a Flask replacement".

## 2. The probe

**One skeleton site, three implementations,** each serving identical bytes. Its routes:
- `GET /`: an HTML home page of about 5 KB with a header, navigation and footer;
- `GET /static/site.css`: a static file with its content type and caching headers;
- `GET /hello/{name}`: a dynamic page that escapes `name` into HTML;
- a 404 page; and `POST /contact`: a small form whose fields are echoed back escaped.

**The implementations:**
- **A. axum only (Rust):** the performance ceiling.
- **B. axum host with Rune handlers through `rnx::server`:**
  - a prototype host in rnx-bench, not product code: every request goes to Rune `main(request)` with 0054's per-request preparation;
  - the routes, escaping and page assembly are written in Rune with what rnx has today;
  - every gap is recorded where it bites, such as no route table or path parameters, no HTML-escape helper, no static-file serving, no content types, and form and query parsing.
- **C. Flask:** the terseness and Python reference, pinned, served by a pinned production WSGI server (gunicorn), with its worker count stated.

**Each implementation's response bytes** are checked equal to a committed fixture, so performance is compared on identical work.

**Measured, with the method stated** (pinned cores, warm, interleaved runs, a pinned load generator such as `oha`, installed by the probe):
- requests per second and p50/p99 latency for `/` and `/hello/{name}` at a fixed concurrency, with keep-alive on (A, C and B's host);
- per-connection cost with keep-alive off;
- resident memory at rest and under load;
- **terseness:** the lines and characters of each implementation's site code (Rune vs Python vs Rust), with the host code counted separately.

No claim about production capacity; these are this machine's numbers.

## 2a. Amended after Codex's review (R1–R3), before implementation

**R1. HTTP behaviour and workload are frozen before timing.** `probes/web-0162/fixtures/` holds the request and response fixtures for every route: status, body bytes and the relevant headers (content type, the static file's `cache-control`).
- **Specified behaviour:**
  - `HEAD` on a page returns the GET headers with an empty body;
  - other methods on a known path return 405 with `allow`, and an unknown path returns 404;
  - `/hello/{name}` percent-decodes as UTF-8, refusing invalid UTF-8 with 400;
  - `/contact` takes form decoding (`+` as space, percent escapes, a missing field as empty, the first of duplicates);
  - a small hostile set (`<script>`, quotes, `&`, `</p>`) must be escaped in the echoed HTML.
- **Comparison:** only `date` and `server` are ignored, by name.
- **Before any timing,** a checker runs every fixture against each live implementation. The benchmark fails on any unexpected status, body mismatch, timeout or error, and only successful responses count.
- **The work is stated per implementation:** the home page and CSS are built once at start-up in all three (A and C embed them; B's Rune program assembles them per request, the work under test), and nothing is read from disk per request.
- This is a small boundary matrix, not a web-framework security record.

**R2. The benchmark is executable and comparable.**
- **Pinned:** versions and lockfiles (Rust, `oha`, Python, Flask and gunicorn), and HTTP/1.1 only.
- **Workers:**
  - **A:** axum on a multi-thread runtime with *W* worker threads.
  - **B:** an axum acceptor plus *W* rnx worker threads.
  - **C:** gunicorn with *W* worker processes of the `gthread` class, which keeps connections alive (the `sync` class doesn't), with the thread count stated.
- **Placement:** servers and the load generator get disjoint CPU sets. Conditions run interleaved (A, B, C in rotation) with a stated warm-up, duration and repetitions.
- **Keep-alive is verified on the wire** for every implementation: connections opened and requests per connection, from `oha` and a socket count. Successful-request counts and connection reuse are recorded, and an implementation that can't keep alive is reported separately.
- **Memory:** RSS sums all of an implementation's processes (every gunicorn worker plus its arbiter; the Rust process with all threads), sampled at rest and at mid-run.
- **Distributions,** not single numbers: the median and spread over repetitions, and p50/p99 latency.
- **Two loads:** concurrency 1, where preparation cost shows without queueing, and a fixed saturating concurrency.

**R3. B keeps the existing lifecycle, and costs are attributed.**
- **Compilation stays outside measured requests:** `Program` is compiled once at start-up.
- **Per request,** as in `servers/http-postgres`: the axum handler builds an owned request and sends it through a bounded queue (full means 503) to one of *W* worker threads. Each thread has a current-thread runtime and a `LocalSet`, with at most 4 active invocations. The worker `prepare`s with the request value built on its own thread, `run`s, converts the response to owned data while its values are valid, then `close`s explicitly on success and on failure, and replies through a oneshot.
- No `unsafe Send` and no lifecycle replacement. The instruction budget is 10,000,000 per request.
- **A control:** a handler error, a budget exhaustion and a refusal each return 500 with cleanup observed, followed by a healthy request on the same worker.
- **Phase timing:** prepare, VM run, response conversion and close are timed separately on a short controlled sample through an instrumented build, with those numbers kept apart from the uninstrumented HTTP timings.
- Reuse or a shared context is a later record; a high preparation cost here is a finding.

**The ranking** separates demonstrated blockers for this skeleton (with measured cost) from ideas it didn't exercise, such as cookies, templates and reload.

**After this probe,** the first product-host record must be used by a real site (in its own repository) and validate the Rune ergonomics there, so the arc doesn't become a chain of measurements without a working site.

## 3. The output

`plans/0162_website_gap_probe_evidence.md` records the numbers and **a ranked gap list**. Each gap gets what it blocks, its evidence from B, and a proposed record. Expected candidates, to be confirmed or refuted by the probe:
1. a product HTTP host: axum-based, with a Rune-declared route table, keep-alive, and non-loopback bind behind a proxy;
2. web helpers: HTML escaping and templating, static files with content types, query, form and cookie parsing, redirects;
3. per-request overhead: the cost of a fresh preparation per request against what Flask or axum pay, and whether reuse is sound under 0054's ownership rules;
4. development experience: a one-command run, reload on edit, request logging.

**The real site,** in its own repository, starts once the first host record lands, as that record's real user. Nothing in rnx depends on it, and nothing is pushed there in this record.

## 4. Out of scope

- Product code in rnx.
- TLS and HTTP/2 at the edge.
- Deployment.
- Databases.
- The site's design and content.
