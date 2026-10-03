# rnx 0158 evidence: a Polars-style table layout for DataFrame previews

**Status:** implemented, for review.

## Source and environment

- **Built and checked at `a21f7f2`.** This commit's tree is that one plus this file.
- **Toolchain and OS:** rustc 1.98.1 and cargo 1.98.1 on Linux 7.0.0-31-generic x86_64. The checker ran in `demos/notebooks/requirements.txt` (Python 3.14.4, nbclient 0.11.0), with `-W error::RuntimeWarning`.

| file | sha256 |
|---|---|
| `demos/polars/target/release/rnx-polars-demo` (worker, built at `a21f7f2`) | `d38fab0ee821cfd68a47e0874a6fcb02d5b1ebef80cb28c1e57144ce96818adf` |
| `target/release/rnx` (rebuilt at `a21f7f2`) | `0116d5d12e66d28f7e9ee5103b4e6692c093a20761af83c4f48bfce85fa4191b` |
| `jupyter/target/release/rnx-jupyter` (unchanged) | `05aedaea48c128b58ef18a2b2bc75a55c748ef1fe92fd82d7f428e050c50f8b5` |
| `adapters/polars/Cargo.lock` | `d0d2236acb99b880afb7f98bbcdb765217fe5815c9e08404d6dab08520ee533b` |
| `demos/polars/Cargo.lock` | `b4a8653827875f1be8d2f6962abe8fa6de67177e2d76cb9e9934b75521ed44dd` |

**Dependencies:** each lock gains exactly one line, the `unicode-width` edge from `rnx-polars`. The package was already locked at 0.2.2 through Polars. No Polars feature or binding inventory changes, and `fmt_no_tty` stays off.

## What changed

- **`adapters/polars/src/preview.rs`:**
  - the layout writer;
  - `shown_rows` (first and last 5 past 10 rows);
  - unquoted escaping at the scalar writer (`plain`, `escaped_into(…, quoted)`);
  - `CUT` = `…`;
  - a `cfg(test)` read counter.
- **0124's reader safety contracts are preserved** (`cell`, `dtype`, `list`, `temporal`, `Bounded`, `shown_columns`, `Capped`): bounded reads, whole-token escapes, checked temporals and opaque placeholders. Two things changed on purpose: top-level strings and names are unquoted, and the cut marker is `…`.
- **`lib.rs`:** a `#[doc(hidden)] pub fn preview_text`, used only by the allocation control.

## Tests

**`cargo test --offline --features test-support --no-fail-fast` in `adapters/polars`: 56 binaries, 280 passed, 0 failed.**

New in `preview.rs`:
- `the_users_reference_frames`: the user's two ipython frames, byte for byte;
- `empty_frames`;
- `dimensions_and_row_column_boundaries`, which now covers 1 column, exactly 10 rows, 11 rows, exactly 8 columns and 9 columns, and checks every cell by position and the read count;
- `omitted_hazardous_columns_are_never_read`: 8 reads of 12 columns, and none of the four hazards' text appears;
- `cells_align_by_display_width` (CJK, combining marks, ZWJ emoji);
- `a_literal_ellipsis_is_just_text`;
- the unquoted-ambiguity and whole-cut assertions in `scalar_boundaries_and_escaping`;
- whole lines, no bottom border, and a first border that can't fit (shape line then marker) in `total_byte_boundary_and_complete_tokens`.

Carried over: every 0124 safety test, with only the expected text changed and every bounds assertion kept. `presentation.rs` and `collect_dtypes.rs` assert the new layout.

**`tests/preview_alloc.rs`,** with the allocation peak above the live bytes, after each frame exists, in one test per binary:

```
hostile 1,000,000 x 1,000: peak 124206 bytes above base, 67 bytes shown
numeric 1,000,000 x 1: peak 1604 bytes above base, 327 bytes shown
```

- **The hostile frame:** every name and cell is `\u{202e}` × 10,000 (8-byte escapes), in scalar columns.
- **The computed bound is 512 KiB:**
  - 14 rows × 9 cells, each at most 80 escaped scalars of at most 8 bytes plus the marker, at up to twice that in capacity: about 160 KiB;
  - one line at a time, with box-drawing characters at 3 bytes each: under 70 KiB;
  - vectors, widths and output: under 20 KiB.
- **What shows:** that frame's first border can't fit the byte cap, so only the shape line and the marker show, as the plan requires.

**Root:** `cargo test --locked`: 49 binaries, 407 passed, 0 failed. The root crate doesn't link the adapter (`cargo tree` has no `rnx-polars`), so stock `rnx` is unaffected by construction.

**Unrelated oracle noise:** two full adapter runs each rewrote `oracle-results.json`, with one and then two `row_order_differs` on `LazyFrame::unique`/`unique_generic`. Polars documents those as order-unspecified, and the oracle doesn't use the preview. The committed file was restored both times. This is pre-existing run-to-run order nondeterminism, noted and not fixed here.

## The notebook

`04_polars_sales.ipynb` was regenerated with `--generate --polars`, so only that notebook was written. One markdown cell describing the preview was reworded. The analysis, data and answers are unchanged.

`check.py`'s parser now reads the table structurally:
- the shape line, then the top border, which gives the column widths;
- every row padded to exactly those widths;
- the `---` row, the separator and the bottom border;
- the elided row by position (after the first 5) only when the shape says there are more than 10 rows;
- a `…` anywhere else is refused.

Partial frames are checked against the CSV's first and last 5 rows.

The demo tree's digests were taken before and after these runs; verification and the controls wrote nothing. The controls give 22 verdicts, all as stated: 7 plain, 15 Polars. The new ones are an elided row in the whole region frame and a literal `…` cell.

```
$ check.py (plain)
ok: 01_mortgage on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on plain rnx: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone)
ok: 03_report on plain rnx: 2 runs match the stored outputs and the terminal reference
exit 0
$ check.py --polars
ok: 01_mortgage on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on plain rnx: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone)
ok: 03_report on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 04_polars_sales on the Polars worker: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone); every frame and answer matches sales.csv
ok: 01_mortgage on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 03_report on the Polars worker: 2 runs match the stored outputs and the terminal reference
exit 0
$ check.py --polars --controls
pass: a harmless same-stream split: accepted
pass: one byte of an answer changed: cell 7: outputs differ:
pass: two different results swapped between cells: cell 3: outputs differ:
pass: stdout turned into stderr (comparison): cell 7: outputs differ:
pass: stdout turned into stderr (judge): cell 7: unexpected stderr output '12 orders read from ../data/orders.json\nShipped revenue by region:\n  north    245.00\n  east     144.97\n  south     99.95\n  west       0.00\nLargest customer: Acme (134.99), ahead of Birch (134.98)\nStill open: 4 of 12 orders (33.3%)\n'
pass: a cell that raises: cell 8: error output RuntimeError: runtime error at input 5, line 2, column 1: Type `::std::vec::Vec` missing integer index `5`
pass: a stream line not matching the terminal reference: the stream no longer matches the terminal reference
pass: the unaltered run against sales.csv: accepted
pass: a harmless same-stream split: accepted
pass: one byte of an answer changed: cell 14: outputs differ:
pass: two different results swapped between cells: cell 4: outputs differ:
pass: stdout turned into stderr (comparison): cell 14: outputs differ:
pass: stdout turned into stderr (judge): cell 14: unexpected stderr output 'Net revenue: $1227.50 from 20 orders (4 returned, left out)\nTop region: north, $387.50, 31.6% of net revenue\n'
pass: a cell that raises: cell 15: error output RuntimeError: runtime error at input 8, line 2, column 1: Type `::std::vec::Vec` missing integer index `5`
pass: a stream line not matching the terminal reference: the stream no longer matches the terminal reference
pass: the stored outputs against an edited CSV: sales: rows differ:
pass: the edited CSV's own terminal answer against the original's expectations: answer (124000, 20, 4, 'north', 40000, 323), expected (122750, 20, 4, 'north', 38750, 316)
pass: two rows swapped in the region frame: by_region: rows differ:
pass: an elided row where the region frame must be whole: 5 rows shown of 4
pass: a literal … cell where no row is elided: not a simple string cell: '…'
pass: a duplicated row in the region frame: 5 rows shown of 4
pass: a non-finite mean: not a finite float cell: 'NaN'
exit 0
demos unchanged by verify and controls
$ check.py --polars (root rnx rebuilt at a21f7f2)
ok: 01_mortgage on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on plain rnx: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone)
ok: 03_report on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 04_polars_sales on the Polars worker: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone); every frame and answer matches sales.csv
ok: 01_mortgage on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 03_report on the Polars worker: 2 runs match the stored outputs and the terminal reference
exit 0
demos unchanged
```

## Cost, measured and not asserted in tests

**Method:**
- one core (`taskset -c 3`) and `POLARS_MAX_THREADS=1`;
- the baseline worker built from `e9c80f0` in a scratch worktree, against this commit's worker;
- an in-process loop of 20,000 `preview()` calls per frame, timed with `time::monotonic_ms`, three interleaved rounds;
- the measured cases are the 24 × 6 sales frame and a generated 200,000 × 12 frame of alternating string and i64 columns.

The plan's 1M × 1,000 case isn't in this timing table. It's covered structurally (read counts) and by the allocation control.

| frame | baseline bytes | 0158 bytes | baseline µs/preview | 0158 µs/preview |
|---|---|---|---|---|
| sales 24 × 6 | 684 | 1,689 | 5.8–6.1 | 10.9–11.0 |
| 200,000 × 12 | 1,075 | 2,221 | 7.9–8.4 | 14.7–14.9 |

- **Showing a frame stays far under 0068's ≤ 0.15 ms** (150 µs).
- **Bytes grow** with the 3-byte box-drawing characters and the padding.
- **Worker launch** (`run` on an empty script, hyperfine, 200 runs, 20 warm-up, pinned): baseline 16.3 ± 0.2 ms, 0158 16.0 ± 0.1 ms.

## Other gates

- `cargo fmt --check` for the adapter passes.
- `git diff --check` is clean.
- No 4-space indentation in any added line, including the notebook's code cells.
- No rnx core, kernel, Polars feature or inventory change.
