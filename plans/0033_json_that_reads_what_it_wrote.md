# rnx 0033: JSON that reads what it wrote

Status: proposed 2026-09-13. The thirty-third record of rnx, and record
0031's gate 3. rnx has had `host::json_parse` since the spike: one line,
`serde_json::from_str` into a Rune value, with no contract of its own. This
record gives it one, and the first thing that contract has to fix is that
rnx cannot read back a number it wrote.

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
- an integer that fits `u64` but not `i64` is an unsigned integer, which
  Rune has and which `json_stringify` already writes back exactly;
- an integer that fits neither is read as the nearest `f64`, which is what
  serde_json has already decided by the time rnx sees it, and is what this
  record **states** rather than discovers: past 2^64 a JSON integer is a
  double and its last digits are gone;
- a number with a fraction or an exponent is an `f64`, and one that
  overflows to infinity stays refused, as it is today.

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

### 4. One depth bound, and a document may not exceed what rnx can write

Today the two disagree: the serializer stops at 256 levels (record 0019's
`MAX_DEPTH`) and the parser stops at serde_json's own 128, so rnx can emit
JSON it cannot read back. One bound, stated in the message when it is hit,
and the JSON pair uses it in both directions. Which number it is, is
decided in implementation against the two existing bounds; what this record
fixes is that there is one of them and that emitting past it is not
possible.

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
   `18446744073709551615u64` does. A number past `2^64` is documented as a
   double and gated as one, not as an equality.
2. **The table above is a test.** Every row, as a gate, with the two
   wrapped rows corrected and the rest asserted as decided.
3. **`null` and unit are the same value both ways**, inside containers as
   well as alone.
4. **A duplicate key takes the last value**, gated so the choice cannot
   change silently.
5. **One bound.** A document deeper than the bound is refused, naming it;
   a value deeper than the bound cannot be written; the two numbers are the
   same number, read from one place in the source.
6. **A refusal says where in the document**, and nothing in the message can
   be mistaken for a position in the script.
7. **Nothing about `json_stringify` changes.** Record 0019's gates pass
   unchanged, and the JSON workload in `rnx-bench` produces identical
   output.

## Guardrails and stop conditions

1. If fixing a number requires a new dependency feature, stop and say so:
   `arbitrary_precision` changes serde_json's behaviour well beyond this.
2. One writer and one reader, each in one place in the source.
3. No silent widening. If a value cannot be represented, the parse says so;
   the one exception is the integer past `2^64`, which is decided in
   decision 1 and documented rather than refused.

## Risks

- **A script relying on today's wrapped number.** It is getting a wrong
  answer now; anything relying on it was already broken.
- **The depth bound moving.** If the single bound is lower than 256, a
  value that could be written yesterday is refused today. The gate for
  record 0019's boundary has to move with it, deliberately.

## Forward

Record 0031's gate 4, HTTP, whose JSON helpers must use these rules rather
than a second set.
