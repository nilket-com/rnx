# 0068 gate 2: the product port and its matrix

Status: the accepted gate 1 candidate is ported into the product unchanged
in substance, and the product-level matrix passes on Linux. Gates 3 and 4
remain open.

## The port

`rnx-bench/probes/frame-seam/candidate.patch` (the gate 1 source, SHA-256
`cbf59f67…`) applies to `73532b5` without conflict: `src/present.rs` is new;
`extensions.rs`, `lib.rs`, `session.rs`, `repl.rs`, `worker.rs` and
`config.rs` carry the seam; the Polars adapter gains `present` and
`DISPLAY_FMT` over the sink-based `render_into`; the project tool gains the
optional `presentation` field, its generator line and the catalogue default.
Nothing in the patch changed between acceptance and port; formatting was
applied afterwards (`cargo fmt` in the three crates).

Beyond the patch, this gate adds only tests, test fixtures and documentation:

- `src/rnx_test.rs` (test-support only) registers three presented types —
  `Presented`, `Broken` (fails with 20,000 repetitions of a coloured error)
  and `Loud` (pushes an OSC title sequence until the budget refuses) — into
  the same registry the extensions fill, after them. The production binary
  has no presenter, as before.
- `adapters/polars/src/bin/fixture.rs` and `tests/presentation.rs`, both
  behind `required-features = ["test-support"]`: the generated wrapper's
  exact shape (`with("polars", build).present("polars", present)`), driven
  as a notebook worker over its control pipes.
- READMEs: the adapter's and the tool's passages that said bare frames stay
  opaque now say when they present and that `preview()` is unchanged.

## The matrix

**Preview oracle through the product** (`adapters/polars/tests/presentation.rs`,
`every_oracle_shape_presents_exactly_the_explicit_preview`). Eleven CSV-read
frames — empty (0 × 2), narrow (1 × 1), wide (1 × 12), tall (15 × 1), nulls
in all four dtypes, an 81-scalar UTF-8 cell, controls with a tab, newline and
escaped quote in a cell, bidirectional controls, float extremes (`1e308`,
`-0.0`, `5e-324`, `0.1`, `-1.7976931348623157e308`), the byte-limit frame
(10 × 8 of 80 crabs) and a column name carrying ESC and a tab — each pass
through the worker as a bare top-level value. For every one the `text_plain`
equals, byte for byte, the `preview()` string the same worker wrote to a
file, and `format!("{f}") == f.preview()?` evaluates true. Each text is at
most 8,192 bytes, contains no control character but newline, and begins with
`DataFrame: `; the byte-limit frame ends with the omission marker and no
other does. A second bare evaluation is identical and a second `preview()`
after presenting equals the first: the frame is unchanged. Spellings are
fixed: `null | null | null`, eighty crabs then `…[truncated]`,
`\u{1b}[2J tab\t nl\nend quote\"`, `\u{202e}` and `\u{2066}` escaped, the
five floats on their own lines, `[0 rows and 4 columns omitted …]` with no
`c8`, `[5 rows and 0 columns omitted …]` with thirteen lines, and the column
name `"a\u{1b}[2Jb\tc": i64`. A bare U+200F directional mark is not an
embedding control and passes unescaped, which is the existing preview policy.

**Inspected work** (`adapters/polars/src/preview.rs`,
`inspection_is_bounded_by_structure_not_frame_size`). A counting sink under
`render_into` sees the same tokens for 200,000 × 40 as for 10 × 8 but the
omission line and a longer title, exactly `ROWS × COLUMNS` = 80 cells either
way, and fewer than 100 extra bytes. For 12 × 9 cells of 5,000 crabs no
token carries more than 80 scalars, the output stays under the cap, and
rendering stops incomplete at the cap. `rendering_is_deterministic_and_leaves_
the_frame_unchanged` renders eleven times and compares the frame to its clone
with `equals_missing`.

**Presentation is not collection** (`lazy_values_stay_opaque_and_unexecuted_
and_presentation_runs_no_engine`). The adapter's test-support engine counter
does not move when a frame presents; a `LazyFrame` filtering on a missing
column, a `LazyGroupBy` and an `Expr` display as their opaque labels with the
counter unchanged; `plan.collect().is_err()` is true and moves the counter by
one — the error surfaces where the work happens. `[sales]`, `Ok(frame)` and
`(frame, 1)` keep the generic rendering. `format!("{plan}")` on a lazy plan
is a failure (no display protocol), and the session continues.

**Failures and fallbacks** (`src/present.rs` `failure_tests`; `tests/repl.rs`
`gate_0068_a_presented_value_at_the_prompt`; `tests/presentation_worker.rs`).
A value mutably borrowed while presented yields `Some(Err("Cannot read …"))`,
`finish` renders the opaque label plus `(preview unavailable: …)` within the
budget, and once the borrow ends the value presents normally. At a real
prompt (pseudo-terminal, `NO_COLOR`): `let v = …;` prints nothing and spends
no number; `v` prints `[2] Presented with 3 rows`; `[v]`, `(1, v)` and
`Ok(v)` print opaque labels; `let b = rnx_test::test_broken(); b` prints
`[6] <::rnx_test::Broken> (preview unavailable: \u{1b}[31mbroken…` once, no
raw ESC reaches the terminal and the whole transcript stays under 40 KB
(the error alone would be 400 KB); `v` still presents at `[7]`;
`rnx_test::test_loud()` prints escaped OSC bytes cut at the budget, never an
interpreted title; `:vars` shows types, not presentations; `:reset` keeps
the registry. The worker path reports the same texts in `text_plain` with
`failure` null and `render_bounded` true, both fallbacks under 16,384 bytes
and free of raw ESC, keeps `v` usable after them, and keeps the registry
across `reset`.

**Checks.** `cargo fmt --check` in all three crates; strict clippy: root
unchanged at its 16 pre-existing diagnostics on this toolchain, adapter and
tool clean with test-support; the six suites serially: root default 390 (376 + 12 seam tests + 2
failure tests), root test-support 435 (the two host-function inventories
now count seven test-support entries), Polars default 9 and test-support
10 + 2 integration, tool 54 and 56.

One pre-existing flake surfaced, not introduced here: the adapter's
`engine::tests::panic_is_joined_before_resuming_on_the_caller` reads
process-global counters while `files.rs` tests run the engine on other
threads, and fails in about half of parallel test-support runs on the
untouched baseline tree as well; it passes serially every time. It is
recorded for a follow-up, not fixed in this record.

## Qualifications and next gate

The matrix drives the worker for exact comparisons and the pseudo-terminal
for the prompt; the real interactive and notebook journey with a project
built by the tool is gate 3, and costs, the walkthrough and the notices are
gate 4. `preview()` policy is unchanged by this record, including the
directional-mark case noted above. No Windows claim is made.
