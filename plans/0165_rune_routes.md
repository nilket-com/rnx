# rnx 0165: Rune route tables

**Status:** plan, before implementation. Codex plans and implements; Claude reviews before implementation and push. The user approved this next terseness cut after 0164. All rnx changes are generic. Application content and its adoption remain in its own repository.

## 1. Opt-in surface

A program without a compiled top-level `routes` function keeps 0164's `main(request)` path and its exact five-field request. No new fields, HTTP decisions or startup VM call on that path. A program exporting `pub fn routes()` opts in, even if it also has main; there is no implicit fallback to main if its table fails.

```
pub fn routes() {
    [
        ("GET", "/", "index"),
        ("GET", "/hello/{name}", "hello"),
        ("POST", "/contact", "contact"),
    ]
}
pub fn hello(request) {
    #{status: 200, headers: #{}, body: request.params.name}
}
```

Each row is exactly a three-entry tuple: method, pattern, handler item path, all strings. Handler **names**, not Rune function values: a name is Rust-owned and can be invoked on every independent slot. VM-owned function values would need a different ownership model. A named handler must resolve to a compiled Rune function taking exactly one argument (sync or async); constructors, native functions, nonexistent functions and wrong arities are refused at startup, without executing a handler. Qualified `module::handler` paths are supported with strict Rune identifier segments. Handlers must be public/reachable to the compiler; a string does not make an unused private function reachable.

The table is invoked exactly once before binding or starting request workers, on a disposable caller-owned invocation with no arguments. The existing instruction budget and request-timeout option also bound its cooperative execution. Decode borrowed values, validate and copy only bounded owned data, drop every Rune value, close the invocation, and drop the owner before storing the table. Failure, native unwind or cleanup failure stops startup; no partially validated table serves. Startup script/native operations are trusted and can have external side effects; deadlines cannot preempt a synchronous native poll. The table is not re-executed by worker construction, replacement or requests.

Routes-mode requests add `params` (an object of decoded strings, empty when none) and `allow` (unit normally; a stable comma-and-space joined string for the method-not-allowed hook). `method`, raw `path`, raw/unit `query`, headers and body keep 0164's semantics. Rust selects a handler name and supplies one request argument; no extra VM call is added to a request.

## 2. Patterns and matching

Patterns are absolute ASCII URI paths. Root `/` is valid; query/fragment delimiters, backslash, whitespace/control bytes, malformed percent escapes, partial braces, empty/invalid/duplicate parameter names and glob/rest syntax are refused. Literal percent escapes remain literal raw text, with no normalization. Patterns may contain empty literal segments, including a trailing slash; `/a`, `/a/` and `/a//b` are distinct. Matching is case-sensitive against the raw path, not query text. No redirects or slash cleanup.

A whole segment `{name}` captures exactly one **nonempty raw segment**. Split on raw `/` before decoding. Percent escapes decode once as UTF-8, `+` stays plus, and `%2F` can yield a slash inside the captured string without becoming another route segment. `%252F` becomes `%2F`, never `/`. Invalid escape or decoded UTF-8 on a structurally matched parameter path gives 400, including when its method is unsupported. Unmatched paths are 404 without globally decoding them. `request.path` always retains its original spelling.

No declaration-order precedence. Identical patterns can have different methods.
Duplicate method/pattern rows and same-shape parameter renamings are refused.
Among complete structurally matching paths, compare segments left to right:
at the first differing kind, literal beats parameter. This permits `/posts/new`
beside `/posts/{id}` and resolves `/a/{x}` versus `/{y}/b` in favor of literal a.
Two different literals cannot both match the same raw segment. Select the most
specific full path first, then its method set; an unsupported method on a literal
route gives 405 rather than falling through to a less specific parameter route.
No partial-path greedy selection, declaration-order fallback or matching backtracking.


Methods are uppercase HTTP tokens, 1..32 bytes. GET supplies HEAD unless an explicit HEAD row for the same pattern exists; explicit HEAD wins. Other methods are exact. A structurally known path with no applicable method is 405; unknown path is 404. Allow includes implicit HEAD for GET and is stable: GET, HEAD, POST, PUT, PATCH, DELETE, OPTIONS, CONNECT, TRACE, then remaining methods in bytewise order. No automatic OPTIONS response.

## 3. Optional response hooks

Routes-mode can export `not_found(request)`, `method_not_allowed(request)` and `bad_request(request)`, each validated once as a compiled one-argument function. A present wrong-arity hook fails startup. Absent hooks produce the host's empty 404/405/400 responses. These fixed reserved hook names avoid a second configuration language and let the existing HTML wire responses survive. No hook applies to legacy main-mode.

Hooks receive the same routes-mode request, with empty params; method_not_allowed receives request.allow. A hook must return its corresponding status (404, 405 or 400), otherwise a bounded schema failure gives 500. The host owns the 405 Allow header and replaces any hook-provided Allow with the computed value; ordinary successful handlers retain normal header rules. HEAD still strips all bodies while preserving the equivalent length. Hooks run through the same selected-name job path, budgets, deadline, output limits, close and replacement rules as ordinary handlers. There is no VM call for the absent-hook responses.

## 4. Bounds and source attribution

At most 256 routes; pattern at most 1,024 bytes, 64 segments and 16 parameters; parameter name at most 64 bytes; handler path at most 256 bytes. Total retained pattern/handler/method/name text at most 64 KiB, checked before copying each row. Check outer vector length, tuple widths, and borrowed string lengths before proportional allocation; account metadata separately, bounded by those counts. Startup Rune allocation itself remains trusted, as in 0164; the decoder's caps are not a sandbox against arbitrary native allocation in routes(). Request capture work is bounded by the existing 8 KiB raw target and 16 parameters. No regex compilation or unbounded matching backtracking. An immutable table scans at most 256 x 64 segments, with borrowed path segments until selection.

Use public Rune 0.14.2 metadata/VM lookup to prove compiled function existence and arity; no compiler fork or guessed names. A small crate-private server helper, gated to http-server, can inspect Program's retained unit and source metadata; no public server API is added. Table-validation errors name their row and offending rule and are attributed to the routes function's retained source entry. This is a function location, not a claim to locate a dynamically constructed row's literal. Hook/handler validation also identifies the named item. If public metadata cannot prove the contract, stop and return to review before using a weaker lookup.

## 5. Controls and regression checks

Unit and actual-command controls cover:

- legacy main-only exact request shape/output and no table call; routes-only without main; routes + main selects routes; table executes once even through replacement; async route handler;
- malformed outer/table/row types, count/text/segment/parameter bounds before copying, invalid patterns/methods/names, duplicates/renamings and specificity in either declaration order; unknown/constructor/wrong-arity handlers and hooks before serving, with source path/line;
- root/literals, qualified names, case/slash/empty segments, multiple captures, Unicode UTF-8, +, encoded slash and double encoding, bad percent/UTF-8, raw path/query preservation;
- GET/implicit HEAD, explicit HEAD, HEAD-only, method mismatch and exact stable Allow, unknown path, hooks and their required status, hook failure/timeout, preserved Content-Length and no HEAD body;
- startup table error, instruction exhaustion, async timeout, native unwind and cleanup failure stop without listening or leaving an owner; per-request timeout/cleanup retirement and replacement still select the same handler;
- borrowed input survives successful/refused decoding where observed through a real VM; tiny-allocation controls on oversize returned strings/collections after inputs exist;
- all 0164 host controls, root test-support and counted feature-off suites, formatting/diff and clippy in touched code. Feature-off behavior and existing embedding callers stay unchanged.

No route table VM value survives startup; no Rune value crosses threads. Startup diagnostics remain bounded/terminal-safe and runtime diagnostics keep 0164's privacy contract.

## 6. Evidence and application adoption

In rnx-bench, rewrite the unchanged 0162 Rune skeleton with named routes and the three optional hooks, leaving page content and manual web helpers unchanged. All 26 wire fixtures and three complete keep-alive responses must remain byte-identical. Retain before/after sources, exact executable and tool hashes, and a replayable line census: total physical/nonblank/noncomment lines, route/declaration/dispatch lines, and unchanged helper/content lines separately. Compare to 0164's original Rune handler and the retained Flask reference (62 lines under 0162's rule). Do not claim Flask-level terseness from removing dispatch while helpers remain.

Measure the same stock binary serving old main-mode and routes-mode with request logs off: two workers, server CPUs 2,4, load CPUs 8,10,12,14; the five 0164 conditions, three interleaved repetitions, 3-second warmups/10-second measurements. All 30 measured samples and 60 complete warmup/measured oha artifacts must satisfy the inherited fail-closed gates (exit, duration, positive finite latency/throughput, 200-only/no errors, exact expected keep-alive occupancy). Retain source/lock/binary hashes, commands, fixtures, raw JSON and row bindings, plus corruption controls. Report medians/ranges and paired dispatch overhead. The phrase negligible measured overhead is permitted only if routes-mode's median throughput is no more than 5% below main-mode in every condition; if it misses, report the measured cost and return for review, without changing the gate or tuning handlers. Also retain a bounded matcher-only measurement to separate matching from VM/transport, without crediting it as end-to-end performance. Recheck 0068's existing stock startup gate against 2a06a81; a >5% regression reproducing in both repeats stops for review.

After acceptance and publication, update the private application separately with its current copy and existing checker expectations; only routing changes, no content or helper redesign. Install stock rnx from the fetched accepted hash, explicit package rnx, --locked, into a private root; all 26 application fixtures and keep-alive must pass. Make its separate local commit reviewable for Claude and the user before pushing. No deployment or visibility change.

## 7. Out of scope

HTML escaping, URL/form helpers and string API additions (next record); rest/globs, mounting, middleware, decorators, static-file I/O, templates, cookies, TLS/HTTP2/proxy trust, hot reload, adapter factories and full runtime diagnostics. No dependency change is proposed; existing axum transport remains a fallback with this bounded generic Rust matcher in front of selected Rune handlers.

## 2a. Plan acceptance clarifications

Claude accepted the plan before implementation. His specificity recommendation is
adopted as section 2 now states; controls include literal/parameter coexistence,
crossed patterns, full-match selection and path-before-method 405, in both row orders.
When routes and a compiled main coexist, emit one bounded startup event stating
main is unused. The three top-level hook names are reserved as route-row handlers
in routes mode and documented in help/README; declaring them as hooks is optional.
The unused-main event and startup routing diagnostics use 0164's best-effort logger.
