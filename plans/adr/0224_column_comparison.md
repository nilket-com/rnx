# ADR 0224: Cross-engine column comparison against Polars, Arrow and DuckDB

Status: Accepted

The user's yardstick for rnx column work is Polars and Arrow, with DuckDB
alongside. ADRs 0213–0223 measured rnx only against itself, in executed
instructions. That does not compare across engines. This record adds a
committed, standalone comparison harness to rnx, and publishes measured wall
times in rnx-bench under the same ADR number.

Scope: the eleven column shapes measured in 0220–0223:
- F64 `x+1`, `x+y`, `x*2+y`;
- Bool `x&y`, `x|y`, `(a&b)|c`;
- F64 `x>1`, I64 `x==k`, `x>1 & y<2`, that conjunction `| z`, `x*2+y>3`.

These run over the existing fixtures (flat, sliced-chunked-null, alternating,
deterministic-random) at 10M and 100M rows. The comparators are Python Polars
2.1.0, pyarrow.compute 26.0.1 (direct eager kernels) and DuckDB 2.0.0. They
are installed with pinned wheels into a user-local virtual environment, with
the wheel hashes recorded. There are no source builds and no version
substitution. Acero is out of scope.

Semantics are made equal before timing:
- strict all-input nulls (Polars and SQL wrap the Kleene operators; Arrow's
  `and_`/`or_` are already strict);
- IEEE NaN comparisons (Polars and DuckDB order NaN greatest, so they guard
  computed operands);
- separate multiply and add.

Outputs compare by validity and values against an independent oracle,
chunkwise, including NaN-class, signed-zero, infinity and truth-table
controls with differing masks. A shape that can't be made equivalent is
reported as non-comparable, never relaxed.

Report two panels per engine (secondary only for shapes whose semantics differ):
- **Equivalent semantics** (primary): the adaptation cost is charged inside
  the timed call.
- **Idiomatic native semantics** (secondary): each engine's natural
  expression, labelled "not semantically equal to rnx". The adaptations
  cost competitors work, and readers should see both.

Measure wall time with internal monotonic timers:
- preparation is excluded; full output materialization is included;
- outputs are checked after each timed call;
- single-thread and default-thread modes are reported separately; rnx is
  single-threaded; Arrow is labelled serial direct kernels. Each is measured
  once and explicitly reused in both panels;
- primary: 5 independent process samples per cell at both sizes; secondary
  native: 100M only, 3 samples; each one first call plus 5 repeated calls,
  reporting medians and ranges;
- a quiet exclusive serial lease, with a fixed 120 s cooldown per round
  rather than per sample;
- thermal, frequency and load snapshots, with contaminated data flagged,
  not replaced;
- peak RSS labelled as process peak.

Durable per-sample receipts, resumable by identity.

Honesty constraints:
- no aggregate score;
- versions, settings, host class ("Intel i7-14700 measurement host") and
  conditions stated;
- whole-kernel API comparisons, not pure-SIMD claims, with DuckDB SQL
  dispatch and result export included;
- no claim that any engine fuses expressions;
- column-only scope.

Add `time::monotonic_ns() -> i64` beside the millisecond clock, sharing its
process-relative Instant origin. Millisecond quantization is unacceptable for
short 10M-row calls. Register and document the API, test monotonicity and the
shared origin, and measure the 0224 production binary. Record column-source
byte identity against 6d253fad; this is not a column implementation change.
Both workers use monotonic nanosecond clocks around the whole map call; record
clock source and resolution without promising nanosecond hardware accuracy.

The committed harness contains no home paths and no agent-infrastructure
calls. Notifications stay external.
