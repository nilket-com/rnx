# rnx 0166: bounded web helpers

**Status:** plan, for Claude's review before implementation.

The user wants terse Rune HTTP handlers and has confirmed the order: finish the
deployment cleanup, implement 0166 web helpers, then 0167 source formatting.
This record is generic rnx/Rune tooling. Application changes and deployment
belong to the application team and its repository. No application-specific
names or content are added to rnx.

## 1. Problem and measure

0165 removed hand-written route dispatch, but the retained HTTP example still
implements HTML escaping, percent decoding, URL-encoded forms and response
objects in Rune. These are repeated protocol mechanics rather than page logic.
Replace those helpers with a small built-in `web` module, leaving the example's
routes, content, status codes, headers and form semantics unchanged.

The measure is the unchanged 26 wire fixtures and three complete responses on
one keep-alive connection, plus a replayable before/after source census. Report
helper lines separately from content, route declarations and total lines.
This is another terseness cut, not a claim of Flask equivalence or axum speed.

## 2. Closed API

The parsing/response functions return Rune `Result<T, String>`; escape_html
returns a String directly and raises a VM error on overflow. All appear in the host catalogue with
their contracts and limits, and work in run, eval, sessions, notebooks and
server contexts. They require no adapter, HTTP-server feature, network or VM
callback. No state or lifecycle resource is added.

- `web::escape_html(text) -> String`: replace `&`, `<`, `>`, double
  quote and apostrophe with `&amp;`, `&lt;`, `&gt;`, `&quot;`, `&#39;`,
  respectively. Its cap failure raises a VM error before allocating; non-string
  arguments fail Rune's typed conversion. This inline template operation does
  not require Result plumbing. Preserve all other UTF-8 bytes, including newlines. Existing
  entities are escaped again. This is escaping for HTML text and quoted
  attributes, not URL validation, unquoted attributes, JavaScript or CSS.
- `web::decode_component(text) -> Result<String>`: decode valid `%HH` octets
  exactly once, preserve `+`, and refuse a result that is not UTF-8. Preserve
  an incomplete or non-hex percent escape literally, matching the existing
  example; this permissiveness is documented rather than called strict URL
  validation. Do not use this helper in 0165's route matcher, whose strict
  percent/UTF-8 and encoded-slash rules remain unchanged. Route params are
  already decoded and must not be decoded again.
- `web::parse_form(body) -> Result<Object>`: borrow a String or Bytes; decode
  `application/x-www-form-urlencoded` text, converting literal `+` to space
  before percent decoding. Split on `&`, then the first `=`; absent `=` means
  an empty value. Empty input yields an empty object. Empty names/values and
  empty pairs are retained; the first occurrence of a decoded name wins.
  Decode and validate even discarded duplicate values. Invalid UTF-8 fails
  the entire parse. No content-type check, multipart parser or automatic
  request parsing: the caller chooses this parser explicitly.
- `web::response(status, body, content_type, headers) -> Result<Object>`:
  build the existing exact `#{status, headers, body}` response shape. Borrow
  body as String or Bytes, content_type as String, and headers as an Object
  whose values are String/Bytes scalars or vectors of String/Bytes. Empty
  vectors are accepted and emit no header value, matching the current host.
  The output always uses vectors, preserving String/Bytes for each body/value. Content type is inserted as
  one value; refuse any case-insensitive content-type key in extra headers.
  Normalize header names to lowercase, refuse case-insensitive duplicates,
  and preserve value order. Validate status, bodies and headers against the
  current host contract before copying. No escaping of an already-built body.
- `web::html(status, body) -> Result<Object>` and
  `web::text(status, body) -> Result<Object>`: the same constructor with empty
  extra headers and `text/html; charset=utf-8` or
  `text/plain; charset=utf-8`, respectively.

No redirects, cookies, JSON response helpers, joining, templates, static-file
serving, persistent state, client-IP trust or source formatter in this record.

The HTTP host also accepts one outer Rune Result from a handler or reserved
error hook: Ok(v) decodes v under the unchanged response rules, while Err(e)
takes the existing 500 path. Retain bounded error text in the private decoding
diagnostic and redact the value from public HTTP logs and wire bodies under
the existing policy. Bare responses still work, nested Results are not
recursively unwrapped, and routes() still requires its bare route table.
This lets a handler directly return web::html(...) without a trailing ?.

## 3. Bounds, ownership and validation

Each input and generated output string/body is at most 1 MiB. HTML escaping
first scans the borrowed text and computes the exact checked expanded byte
count; reject before reserving or copying if it exceeds 1 MiB. Component
decoding cannot expand bytes; validate size first and UTF-8 before publishing.

Forms allow at most 1,024 raw pairs, including duplicates and empty pairs;
count them without building a proportional table. In a first bounded scan,
validate every component's decoded UTF-8 and checked cumulative decoded bytes
(at most 1 MiB), including duplicates, before allocating owned keys/values or
the output object. A second pass builds it. No per-pair copy of the original
input and no allocation proportional to an over-limit collection. Table
metadata is bounded by the pair cap and reported separately from text bytes.

Responses preserve the current status range 200..599, empty bodies for 204/304,
1 MiB body cap, 64 header names and 64 total values, and 16 KiB header byte cap
with the existing 64-byte transport reserve. Count the generated content-type
name/value in those limits. Header grammar and reserved transport names follow
the current host exactly: content-length, transfer-encoding, connection,
keep-alive, upgrade, trailer, te and proxy-connection are refused. All arithmetic
is checked; name/value lengths and vector sizes are checked on borrowed data.
Validate the complete argument set before copying the body or header payload.
Errors identify the operation and field/index without echoing large inputs.

Use the already-present HTTP header types through reqwest's public re-exports,
not a new optional server dependency. Factor shared response validation rules
into a pure crate-private component usable with and without http-server, preserving the host's accepted inputs and wire output. The host
still validates arbitrary hand-built responses; constructors are convenience,
not a bypass. Do not introduce content-type restrictions absent from the host
merely because the field is named content_type.

Every supplied Rune string, byte buffer, header object/vector and body remains
usable after success and after a later-field refusal. Use public borrow guards;
do not convert owned Strings/Vecs out of script values. The result owns its
independent container and copied payload. No alias between input mutable
containers and output containers is promised or introduced.

## 4. Controls

Exercise native implementations and real Rune VM calls, including catalogue
registration in stock and server contexts and feature-off builds:

- exact escaping, entities, Unicode and control bytes, malicious markup,
  quoted-attribute examples and overflow-by-expansion;
- valid upper/lower hex, encoded Unicode, literal plus, malformed/incomplete
  percent escapes, double encoding, encoded slash, invalid decoded UTF-8;
- form plus versus `%2B`, repeated decoded keys, missing/empty values, empty
  input/pairs/names, first `=` only, invalid UTF-8 in a discarded duplicate;
- String/Bytes parity, caps at and above boundaries, wrong argument types,
  too many pairs before allocation, and cumulative byte accounting;
- response exact shape, html/text MIME strings, repeated values, lowercase
  normalization, case-insensitive duplicates and content-type conflict,
  invalid/reserved names, invalid values including CR/LF, status range,
  body-forbidden statuses, name/value/byte/body boundaries;
- caller inputs reused after success and early/late errors; mutate returned
  headers/body containers where applicable and show inputs remain unchanged;
- allocation counters start after inputs exist: oversized escaped text, form
  pair count and header vectors or a late invalid header with a large valid
  body refuse below 256 KiB additional allocation. Controls that bypass the
  relevant preflight must fail the allocation or correctness assertion.

Retain independent expected-value fixtures rather than validating solely by
the new implementation. Existing host response and route controls must pass,
including hand-built responses and explicit/implicit HEAD behavior. Ok-wrapped
responses match bare ones; Err logs a redacted failure and returns 500;
Ok(non-response) refuses. Test all reserved hooks and routes() Result refusal.

## 5. Evidence and regression checks

In rnx-bench, retain 0165's named-route example as the before case and port
only its manual helpers to the new module. Keep every page byte and the 26
fixture expectations unchanged; retain source and executable hashes, commands,
complete checker output and the line-census script/results. Count handler-side
tokens/characters and ? tokens before/after as well as physical lines. Keep malformed
percent and duplicate-form controls, rather than tightening semantics during
the port. Query parsing can use parse_form explicitly, but no new example
feature is needed to earn credit.

Run the core test-support suite, feature-off suite, relevant server/command
controls, formatting and touched-code clippy. Measure binary size and stock
launch against b4cc0a6 using 0068's existing method; a reproducible greater than
5% startup regression stops for review. For the same new binary, compare the
old-helper and new-helper examples in three interleaved runs per 0165's five
conditions, with the inherited fail-closed throughput/latency/error/artifact
checks (30 measured samples plus warmups). Report all medians/ranges. A
greater than 5% lower median throughput in any condition stops for review,
without weakening fixtures or tuning unrelated code.

After implementation acceptance, publish the generic rnx record under the
existing review procedure. The application team may then adopt these helpers
in a separate reviewed commit, checking its own fixtures before any deployment.
No production host or application repository changes are part of this record.

## 6. Plan acceptance

Claude accepted d10e095 with three amendments, incorporated above: one outer
Result is accepted for handler/hook responses with private error text and
public redaction; scalar extra header values normalize to vectors, including
the stated empty-vector behavior; and call-site token/character/? counts join
the line census. Response caps and validation come from one shared source.

## 7. Implementation review amendment

Escape HTML returns a plain String because it is used inside templates and its
only data-dependent failure is the size cap; a cap refusal raises a VM error.
This does not change preflight or the host's redacted 500 behavior. Parsing and
response constructors keep Result: malformed UTF-8, status/body constraints and
header validation are errors a caller may handle deliberately. HTML/text retain
that same response contract rather than varying their return type by body type.
String body/header values stay String and Bytes stay Bytes in independent
output. Constructor header errors name the operation, bounded escaped header
name and failing value index, without echoing the value.
