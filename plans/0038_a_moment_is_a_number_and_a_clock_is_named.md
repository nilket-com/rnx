# rnx 0038: a moment is a number, and a clock is named

Status: implemented 2026-09-14 after review and the fixture correction below.
Evidence: `0038_a_moment_is_a_number_and_a_clock_is_named_evidence.md`. The
thirty-eighth record of rnx, and the
batteries record 0031 called time and dates. It gives a script two clocks
with distinct origins and documented meanings, a calendar that knows what a time
zone is, and a sleep that the async driver can end. It chooses one
dependency by measurement, and it keeps every moment a script holds as
the integer record 0035 already chose for a file's modification time.

## Context

A script that logs, polls, ages files, or reports needs five things, and
each is a real script rather than a guess:

1. *"Stamp each log line."* The wall-clock now, written as a string a
   person and another program both read: `2026-09-14T06:26:54.429Z`.
2. *"How long did that take?"* A stopwatch that cannot go backwards when
   the wall clock is adjusted, subtracted from itself, in milliseconds.
3. *"Files modified in the last week."* `fs::metadata(p).modified_ms`
   compared with now, so `modified_ms` must be directly usable: same unit,
   same epoch, same sign convention, negatives included.
4. *"Show it in local time, or in the office's time."* The same moment as
   a calendar in a named zone, with the offset that zone had at that
   moment, daylight time included, and back again from fields.
5. *"Wait half a second and try again."* A sleep that Ctrl-C ends.

Rust's standard library gives the first two clocks and nothing of the
calendar: `SystemTime` and `Instant`, no zones, no formatting, no parsing.
Upstream `rune-modules` 0.14.2's `time` is tokio's `Duration`, `Instant`,
`sleep` and `interval` — monotonic and async only, no wall clock, no
calendar. So a dependency is needed, and three were measured on
2026-09-14 (rnx-bench `probes/time-candidates`, one small binary each,
release, offline; what they exercise is stated under the table):

| crate | standalone binary | build | local time | a named zone, `Europe/Paris`, on a Chicago machine |
| --- | --- | --- | --- | --- |
| `jiff` 0.2.24 | 0.92 MiB | 2.7 s | the system's zone, from zoneinfo | from the system's zoneinfo, in the crate |
| `chrono` 0.4.44 | 0.56 MiB | 1.7 s | the system's zone, from zoneinfo | a second crate: `chrono-tz` (embedded database) or `tzfile` (system zoneinfo) |
| `time` 0.3.47 | 0.47 MiB | 2.1 s | an offset, through the C library | **not available**: no zone names at all |

What the probes measured is narrower than the five examples: each
binary does the wall-clock now as RFC 3339, a negative timestamp, local
time under `TZ`, one parse, and the overflow refusal. They do not
exercise a stopwatch, file ageing or a sleep, which every candidate can
do and which is not what separates them; and their sizes are standalone
executables, not the delta an integration adds to rnx, which gate 7
measures. What separates them is example 4, and the evidence for that
column is the crates' source, read on the same day: `time` obtains the
local offset from `localtime_r` and has no notion of a named zone, so a
moment "in the office's zone" cannot be expressed; `chrono` reads
`/usr/share/zoneinfo` for the local zone and re-resolves the offset after
arithmetic, which is correct, and for any *other* name needs a second
crate — `chrono-tz`, whose database is compiled in and ages with the
binary, or `tzfile`, which reads the system's; `jiff` does it in one
crate, reads the system's zoneinfo for every name — this machine's
`/usr/share/zoneinfo` is 2.0 MiB and `/etc/localtime` names
`America/Chicago` — carries the zone in the value, and is modelled on the
Temporal proposal, the most carefully specified calendar API in common
use. That is a win on integration simplicity, not a disqualification of
`chrono`, and it costs 0.4 MiB more than the cheapest standalone. An
earlier draft of this record disqualified the other two on grounds that
turned out to be false for these versions; the table above is what the
source says.

## Decision

### 1. A moment is an `i64` of milliseconds since the epoch, UTC, signed

Every wall-clock moment a script holds is the integer record 0035 chose
for `modified_ms`: milliseconds since 1970-01-01T00:00:00Z, negative
before it, floor-rounded. No native value, for the reason record 0037
gave — five contracts would have to be assessed for a type — and for a
better one: an integer already compares, subtracts, sorts, serialises to
JSON, renders, and inspects, with nothing to add. `now_ms() -
metadata.modified_ms` is the age of a file in milliseconds, with no
conversion between them, which is the point of example 3.

Precision is milliseconds, everywhere, stated: the wall clock is read at
nanosecond precision and **floored** to the millisecond; parsing a
string with more precision floors too, so `1969-12-31T23:59:59.999999999Z`
is −1, not 0. That is not what jiff's `as_millisecond` gives — measured,
it rounds that input to 0 — so rnx computes the floor itself from the
whole seconds and the signed sub-second nanoseconds, which jiff exposes,
and the conversion is one function in one place with a unit test on the
pre-epoch fractional case. `rfc3339` shows exactly three fractional
digits; `format` shows what its pattern asks for.

Range is jiff's: years −9999 through 9999. A millisecond count outside it
— below −377 705 023 201 000 or above 253 402 207 200 999 — is refused
by every function that takes one, naming the number, rather than
wrapping or clamping. The constructor is jiff's `Timestamp::new` from
whole seconds and nanoseconds, not `from_millisecond`, which — measured —
stops at the last whole second and refuses the final 999 milliseconds of
the range while `new` accepts them;
the adapter in each direction is stated so that the advertised range
and the accepted one are the same. `i64::MAX` milliseconds is about 292
million years and is refused.

### 2. The other clock is a different function with a different origin

```
time::now_ms()        -> i64   milliseconds since the epoch, wall clock
time::monotonic_ms()  -> i64   milliseconds since an origin this process chose
```

`monotonic_ms` is `Instant`, as an integer counted from the first call in
the process. It never decreases, it is unaffected by the wall clock being
set, and its origin means nothing outside the process — it is for
subtracting from itself and nothing else. The two are separate functions
with separate names and separately documented meanings. That is a
distinction in meaning and name, **not one the API enforces**: both are
integers, and a script that passes a stopwatch reading to `parts` gets a
date in early 1970 and no error, because an integer cannot say which
clock it came from. The record chose integers with that cost known
(decision 1), and the README says beside `monotonic_ms` what it is not.
Example 2 is `let t = time::monotonic_ms(); ...; time::monotonic_ms() - t`.

### 3. A zone is named, and `"local"` is a name that can fail

Every function that turns a moment into a calendar, or a calendar into a
moment, takes a zone as a string:

- `"UTC"`;
- an IANA name, `"Europe/Paris"`, `"America/Chicago"`, from the system's
  zoneinfo on Unix and from a bundled copy on Windows (decision 7);
- a fixed offset, `"+02:00"`, `"-05:30"`, for a script that has one and
  no name;
- `"local"`, the zone the process runs in: `TZ` if set, else the
  system's configuration.

A name the database does not have is refused naming it. `"local"` is
refused, naming what was consulted, when the zone cannot be determined —
jiff's `try_system`, not its silent fallback to UTC — because a script
that asked for local time and got UTC without being told would report
the wrong hour to whoever reads its output. On a machine with no
zoneinfo, `"UTC"` and fixed offsets still work and every name is
refused; the record does not bundle a database on Unix to paper over a
system without one.

### 4. Between a moment and a calendar

```
time::format(ms, pattern, zone)  -> Result<String>
time::parse(text)                -> Result<i64>
time::parts(ms, zone)            -> Result<Object>
time::from_parts(object, zone)   -> Result<i64>
```

`format` is strftime, jiff's dialect: `%Y-%m-%d`, `%H:%M:%S`, `%.3f` for
the milliseconds, `%:z` for the offset, `%Z` for the abbreviation, `%:Q`
for the zone name. It shows what the pattern asks for and promises no
particular precision; a pattern the dialect does not accept is refused
naming the directive, through jiff's fallible formatting call, never
through a `Display` that could panic.

`rfc3339(ms, zone)` is **strict RFC 3339**, and says so in its name:
`YYYY-MM-DDTHH:MM:SS.sss` followed by `Z` for UTC or `±HH:MM` for any
other zone, exactly three fractional digits. RFC 3339 cannot write every
moment in decision 1's range, and the function refuses rather than
bends: a year outside `0000`–`9999` is refused naming the year — the
range's low end, measured, would otherwise print as `-9999-01-02…`,
which no RFC 3339 parser reads back, while the high end,
`9999-12-30T22:00:00.999Z`, fits and round-trips — and an offset is
written only when it is a whole number of minutes with an hour part of
at most 23, `±HH:MM`; one that is not is refused naming it, which
historical zones produce: Paris in 1900 is `+00:09:21`. The same offset
grammar is what `parse` accepts as a fixed offset, and what the zone
argument accepts as one. A script that needs one of those moments
as text has `format`, whose output is what its pattern says and not a
standard's promise.

`parse` reads one thing: an RFC 3339 / ISO 8601 date-time **with an
offset or a `Z`**, such as `2026-09-14T12:00:00Z` or
`2026-09-14T12:00:00.5+02:00`. It returns the moment. A string with no
offset is refused, because a date-time without one is not a moment — it
is a calendar reading that needs a zone to become one, which is what
`from_parts` is for. Two things jiff's timestamp parser accepts silently
are decided here rather than inherited, both measured:

- A bracketed annotation, `[Europe/Paris]`, is **validated**: the name
  must be in the database, and a numeric offset written before it must
  agree with that zone at that moment, else refused naming the
  disagreement. jiff's `Timestamp` parser accepts `[Made/Up]` and an
  offset that contradicts the zone; its `Zoned` parser with conflict
  rejection refuses both, and that is the path rnx takes when a bracket
  is present. One case is different by design and rnx adopts jiff's
  documented distinction rather than adding a check: `Z` before an
  annotation — `2026-07-01T12:00:00Z[Europe/Paris]` — is accepted, and
  means the exact instant `12:00Z`, with the zone naming only how it is
  displayed, `14:00+02:00`. That is the Temporal rule `Z` follows: it
  says "this instant, offset unknown", where `+00:00` says "this is the
  zone's offset", which Paris in July contradicts. Measured: the first
  parses and the second is refused. `parse` returns the instant either
  way, so nothing is lost by accepting `Z` here. Any annotation that is
  not a time zone is refused naming it.
- A leap second, `23:59:60`, is **refused**. jiff turns it into
  `23:59:59` without saying so; a moment in rnx has no leap seconds, and
  a script that reads one from a log gets a sentence, not a moment one
  second early.

A string that parses but names a moment outside the range is refused
naming the range.

`parts` gives the calendar reading of a moment in a zone:

```
#{ year, month, day, hour, minute, second, millisecond,
   weekday,            1 = Monday … 7 = Sunday, ISO
   offset_seconds,     the zone's offset at that moment, signed
   zone }              the name resolved, "Europe/Paris", "UTC", "+02:00"
```

`from_parts` goes back: an object with at least `year`, `month`, `day`,
the rest defaulting to zero, in a zone, to a moment. Two local readings
are not one moment: in a zone's autumn fold `02:30` happens twice, and in
its spring gap it does not happen at all. jiff can pick for a caller —
earlier, later, or the "compatible" choice — and this record does not
let it: **an ambiguous or nonexistent reading is refused**, naming the
reading, the zone, and which of the two it was. The two are resolved
differently, because they are different things. A **fold** is resolved
by `offset_seconds` in the object: `+02:00` or `+01:00` picks the
occurrence, and jiff confirms the offset is one the zone had. A **gap**
is not resolved by anything: `2026-03-29T02:30` in Paris did not happen,
and — measured — jiff refuses it with `+01:00` and with `+02:00` alike,
because no offset makes a reading that does not exist into one that
does. The refusal for a gap says so and does not suggest an offset. On
any date, an `offset_seconds` that disagrees with the zone at that
reading — `+01:00` on a July noon in Paris — is refused naming both,
rather than trusted over the zone. Fields out of range — month 13,
second 60 — are refused naming the field.

### 5. `sleep` is async, and the driver ends it

```
time::sleep(ms).await  -> Result<()>
```

Tokio's sleep, on record 0032's runtime, so an input that **awaits** a
sleep is an input the driver runs and Ctrl-C ends within one cadence, as
it ends any host future. There is no synchronous sleep: a blocking one
could not be interrupted, which is the constraint decision 7 of record
0035 states for file calls, and here there is no reason to accept it.
What "async only" means is stated precisely, because Rune lets
synchronous code *call* an async function and hold the future it
returns without awaiting it — measured on the current release. Creating
the future is inert: no time passes and nothing sleeps until it is
awaited, and awaiting is what needs an async input. So `sleep` is
`sleep(ms).await -> Result<()>`, and the `Result` is where a negative
count goes: refused, catchable, naming the count. The accepted range is
`0` through `68 719 476 735` — 2^36 − 1 milliseconds, about 2.2 years —
and a count above it is refused naming the bound. The bound is this
record's choice, not tokio's: tokio's wheel spans that many milliseconds
and wraps a later deadline around it, still firing it on time, so
nothing would clamp; the limit is chosen because a sleep longer than two
years is a mistake in a script and because it keeps every sleep within
one cycle of the wheel, which is easier to reason about than the wrap.

### 6. What this record does not decide

- Durations as values, intervals, timers that fire, and formatting a
  span as "3 days".
- Calendar arithmetic — add a month, the last day of February — which
  jiff does well and which waits for a script that needs it.
- Microsecond or nanosecond precision.
- Windows execution, as every record since 0025 has said.

### 7. The dependency, its features, and its database

`jiff` 0.2.24, `default-features = false`, with `std`, `tz-system` and
`tzdb-zoneinfo` on every platform, and `tzdb-bundle-platform` — which
pulls `jiff-tzdb`, a compiled copy of the IANA database — on Windows
only, where there is no zoneinfo to read. The bundled copy ages with the
binary, stated in the README beside the TLS-roots note record 0034 made
for the same reason. Not `jiff-static`, not `serde`, not `logging`.
Record 0029's notices workflow runs for the new tree.

## Acceptance gates

Every gate that touches the local zone sets `TZ` explicitly in the
child's environment, so the developer's machine decides nothing; the
`"local"` refusal is provoked by `TZ` naming a zone that does not exist
and by a valid UTC-only zoneinfo directory pointed to through jiff's
`TZDIR`, with `TZ=Europe/Paris`. An empty directory is not a missing-database
fixture: Jiff rejects it and searches the default system directories. This
was measured during implementation; the fixture must lack the requested name
without triggering database discovery fallback.

1. **The five examples run**, as scripts in the evidence: a stamped log
   line, a measured elapsed, files aged against `modified_ms` including
   a file whose time was set before 1970, a moment shown in
   `America/Chicago` and `Europe/Paris` with the two different offsets
   on a date in July and again in January, and a sleep ended by Ctrl-C.
2. **The clocks are distinct in meaning, and documented as such.**
   `monotonic_ms` never decreases across ten thousand calls and starts
   near zero; `now_ms` agrees with the process's own `SystemTime` within
   a stated tolerance; `:help` for each says which clock it is and what
   it must not be used for. No gate claims the API prevents mixing them,
   because it does not.
3. **A moment survives the round trip, in the domain each function
   covers.** For moments at the epoch, before it, at the range's two
   ends, and at the fold and the gap in `Europe/Paris`:
   `from_parts(parts(ms, z), z) == ms` for every `z` including `"local"`
   with `TZ` set, with `offset_seconds` carried through so the fold
   resolves; `parse(rfc3339(ms, z)) == ms` for `"UTC"` and `"+02:00"` on
   every moment within RFC 3339's domain, the range's maximum among
   them, while `rfc3339` refuses the range's minimum naming the year,
   refuses Paris in 1900 naming the offset `+00:09:21`, and refuses a
   fixed-offset zone of `"+24:00"`, while `format` with `%:z` writes the
   first two; and `parts`
   reports the offset the zone had at that moment, `+01:00` in January
   and `+02:00` in July.
4. **Refusals name what they refuse.** A zone the database lacks;
   `"local"` when it cannot be determined; a millisecond count one past
   either end of the range, and the two ends themselves accepted; a
   `parse` input with no offset, with a leap second `23:59:60Z`, with an
   unknown annotation `[Made/Up]`, and with an annotation whose numeric
   offset disagrees, `+05:00[Europe/Paris]`, while `Z[Europe/Paris]` is
   accepted as `12:00Z`; a `format` pattern with an unknown
   directive; a `from_parts` reading in the fold without an offset, in
   the gap with and without either offset, with an offset that disagrees
   on a July noon, with month 13, and with `second` 60 — each is `Err`
   naming the thing; the fold succeeds with either of its two offsets
   and the gap succeeds with none.
5. **Precision is milliseconds and rounding is floor.** Parsing
   `…:00.999999Z` gives the same moment as `…:00.999Z`; parsing
   `1969-12-31T23:59:59.999999999Z` gives `-1`, the conversion's unit
   test pins that against jiff's own `as_millisecond`, and a file whose
   time is set to that instant reports `modified_ms` `-1` through record
   0035, so the two floors agree.
6. **`sleep` is the driver's.** `sleep(50).await` in `run`, `eval` and a
   session takes at least 50 ms and returns `Ok(())`; `sleep(10000)`
   awaited under Ctrl-C ends as "interrupted" within record 0032's
   cadence tolerance. An `eval` or session input that awaits a sleep is
   *promoted* to the async wrapper, as record 0032 arranged, and the gate
   observes that through `:debug` showing `pub async fn main`, not
   through a refusal, because there is none to see. The refusal exists
   only for an explicitly synchronous function — `fn f() {
   time::sleep(1).await }` — and is gated separately with the
   compiler's wording. Creating the future is inert: `let f =
   time::sleep(10000);` in a synchronous input returns within a generous
   bound stated in the evidence, well under ten seconds, so the gate
   cannot mistake process noise for a sleep. `sleep(-1).await` and
   `sleep(68719476736).await` are catchable refusals naming the count and
   the bound.
7. **Existing guarantees.** Both suites, the startup measurements and the
   session baseline before and after, the binary size delta reported
   against the probe's 0.92 MiB, and the notices check.

## Guardrails and stop conditions

1. Milliseconds since the epoch is the only representation of a moment
   any function takes or returns. If a function wants a second one, stop.
2. No silent choice at a fold or a gap, no offset trusted over the zone,
   no leap second folded into the previous one, no annotation accepted
   unchecked, and no silent UTC when `"local"` was asked for.
3. No synchronous sleep, and no sleep the driver cannot end.
4. One conversion between milliseconds and jiff's timestamp, in one
   place, floor in both directions, unit-tested at the pre-epoch
   fractional case and both range ends. Formatting goes through jiff's
   fallible API; no `to_string` on a formatter that can fail.
5. If `jiff` cannot be built with the feature set in decision 7 on a
   platform rnx builds for, stop and say which feature and why; do not
   enable the bundled database on Unix to get past it.

## Risks

- **0.4 MiB more than the cheapest standalone probe.** Measured as
  standalone sizes; what it buys is one crate that names any zone from
  the system's database, not a correctness the others lack.
- **The Windows bundled database ages with the binary.** Stated; the
  Unix build reads the system's, which the system updates.
- **A script that wants "3 days ago" has to subtract milliseconds.**
  Durations are decision 6's deferral; the subtraction is one line.
- **Ambiguous local readings refuse where other tools guess.** A script
  building a moment from local fields at 02:30 on the fold day gets an
  error instead of an answer, and one on the gap day gets an error that
  no offset can clear. That is the design, and the error says which.
- **Two integers, two meanings, one type.** A stopwatch reading passed
  as a moment is a wrong answer with no refusal. Decision 2 states it;
  the alternative was a type, and decision 1 says why not.

## Forward

Durations and calendar arithmetic when a script needs them; then the
child-environment option on `host::process`, which is the last item
record 0031 named.
