# Upstream defect draft: deserializing an unsigned integer wraps it

Ready to file against `rune-rs/rune`; the report is everything below the rule.
This is a **defect**: a value inside the range of a type Rune has comes back
as a different, negative number, with no error.

Not filed. Filing it is the operator's call. Found on 2026-09-13 while giving
rnx's JSON parsing a contract; rnx no longer routes through the deserializer,
so it is unaffected, but every other host that deserializes into `Value` is.

---

**Title:** `Deserialize for Value` casts `u64` and `u128` to `i64`, so an
unsigned integer above `i64::MAX` deserializes to a negative number

**Version:** 0.14.2. Present on `main` (0.15.0 at `bb8e6937`) by reading.

### What happens

```rust
let value: rune::Value = serde_json::from_str("18446744073709551615").unwrap();
// value is -1
```

| document | deserializes to |
| --- | --- |
| `9223372036854775807` (`i64::MAX`) | correct |
| `9223372036854775808` | `-9223372036854775808` |
| `18446744073709551615` (`u64::MAX`) | `-1` |

No error is raised, and the value is silently wrong.

### Where

`runtime/value/serde.rs`:

```rust
fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
where E: de::Error,
{
    Ok(Value::from(v as i64))
}

fn visit_u128<E>(self, v: u128) -> Result<Self::Value, E>
where E: de::Error,
{
    Ok(Value::from(v as i64))
}
```

`v as i64` wraps. The neighbouring `visit_i64` is `Value::from(v)` and is
correct.

### Why it looks like an oversight rather than a decision

`Value` already represents unsigned integers — `Inline::Unsigned(u64)` — and
the `Serialize` half writes them back exactly. So the round trip is lossy
against Rune's own serializer:

```rust
let v = rune::Value::from(u64::MAX);
let text = serde_json::to_string(&v).unwrap();   // "18446744073709551615"
let back: rune::Value = serde_json::from_str(&text).unwrap();  // -1
```

`visit_u64` has a lossless value to produce and produces a wrapped one.

### Suggested fix

`visit_u64` becomes `Ok(Value::from(v))`, which selects the unsigned inline
variant. `visit_u128` has no lossless target and should convert when the value
fits `u64` and fail otherwise, rather than wrap — `E::custom` with the value in
the message.
