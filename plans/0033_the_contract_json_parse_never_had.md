# rnx 0033: the contract `json_parse` never had

Status: proposed 2026-09-13. The thirty-third record of rnx, and record
0031's gate 3. rnx has had `host::json_parse` since the spike: one line,
`serde_json::from_str` into a Rune value, with no contract of its own. This
record gives it one, and the first thing that contract has to fix is that
rnx cannot read back a number it wrote.

An earlier draft of this record was called "JSON that reads what it wrote".
It is renamed because decision 4 declines to make that true at every depth,
and a title should not promise what a decision withholds.

## Context

`host::json_stringify` is record 0019's: bounded, cycle-guarded, and it
names where it refused. `host::json_parse` is this, in full:

```rust
fn json_parse(text: &str) -> Result<Value, String> {
	serde_json::from_str(text).map_err(error)
}
```

Everything it does is Rune's `Deserialize for Value` and serde_json's
defaults. Measured on 2026-09-13, against 0.14.2 on Linux:

| document | what comes back today |
| --- | --- |
| `1` | `1` |
| `9223372036854775807` | exact |
| `9223372036854775808` | **`-9223372036854775808`** |
| `18446744073709551615` | **`-1`** |
| `18446744073709551616` | `18446744073709552000.0` |
| `-9223372036854775809` | `-9223372036854776000.0` |
| `1.5`, `1E2` | `1.5`, `100.0` |
| `-0` | `-0.0` |
| `1e400` | refused: number out of range |
| `null` | `()` |
| `[1,null,2]` | `[1, (), 2]` |
| `{"a":1,"a":2}` | `{"a": 2}` |
| `1 2` | refused: trailing characters at line 1 column 3 |
| `{`, `nul`, `[1,]`, `"\ud800"` | refused, each naming a position |
| 128 levels of `[` | refused: recursion limit exceeded |

Two of those rows are not choices anyone made. `9223372036854775808` and
`18446744073709551615` are inside JSON's range and inside Rune's — Rune
holds `18446744073709551615u64` and prints it — and they come back wrapped,
silently, as a negative number. The cause is upstream, in Rune's
`Deserialize for Value`:

```rust
fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E> {
    Ok(Value::from(v as i64))
}
```

with `visit_u128` the same. A draft is filed beside this record.
The shortest demonstration uses only rnx's own two functions:

```
rnx eval 'host::json_stringify(18446744073709551615u64)?'   → "18446744073709551615"
rnx eval 'host::json_parse("18446744073709551615")?'        → -1
```

rnx writes it correctly and reads it wrongly. That is this record's first
job, and rnx does not have to wait for upstream to do it: nothing obliges
`json_parse` to go through Rune's deserializer.

The rest of the table is behaviour nobody has decided: it is what two
dependencies happen to do. Record 0031's gate 3 asks for numeric
conversion, null, duplicate keys, encoding, malformed input and nesting to
be specified. This specifies them.

## Decision

### 1. A number keeps its value, or the parse says it could not

Parsing goes through `serde_json::Value` and rnx converts, rather than
letting Rune's deserializer do it. Then:

- an integer that fits `i64` is a signed integer;
- an integer above `i64::MAX` and at most `u64::MAX` is an unsigned integer,
  which Rune has and which `json_stringify` already writes back exactly;
- an integer **outside `i64::MIN ..= u64::MAX`** — below `-2^63` or above
  `2^64 - 1`, both ends, `2^64` itself being the first above — is read as a
  double, which is what serde_json has already decided by the time rnx sees
  it. This record states that rather than discovering it: outside that range
  a JSON integer is a double and its last digits are gone;
- a number with a fraction or an exponent is a double by the same
  conversion, and one that overflows to infinity stays refused, as it is
  today.

The double is **serde_json's conversion, which is approximate**, and this
record does not promise the nearest representable double, because the
configured parser does not give one. Measured: `55527869896048623745833`
reads as `0x1.78457fdde62a9p+75`, where the nearest double is
`0x1.78457fdde62aap+75` — one unit in the last place away. serde_json's
`float_roundtrip` feature makes the conversion correctly rounded; it is not
enabled and this record does not enable it. That changes how every float in
every document is read, which is a numerics decision rather than a contract,
and belongs to a record that measures it.

`-0` reads as `-0.0`: serde_json makes it a float and rnx does not re-lex
numbers to disagree. Stated, not fixed.

### 2. `null` is the unit value, in both directions

It already is, and `json_stringify` already writes unit as `null`, so the
pair round-trips. This record gates it rather than leaving it to two
dependencies that happen to agree.

### 3. A duplicate key takes the last value, and that is said out loud

RFC 8259 leaves it undefined; serde_json, JavaScript and Python all take
the last. rnx does too, because reading is about accepting documents the
world actually sends, and refusing one that every other tool accepts helps
nobody. The asymmetry with record 0019 is deliberate: rnx refuses to
**write** anything ambiguous, and reads what exists. What a caller loses is
the ability to know a key was repeated; that is the cost, and the
documentation says so rather than leaving it to be discovered.

### 4. Two depth bounds, both stated, and neither moved

The two differ and this record leaves them differing. The writer stops at
256 levels, record 0019's `MAX_DEPTH`, deliberately **shared with the
renderer** — `tests/json.rs` has a gate by that name. The reader stops at
serde_json's own recursion limit, 128 today, which is not rnx's number. So a
value between the two can be written and not read back.

Both ways of reconciling them cost more than the asymmetry does:

- **Raising the reader to 256** means turning serde_json's recursion guard
  off (`unbounded_depth`) and replacing it with a depth scan of rnx's own
  over the document text, brackets and strings and escapes. That trades a
  guard that works for one rnx has to keep correct, and the failure when it
  is not correct is a stack overflow — the class of failure record 0019's
  recursive-drop draft is about. Not for a depth no real document reaches.
- **Lowering the writer to 128** unshares a bound record 0019 shared on
  purpose and moves its gates, to fix a case nothing reaches.

So both numbers are stated, in the documentation for the pair and in the
message when either is hit, and the asymmetry is written down rather than
left to be discovered. If a document ever arrives that needs more, the first
alternative is the one to cost out, with the scan gated before the guard is
turned off.

### 5. A refusal names the document, not just the parser

serde_json's messages carry a line and column **in the JSON text**, which
is not the script's line and column and must not be mistaken for it. A
refusal says what rnx was doing, and keeps serde_json's position, marked as
a position in the document.

### 6. Nothing routes around the guarded serializer

`host::json_stringify` stays the only way to produce JSON, with record
0019's bound, cycle guard and refusal vocabulary. This record adds no
second writer and changes nothing it does. `host::json_parse` keeps its
name; a script that uses it today keeps working, except where it was
getting a wrong number.

### 7. What this record does not decide

- The companion modules record 0031 lists. This is rnx's own JSON, not
  `rune-modules`' `json`, and record 0031 already says that module is to be
  wrapped or selectively registered rather than installed as it stands.
- Key order. Objects come back in whatever order the map gives, and
  `json_stringify` already does not preserve insertion order. That is a
  separate question and a separate record.
- Streaming, partial parses, or parsing from a file without reading it
  into memory first.

## Acceptance gates

1. **A number survives a round trip.** For a set spanning `0`, `±1`,
   `i64::MAX`, `i64::MAX + 1`, `u64::MAX`, `i64::MIN`, and floats,
   `json_parse(json_stringify(v))` equals `v`, and in particular
   `18446744073709551615u64` does. An integer outside `i64::MIN ..= u64::MAX`
   is gated as a double at both ends, not as an equality; the gate asserts
   what serde_json's conversion gives rather than the nearest double, and
   carries the measured one-unit example, so enabling `float_roundtrip`
   later shows up as a gate that has to move.
2. **The table above is a test.** Every row, as a gate, with the two
   wrapped rows corrected and the rest asserted as decided.
3. **`null` and unit are the same value both ways**, inside containers as
   well as alone.
4. **A duplicate key takes the last value**, gated so the choice cannot
   change silently.
5. **Both bounds.** A document at the reader's bound parses and one past it
   is refused, carrying a position; a value at the writer's bound is written
   and one past it is refused in record 0019's words. Both numbers are
   asserted, so the day either changes, a gate says so.
6. **A refusal says where in the document**, and nothing in the message can
   be mistaken for a position in the script.
7. **Nothing about `json_stringify` changes.** Record 0019's gates pass
   unchanged — including `the_depth_bound_is_shared_with_the_renderer`, which
   decision 4 is written to keep — and the JSON workload in `rnx-bench`
   produces identical output.

## Guardrails and stop conditions

1. If fixing a number requires a new dependency feature, stop and say so:
   `arbitrary_precision` changes serde_json's behaviour well beyond this.
2. One writer and one reader, each in one place in the source.
3. No silent widening, with one exception stated at both of its ends: an
   integer outside `i64::MIN ..= u64::MAX` — below `-2^63` or above
   `2^64 - 1` — is read as a double by decision 1 and documented rather than
   refused. Everything inside that range is exact, or the parse says why not.

## Risks

- **A script relying on today's wrapped number.** It is getting a wrong
  answer now; anything relying on it was already broken.
- **The two depth bounds drifting apart.** The reader's is serde_json's and
  can move in a version bump. Gate 5 asserts both numbers, so a bump shows up
  as a failing gate rather than as a wider gap nobody notices.

## Forward

Record 0031's gate 4, HTTP, whose JSON helpers must use these rules rather
than a second set.
