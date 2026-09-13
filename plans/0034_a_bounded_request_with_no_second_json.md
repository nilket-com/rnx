# rnx 0034: a bounded request, with no second JSON

Status: proposed 2026-09-13; revised the same day after review, and
again after implementation stopped on the resolver, which decision 3 now
settles; implemented, with evidence beside this record. This is the thirty-fourth record of rnx
and record 0031's gate 4. It gives a script
one way to make an HTTP request, and states every bound, refusal and
default before any of them is written. The scope is chosen on purpose:
`GET` by name, `HEAD` and any method with a body through `request`,
so that a script can post as well as fetch. What this record does not give
those methods is semantics beyond delivery — decision 7 — and it gives no
type that outlives the call. The lifecycle and the bounds are the work.

## Context

Record 0031 named HTTP as the battery most asked for and told this record to
choose its numbers: deadlines, body limits, whether limits apply after
decompression, whether waiting for headers and reading the body share a
deadline. Record 0032 built the only path an HTTP call can run on — one
current-thread Tokio runtime per command, kept for a session, driving the
whole execution under its whole budget with cancellation by drop. Record
0033 wrote the JSON reader that this record's responses must go through.

The starting point is upstream `rune-modules` 0.14.2's `http`, read from the
published source on 2026-09-13. It is `reqwest` 0.12 with `rustls-tls`,
`gzip` and `json`, and it exposes `http::get(url).await`, a `Client` with
five verbs, a `RequestBuilder`, a `Response` with `text`, `json`, `bytes`,
`status`, `version` and `content_length`, sixty status constants, five
protocol versions and an opaque `Error`. Four things in it decide this
record's shape:

- **`Response::json` is the defect record 0033 removed.** It is
  `self.response.json().await`, serde_json into Rune's `Deserialize for
  Value`, so `18446744073709551615` in a response body comes back `-1`.
  Installing the module as it stands would reintroduce, through HTTP, the
  wrong number that `host::json_parse` no longer gives.
- **Nothing is bounded.** `http::get` is `reqwest::get`, which builds a
  fresh client for every call and sets no timeout; reqwest's default is
  no timeout at all. `bytes` reads until the server stops sending, and it
  pre-allocates whatever `Content-Length` claims, so a header can ask for
  memory before a byte arrives. Record 0031 found the same absence and
  said the numerical policy is this record's to set.
- **`text` is lossy.** reqwest's `text` decodes by the response's charset
  and replaces what it cannot decode. rnx's `host::process` and
  `host::read` refuse bytes that are not UTF-8 and name the byte, with a
  bytes-returning form beside them. A response should not be the one
  surface that quietly rewrites its input.
- **Errors are display-only.** One `Error` type wrapping `reqwest::Error`
  with a `Display`. A script cannot tell a refused connection from a
  deadline from a bad URL except by reading prose.

The alternative is not a fork and not a rewrite of reqwest. It is a small
rnx module over reqwest directly, in the shape `host::process` already has,
so that a script learns one set of rules for bytes, text, deadlines and
limits and finds it again here. rnx-bench measured all eight companion
modules at +0.4 ms of context and +4.7 MiB of binary; this record installs
none of them and measures reqwest alone.

## Decision

### 1. The shape is `host::process`'s: one call, one deadline, one object

```
http::get(url).await?
http::get_bytes(url).await?
http::request(method, url, options).await?
http::request_bytes(method, url, options).await?
```

`get` fetches with the defaults in decision 2 and returns an object:

| field | what it is |
| --- | --- |
| `status` | the status code as an integer, `200`, `404`, `503` |
| `headers` | an object, lowercase name to a list of values, decision 5 |
| `body` | the body as a `String`, decision 4 |
| `url` | the URL the body came from, after redirects, decision 6 |

`get_bytes` returns the same object with `body` as `Bytes`. `request` takes
the method as a string (`"GET"`, `"POST"`, `"HEAD"`), and an options object
whose keys are all optional: `headers` (an object), `body` (a `String` or
`Bytes`), `timeout_ms`, `body_limit`. Every name a script can reach is one
of those four, and a key in `options` that is not one of the known keys is
refused by name, because a misspelt `timout_ms` that silently kept the
default is the kind of mistake this project exists to name.

**A status is a result, not a refusal.** `404` and `500` come back as
`Ok` with `status` set, exactly as `host::process` returns a non-zero exit
in `status` rather than as `Err`. The script decides what a status means.
`Err` is reserved for the request not completing: a URL that does not
parse, a name that does not resolve, a connection refused or reset, TLS
failing, the deadline passing, the limit being exceeded, a body that is
not UTF-8 on the text form, and too many redirects. Record 0031 asked for
status policy to be documented apart from transport failure; this is the
line, and gate 3 holds it.

No `Client`, `RequestBuilder` or `Response` type reaches a script in this
record. That is a smaller surface than upstream's, on purpose: what those
types would carry — a connection a script holds open, a body it has not
read yet — is exactly what makes bounds hard to state. Decision 7 says what
would bring them back. `POST`, `PUT`, `PATCH` and `DELETE` are reachable
through `request` and gated as far as this record can gate them: the body
goes out, the fixture reads it back. Their meaning — what a `201` with a
`Location` promises, which are idempotent — is the caller's, not rnx's.

### 2. One deadline, one limit, both with defaults, both bounded above

| | default | range | applies to |
| --- | --- | --- | --- |
| `timeout_ms` | 30 000 | 1 ..= 90 000 | connecting, every redirect, waiting for headers, and reading the body, as one deadline |
| `body_limit` | 8 MiB | 1 ..= 64 MiB | bytes of the body **after** decompression |

The deadline is reqwest's per-request timeout, which its documentation
states is "applied from when the request starts connecting until the
response body has finished", and rnx reads the body inside that window,
so there is exactly one clock and a script cannot be waiting on anything
that clock does not cover. The upper bound and the refusal for `0` are
`host::process`'s, which says "the deadline must be between 1 and 90000
ms", so a script that learned one learned both.

The default limit is the 8 MiB `host::read` and `host::stdin` already
apply, so a response and a file are held to the same size for the same
reason: a whole body is a value in one process's memory under record 0005's
ceiling. The limit counts what the script would receive. With `gzip`
enabled reqwest decompresses transparently and drops `Content-Length`, so
a limit on the wire would be a limit on the wrong number. rnx reads the
body in chunks and stops at the first byte past the limit; **`Content-
Length` is never trusted for memory** — not to skip the count, and not to
size a buffer, which is the pre-allocation upstream's `bytes` performs. A
body that exceeds the limit is refused, in `host::process`'s vocabulary for
capture, naming the limit; nothing truncated is returned as though it were
whole.

`Content-Length` **is** trusted for what RFC 9112 §8 says it is, the
framing of the message. A server that declares a length and closes after
fewer bytes has not sent a response, and rnx says so — "cannot get {url}:
the body ended after N of M bytes" — rather than returning the fragment as
though it were the whole, which is what a reader that ignored the header
would do. A server that declares a length and then sends nothing is a
server the deadline ends. So a declared 1 GiB costs the caller no memory
and no trust: nothing is allocated for the claim, and the claim is held to.

A `HEAD` request has no body and the limit does not apply to it.

### 3. The client is one per process, made on first use, dropped by `:reset`

One `reqwest::Client`, built the first time any `http::` function runs,
inside the runtime record 0032 already has, and kept for the process.
`run` and `eval` end with the process. A session keeps it across inputs so
a second request can reuse a connection, which gate 8 measures, and
`:reset` drops it along with everything else a reset drops, so what the
pool holds — idle sockets, TLS session state, buffers — is released and
`:memory` samples without it. That is the same rule record 0032 applies to
the runtime, stated for the client: a reset frees what a script could
observe, and need not free infrastructure, but the client is observable
through the allocation figure, so it goes.

The runtime gains `net`. Nothing else about record 0032's driver changes:
a request is a host future, the driver races it against the interrupt
flag on its 5 ms cadence, and cancellation is drop.

What drop does and does not do has to be said precisely, because the
runtime is current-thread. hyper spawns a dispatcher task per connection
into that runtime, and a spawned task runs only while something is inside
`block_on`. Between inputs, while the user sits at the prompt, nothing
runs. So: dropping the request future releases rnx's side of the request,
but the socket is the dispatcher's, and the dispatcher notices only when
it is next polled. Dropping the client drops the pool's handles to every
dispatcher, and the same is true of them. Neither is a socket closed; each
is a socket that will close the next time the runtime turns.

A probe settled what the turn has to be, before this record asked for it
(rnx-bench `probes/http-lifecycle`, results in `results/http_lifecycle.txt`,
reqwest 0.12.28 on a current-thread runtime against a loopback fixture,
two runs alike; the fixture reports a clean end-of-file, `Ok(0)`, apart
from a read error, and every closure below was the clean kind). After one
successful request two tasks stay alive: the connection's dispatcher and
the pool's own maintenance task. A request to a second fixture, cancelled
by drop while the first connection sits healthy in the pool, leaves three
alive, and the second fixture sees nothing for as long as nothing turns
the runtime. Turning it with the client kept closes the cancelled
connection — the second fixture reads end-of-file — and leaves the healthy
one open, which the next request reuses; but the drain bottoms out at two,
the healthy dispatcher and the pool's task, both entitled to survive, so
a bound of 100 ms on "all tasks finished" fails on a healthy client.
Dropping the client and then turning the runtime reaches zero tasks in
about 6 ms, and the first fixture reads a clean end-of-file on the socket
the pool was holding.

So "drain until nothing is alive" and "keep the pool" cannot both be the
rule after a cancel, and this record chooses the simple one: **`Ctrl-C`
and `:reset` both discard the client**, then drive the runtime until no
task is alive or a 100 ms bound passes, and the next request builds a
client again. A successful input keeps the client, so ordinary sessions
still reuse connections, which gate 8 shows. What is given up is reuse
across a cancellation, which is the rare case; what is gained is a rule
that is checkable — zero tasks, fixture-observed end-of-file — with no
need to tell a cancelled connection's task from one the pool is entitled
to keep. If the bound passes with a task still alive, the session says so
at the prompt rather than pretending, and guardrail 6 makes that a
finding. reqwest's 90 s idle timeout makes a pooled connection eligible
for eviction while the prompt waits; evicting it is still work the
runtime does on its next turn, so it is stated here so nobody measures a
socket at the prompt and calls it a leak.

The implementation must also cover a request future retained in a Rune
binding from an earlier input. HTTP work therefore runs in tracked Tokio
tasks, with abort-on-drop guards for unretained futures. Ctrl-C and reset
abort all tracked requests before dropping the client and draining. A
retained Rune future then holds an ended task, not a live connection. This
is an HTTP lifecycle addition to record 0032, not a change to VM budgets.

Gate 6 is the proof, from the fixture's side, not rnx's: the fixture reads
end-of-file on the socket it accepted, after the cancel and after the
reset. A subsequent request succeeding proves the client works; it does not
prove the old socket closed, and the gate does not confuse the two.

**The one thing a deadline cannot end is a name lookup already in the C
library.** Implementation stopped on this, correctly, and it is decided
here rather than worked around. The selected default reqwest resolver uses `getaddrinfo` on a blocking thread
(hyper-util's `GaiResolver`, `spawn_blocking`), and a blocking task cannot
be aborted once it has started — tokio says so, and says that dropping a
runtime waits for it. Codex's probe (rnx-bench `probes/http-dns-shutdown`,
a controlled two-second lookup in place of the system one) shows the
shape exactly: the request's deadline returns at 51 ms, the async task
count is zero at 63 ms, and dropping the runtime takes 2 001 ms. The
script's bound holds. What does not is the process's exit, and a thread.

Three ways out were weighed. Going lower than reqwest changes nothing,
because the blocking call is the C library's, not reqwest's. An async
resolver — hickory, which reqwest can take through the same
`dns_resolver` hook the fixture uses — is cancellable, but it reads
`resolv.conf` and `hosts` itself and does not go through `nsswitch`, so a
name that depends on a corporate NSS module or another native lookup
backend may behave differently. Some VPN configurations use an ordinary
DNS stub and would still work. This record keeps native name-service
integration rather than claiming those configurations are equivalent. The third way is to state the bound that actually exists: **a lookup
in flight ends when the OS resolver returns, or the process exits**.
DNS retry settings do not establish a universal deadline for every native
name-service backend. rnx promises not to wait for this work at runtime
shutdown; it does not promise to cancel it. So:

- `run` and `eval` end their runtime with `shutdown_background`, tokio's
  documented way to stop without waiting for blocking work. The process
  then exits, and exit ends the thread; nothing outlives it. A `run`
  whose lookup is stalled exits when its deadline says, not when the
  resolver does.
- A session's lookup thread, after `Ctrl-C`, stays in tokio's blocking
  pool until the resolver returns, invisible to the alive-task count and
  to the fixture, and then ends. The lookup can retain OS-managed
  resources, including resolver sockets; it is not an HTTP connection.
  Repeated stalled lookups can leave more than one such operation running. This is stated, in the documentation and
  here, as the exception to decision 3's zero: zero *async* tasks, and
  at most the lookups the OS has not yet given up on.
- The resolver stays the system's, preserving native name-service
  integration. An async resolver is a later option for whoever needs the
  bound more than the compatibility, and it plugs into the same hook.

Gate 10 holds this to a number.

The client is built with: `rustls-tls`, which is rustls with the bundled
`webpki-roots` and the `ring` provider, so a release binary trusts the same
roots on every platform and links no system TLS — rnx-bench confirmed the
probe binary links only libc, libm and libgcc; `gzip`; and reqwest's
redirect policy, decision 6. Not `json` — decision 4 — and not `charset` —
decision 4 again — and not `http2`, which this record neither needs nor
measures. reqwest reads `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and
`NO_PROXY`, in either case, from the environment by default and this
record leaves that on, stated: a script
behind a proxy works the way `curl` does, and a script that must not use
one is a later option.

### 4. Text is UTF-8 or refused; JSON is record 0033's reader or nothing

`body` on the text form is the response's bytes, decoded as UTF-8 or
refused naming the byte and pointing to the bytes form — the words
`host::process` uses for a child's output, adapted: "cannot get {url}: its
body is not UTF-8 at byte N; use http::get_bytes to read it". No charset
transcoding: a `Content-Type` naming `windows-1252` does not make rnx
decode windows-1252, because a silent transcoding is the same class of
surprise as a silent lossy replacement, and a script that needs it has the
bytes and can say so. reqwest's `charset` feature stays off.

There is no `response.json()`. A script that wants JSON writes
`host::json_parse(http::get(url).await?.body)?`, and gets record 0033's
contract — unsigned integers exact, both depth bounds, a position in the
document — because that is the only reader in the process. A request body
that is a value goes out through `host::json_stringify`, record 0019's
guarded writer, for the same reason. This record adds no JSON function of
its own and installs none; guardrail 2 makes that a stop condition.

### 5. Headers in, headers out

Out: `options.headers` is an object of string to string. A name or value
that is not a valid header — reqwest refuses control characters and
non-visible-ASCII names — is refused by name before anything is sent. rnx
sets a `User-Agent` of `rnx/<version>` unless the script sets one, so a
server log says what asked. reqwest still adds protocol headers such as
Host and Accept-Encoding as needed.

In: `headers` is an object with lowercase names, because HTTP names are
case-insensitive and a script should be able to write `headers["content-
type"]` without guessing the server's capitalisation. These are reqwest's decoded-response headers: gzip decoding removes the
wire Content-Encoding and Content-Length. Each remaining name maps to a
**list** of values, in the order received, one entry per occurrence. That
is the only representation that loses nothing: RFC 9110 §5.3 lets a sender
split a list-valued field across lines, and `set-cookie` is the field it
names as one that must not be joined. So `headers["content-type"][0]` is
the common case, and a script that wants every `set-cookie` has them,
intact, even though this record stores and sends no cookies. A value that
is not visible ASCII is refused, naming the header, as the text body is.
`HEAD` returns headers and an empty body.

### 6. Redirects are followed, ten deep, credentials stripped, and the final URL is reported

reqwest's default policy: up to 10 redirects, then refused as too many.
On a redirect to a different host reqwest removes `Authorization`,
`Cookie` and `Proxy-Authorization` before following — verified in
`redirect.rs` of 0.12.28 — so a bearer token a script set for one origin
does not travel to another. Both facts are stated rather than trusted:
gate 5 has a fixture that redirects across a port and asserts the header
did not arrive. Because the body a script receives may come from a URL it
did not name, `url` in the result says which; a script that must not be
redirected compares it.

### 7. What this record does not decide

- `POST`, `PUT`, `PATCH`, `DELETE` are reachable through `request` and
  gated only as far as the fixture sends a body and reads it back. Their
  own semantics — idempotency, what a `Location` on `201` means — are not
  this record's.
- Streaming a body larger than the limit, a body to a file, or a response
  a script reads in pieces. Those are what a `Response` type is for, and
  it comes back when a use case needs it, with its own bound on how long
  a script may hold a connection.
- Cookies, authentication helpers, multipart, compression other than
  gzip, HTTP/2, and any way to turn TLS verification off. The last is a
  decision to be made in the open when someone needs it, not a builder
  method waiting to be found.
- A disabled network. A build or a flag that refuses every request is a
  sandboxing decision and belongs with the rest of rnx's sandboxing, which
  has not been designed.

## Acceptance gates

Every network gate runs against a fixture server in the test harness,
bound to `127.0.0.1` on a port the OS chooses, never a public service, so
the suite passes with no network and no DNS. It runs under `test-support`,
as record 0032's `host::test_pending` does, because it is a test fixture.

1. **A round trip.** `http::get` of a fixture path returns `200`, the
   headers the fixture set with lowercase names, and the body it sent; the
   same through `get_bytes`; `HEAD` returns the headers and an empty body;
   `request("POST", url, #{body: ...})` delivers the body and the fixture
   echoes it back. All from `run`, `eval` and a session.
2. **The JSON path is record 0033's.** The fixture serves
   `{"n":18446744073709551615}`; `host::json_parse(body)?` gives an
   unsigned integer that `json_stringify` writes back exactly. A body of
   `[` nested 128 deep is refused with a position in the document. There
   is no other way to get a value from a body, asserted by a gate that
   lists the module's registered names.
3. **A status is a result.** `404` and `500` return `Ok` with `status`
   set. A connection refused, a name that does not resolve, a URL that
   does not parse, a TLS handshake against a plain-text port, and a
   certificate the bundled roots do not trust each return `Err` whose
   message starts "cannot get {url}:" and names which of those it was.
   The message never contains the body. The unresolvable name is
   **injected**, not looked up: under `test-support` the client takes a
   resolver that refuses one fixture name, so the gate does not depend on
   the machine having, or lacking, a network. The plain-text port proves
   only that a protocol failure is named; certificate verification is
   proved by the fixture serving TLS with a self-signed certificate and
   rnx refusing it, so that a build that quietly accepted any certificate
   would fail here and nowhere else.
4. **One deadline, one limit.** Headers that arrive after the deadline,
   and a body whose second chunk arrives after the deadline though its
   headers were prompt, are both refused as the deadline. The refusal is
   timed from the fixture's side and asserted within a tolerance the gate
   states — the deadline plus the driver's cadence plus a scheduling
   allowance, 250 ms on the reference machine, named in the evidence, not
   the prose — and never against the fixture's own timeout, which is set
   ten times longer so that the wrong clock cannot pass the gate. A
   chunked body with no `Content-Length` that exceeds the limit is refused
   as the limit; a gzip body under the limit on the wire and over it
   decoded is refused as the limit. A `Content-Length` of 1 GiB followed
   by ten bytes and a close is refused as incomplete, naming both counts,
   and the allocation figure shows nothing was reserved for the claim; the
   same header followed by nothing is refused as the deadline. `timeout_ms`
   of `0` and of `90001`, and `body_limit` of `0`, are refused before
   anything is sent, in `host::process`'s words for the deadline. An
   unknown key in `options` is refused by name.
5. **Redirects.** A chain of three is followed and `url` names the last.
   Eleven are refused as too many. A redirect from one fixture port to
   another with an `Authorization` header set arrives at the second
   without it, asserted by the fixture. A `set-cookie` sent twice arrives
   as a two-element list.
6. **Cancellation, then closure, then recovery.** In a session, a
   successful request first, so the pool holds a healthy connection; then
   `Ctrl-C` during a request to a second fixture port that will never
   answer ends the input as "interrupted", per record 0032, within the
   same stated tolerance as gate 4; before the next prompt both fixtures
   read end-of-file on the sockets they accepted — the cancelled one and
   the healthy one, since the client is discarded. The next input makes a
   request that succeeds on a newly accepted socket. Every end-of-file in
   this gate is `Ok(0)` on the fixture's read, told apart from a read
   error, so a reset connection cannot pass as a closed one.
   After `:reset`, the fixture reads end-of-file on the idle connection
   the pool was holding, and `:memory` is at the reset baseline, so the
   client did not survive it. In both cases the runtime reports no task
   alive before the prompt returns. Each closure is observed by the
   fixture, not inferred from the next request working.
7. **Text is UTF-8 or refused.** A body containing byte `0xFF` is refused
   by the text form naming the byte and `http::get_bytes`, and delivered
   by the bytes form. A body served as `windows-1252` is not transcoded:
   the bytes form returns the bytes, and the text form refuses.
8. **Reuse is established by the fixture, and timed for the record.** Two
   sequential requests in one session to the same fixture arrive on one
   accepted socket, asserted by the fixture's count. The two durations
   are reported in the evidence, as measured, with no claim about what
   the difference is.
9. **Nothing depends on the machine.** Every fixture gate runs rnx with
   `HTTP_PROXY`, `HTTPS_PROXY`, `ALL_PROXY` and `NO_PROXY` cleared from
   the environment, in upper and lower case both, so a developer's proxy
   cannot route a loopback request elsewhere. A gate that drives a
   session through pipes sets `TERM` to `dumb`, because that is the one
   condition under which the line editor writes its prompt to a pipe;
   review found the session gates passing only where the shell already
   set it. Proxy behaviour has its own gate: a second fixture
   acting as a proxy, `HTTP_PROXY` set to it, and the request observed
   arriving there and not at the origin.
10. **A stalled lookup does not stall the exit.** Under `test-support` the
   resolver can be told to stall one fixture name for two seconds. A
   `run` that requests that name with a 100 ms deadline reports the
   deadline and exits within the deadline plus the stated tolerance,
   asserted from outside the process; a session reports the deadline,
   shows a prompt within the same tolerance, and the next input runs.
   Nothing here resolves a real name.
11. **Existing guarantees.** Both suites unchanged. `version`, `help`,
   `eval 42`, the bare run and the JSON workload measured before and after
   in rnx-bench, with the reqwest-only binary size and the context time
   reported against record 0031's eight-module figures. Record 0032 as
   fixed builds the runtime for every file and every session; adding
   `net` to it is a cost the bare run measures, and `version` and `help`
   still never build one. Record 0029's notices workflow re-run for the
   new tree: reqwest, hyper, rustls, ring and webpki-roots each carry a
   licence that is listed.

## Guardrails and stop conditions

1. If a bound cannot be enforced — if reqwest's timeout turns out not to
   cover the body read, or the decoded count cannot be observed per chunk —
   stop and say so; do not ship a bound that holds only for cooperative
   servers.
2. One reader, one writer. If the implementation finds itself wanting a
   JSON function on the response, stop: that is decision 4 being undone.
3. No type reaches a script that holds a connection open past the call.
4. If the runtime change for `net` alters record 0032's measurements for
   the bare run or the session baseline beyond noise, stop and measure
   before continuing.
5. No public network in any gate. A gate that needs a real DNS name is
   a gate that fails on a train, and this suite does not.
6. If the runtime cannot be driven to closure after a reset — if a
   dispatcher task outlives the bounded turn in decision 3 — that is a
   finding, not a tolerance; stop and report it before the gate is
   softened. A name lookup in the C library is the stated exception, and
   decision 3 states its lifetime and the non-waiting shutdown guarantee;
   nothing else is exempt.

## Risks

- **rustls with bundled roots ages.** A root added after the last
  `cargo update` is unknown to a stale binary. Stated in the README beside
  the TLS claim; the alternative, native roots, costs a directory read at
  first use and differs by platform, and is a later option, not a default.
- **The proxy environment is trusted.** A poisoned `HTTPS_PROXY` redirects
  every request. That is `curl`'s exposure too, and a script runs as the
  user who set the variable; stated, not fixed.
- **Headers are lists, so the common case has an index in it.**
  `headers["content-type"][0]` is one more character than a script
  would like. That is the price of losing nothing, and a helper is a
  later nicety, not a reason to join.
- **The limit is 8 MiB by default and 64 MiB at most.** A script that
  needs a larger download needs the streaming form decision 7 withholds.
  That is a deliberate ceiling: an unbounded body is an unbounded value
  in one process, and record 0005 exists so that never happens quietly.

## Forward

The remaining verbs' semantics, a `Response` type with a held-connection
bound, and the filesystem breadth of record 0031's gate 5, each on its own
record. The upstream draft for the unsigned deserializer gains a second
demonstration: an HTTP body, through the companion module, wraps the same
number.


## Initial implementation finding: the default DNS resolver escaped the shutdown bound

The initial implementation passes the loopback gates, including cancelling a
retained Rune future beside a healthy pooled connection, but it does not
satisfy the whole-operation deadline for the default system DNS resolver.
Hyper's GaiResolver runs getaddrinfo through Tokio spawn_blocking. Aborting
its handle cannot cancel a blocking call that has started, and runtime drop
waits for it. num_alive_tasks reports async tasks; zero is not proof that
blocking DNS has finished.

The deterministic rnx-bench probe at probes/http-dns-shutdown models that
same started blocking lookup and abort-on-drop handle, with a two-second
controlled delay instead of system DNS. No public network is used. The
50 ms request timeout returns at 51.5 ms and async task count is zero by
62.9 ms, but runtime drop finishes only at 2.001 seconds. Raw output is in
results/http_0034_dns_shutdown.txt. Source attribution and the other gates
are in [the implementation evidence](0034_a_bounded_request_with_no_second_json_evidence.md).

This triggered guardrail 1 in the initial implementation. Decision 3 now
accepts the lingering system lookup explicitly, preserves the native
resolver, and uses non-waiting runtime shutdown. Gate 10 verifies that
request completion and process exit remain bounded. The change closes the
exit-waiting defect; it does not establish cancellation of system DNS.
