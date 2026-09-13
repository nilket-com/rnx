# rnx 0033 evidence: the contract `json_parse` never had

Measured on nano, Linux, Intel i7-14700, 2026-09-13, Rust 1.98.1
(`48a229cea`), Rune 0.14.2, serde_json 1.0.151. No dependency or feature
changes. This is Linux evidence, not Windows validation.

## Conversion and refusal gates

`tests/json_parse.rs` exercises the public script API in seven tests:

- Signed integers through `i64::MAX`, unsigned integers through `u64::MAX`,
  and `i64::MIN` retain their value. Script `is i64`/`is u64` checks pin the
  chosen representation. Scalar and array serialization preserve the large
  positive integers, and the planned numeric round trips return equality.
- Out-of-range integers retain serde_json's configured double conversion at
  both ends. `55527869896048623745833` writes as
  `5.552786989604862e+22`, retaining the measured approximate conversion.
  Fraction, exponent, and negative-zero cases are gated too. The numeric
  values in the plan's rendering table use exponent notation when written
  as JSON; no serializer change was needed.
- Null is unit, alone and inside arrays and objects. Booleans and empty
  containers retain their JSON shape.
- Duplicate keys keep the last value, including equivalent escaped keys.
- Unicode, a surrogate pair, and escaped controls round-trip. An unpaired
  surrogate is refused (serde_json calls it an unexpected end of hex escape).
- Malformed input, trailing input, and numeric overflow produce catchable
  errors prefixed `cannot parse JSON document`, retaining document line and
  column. A multiline document pins its own position independently of the
  one-line script calling the parser.
- Arrays and objects nested 127 levels parse; 128 and 2048 are refused
  without a signal. The refusal names the 127-container maximum and the
  dependency's 128 recursion counter. The parser's recursion guard stays on.

The existing `tests/json.rs` is unchanged, including
`the_depth_bound_is_shared_with_the_renderer`: the writer still accepts 256
levels and refuses 257. There is no second serializer.

## Validation

```text
cargo test --locked                          252 passed, 0 failed
cargo test --locked --features test-support  280 passed, 0 failed
cargo build --release --locked               passed
scripts/third-party-notices.sh --check        passed; notices unchanged
```

The default suite preceded the additional integer representation assertions;
the final test-support suite includes them, and a final default-feature
`cargo test --locked --test json_parse` rerun passes all seven reader tests.
Both full suites execute the unchanged writer gates.

## Existing workload and startup

Before is the release executable preserved before implementation, with
SHA-256 `e80842eaaa1ece5dd8dddde98df6c1ee6698877c8279a58ac55d311c3c53759d`.
After is the locked release build of this implementation, with SHA-256
`d8d78d10954493a451fc90132e44d9430398c5c006a061585167ac2c839a4faf`.
The measurements compare those artifacts; the before artifact was not
rebuilt during this implementation.

The bare and 10,000-iteration JSON scripts are in rnx-bench commit
`115270f88e551ed0b8c4f8f1f2a16a767af154f6`, `scripts/bare.rn` and
`scripts/json.rn`. Both JSON workload runs exit 0 and have byte-identical
stdout, SHA-256
`9e7a787318d33adbd30f40f3594ba28f739932a1fac12b9fbf64b9e681764c6c`.

`taskset -c 4 hyperfine -N --warmup 10 --runs 50`, with before and after
adjacent for each command, produced these milliseconds (mean ± standard
deviation). Raw samples are in [the export](0033_startup_measurements.json).

| command | before | after |
| --- | --- | --- |
| version | 0.525 ± 0.047 | 0.496 ± 0.021 |
| help | 0.495 ± 0.017 | 0.499 ± 0.017 |
| eval 42 | 3.945 ± 0.035 | 3.896 ± 0.022 |
| run bare file | 3.614 ± 0.035 | 3.558 ± 0.025 |
| run JSON workload | 11.571 ± 0.123 | 11.603 ± 0.127 |

No startup regression is evident in this run. These commands do not parse
JSON, so these measurements say nothing about parser throughput. Binary
sizes were 9,881,808 and 9,880,504 bytes. A fresh piped session's `:memory`
reported startup reference points of 1,798,326 and 1,798,291 bytes. These
single allocator observations are not peak memory measurements.

## Limits

Parsing builds an intermediate serde_json tree, then consumes it to build
Rune values. Peak parser allocation and parsing throughput were not measured.
The conversion stays within the already-checked reader depth; allocation
errors from Rune's containers return a conversion refusal without inventing
a source position. The existing process-wide allocation ceiling remains in
force. No upstream change, fork, or new module registration is involved.

This completes record 0031's JSON parsing contract work. Its separate async
CPU-interruption clause remains open, and HTTP still needs its own work.
