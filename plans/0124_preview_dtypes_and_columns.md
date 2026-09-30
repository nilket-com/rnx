# rnx 0124: the preview shows every dtype, and keeps a frame's last columns

Status: plan. Record 0123 is closed on `origin/main` at `7629cd3`. The user approved this record from 0123's display ranking. It takes both display families at once, because each is small and they share one function.

**The measure (0123's probe, v2).** 5 of 8 composed workflows compute, but only W2, W3 and W8 are usable: they compute and also display fully, printed and at the session prompt.
- **P (preview dtypes):** keeps 6 steps and W4 from displaying.
- **Q (preview columns):** makes 2 steps and W7 partial.

This record aims to make W4 and W7 usable without weakening any of the preview's bounds.

## What the preview is today (record 0058, `adapters/polars/src/preview.rs`)

- **Scope:** it inspects bounded slices only: at most 10 rows and 8 columns, each scalar cut at 80 characters, 8,192 bytes in total with an omission marker. The explicit `preview()` string, a frame's `DISPLAY_FMT` (`println!`) and the session presenter all go through `render_into`, so their text is identical. It never formats a whole frame or an arbitrary `AnyValue`.
- **P:** `dtype()` and `cell()` accept only String, Int64, Float64 and Boolean; anything else is an error, and the whole frame shows "preview unavailable". So a count (UInt32), a date, a weekday (Int8) or a rank refuses the entire preview.
- **Q:** it shows the first 8 columns. A workflow's derived columns are appended last, so they are the ones cut. Polars' own display shows the first and last columns with an ellipsis between them.

## 1. Every dtype, bounded (family P)

**Column dtypes** (review of the plan: bounded before the cut, not after it):
- The four existing names keep their spelling (`string`, `i64`, `f64`, `bool`), because 0058's tests and users' output depend on it.
- A dtype name is built by a bounded, early-stopping formatter. It never formats a whole nested `DataType` and then cuts the text:
  - `List` recurses as `list[…]`, and stops with `…` once the name passes the scalar bound. A 2,000-deep nested list costs one bound's worth of text.
  - Struct, Array, Object, Categorical, Enum and any other nested dtype get a fixed name (`struct`, `array`, `object`, `cat`, `enum`, `nested`). Their fields and categories are never formatted.
  - A closed list of fixed-width dtypes shows Polars' own `Display` (for example `u32`, `date`, `datetime[ms]`): numbers, decimal, null, bool, string, binary, date, time, duration and an unzoned datetime (review of the implementation, round 1).
  - A zoned datetime is `datetime[unit, zone]`. A zone is arbitrary text, so its characters are escaped as names and strings are, and counted against the bound (review of the implementation, round 2).
  - Like a quoted string, a dtype name is bounded in input characters: at most 80 are written, each whole, so an escape is never split, and then `…[truncated]` once.
  - An extension dtype is `ext`, and any dtype the list doesn't name is `opaque`. Neither is ever formatted.

**Cells, each within the 80-character scalar bound:**
- **Integers of every width** (i8 to i128, u8 to u128), decimals and floats (f16, f32, f64): exact text. Floats use the shortest round-trip form, as f64 does today.
- **Date, Datetime and Time:** read as the physical integer, range-guarded (Polars' own `_opt` conversions overflow on a huge date and wrap a huge time), and converted with Polars' `_opt` conversions, giving the same text as Polars for every in-range value (review of the implementation, round 1). Polars' own `Display` for these values `expect`s the value in range and panics otherwise.
  - An out-of-range value shows `<out of range>`.
  - A zoned datetime shows its UTC instant, marked `UTC`. The 0.55.2 build has no time zone database, and Polars' zoned `Display` panics there; the header names the zone.
- **Duration:** Polars' own text, which only divides and takes remainders by positive constants.
- **Categorical and Enum:** the category's string, quoted and escaped as strings are.
- **Binary:** `b"…"` with each byte escaped, cut at the bound.
- **Null:** `null`, as today.
- **List:** a bounded rendering, `[1, 2, 3]`, bounded in input characters as a string is, across every nesting level (review of the implementation, round 3: an output-side cut split escapes).
  - Each opening bracket, separator character, string character and byte costs 1. Every other element is a token shown whole or not at all.
  - So at most 80 elements and 80 nesting levels are visited, nothing is cut inside an escape or a number, and `…[truncated]` is written once. A million-element list cell, or a list nested far deeper than the bound, costs no more than an 80-character one. This keeps 0058's rule: no arbitrary `AnyValue` is formatted whole.
- **Struct, Array, Object, and any other nested dtype:** a fixed placeholder from the bounded dtype name (`<struct>`, `<array>`, `<object>`), never an error. A frame with one opaque column still previews its other columns. Struct and Array cells are placeholders rather than recursive renderings, because their `AnyValue` variants are feature-gated and cannot be named at 0.55.2's feature set. A recursive rendering can come later, as its own record, if a workflow needs one.

After this, "unsupported column dtype" no longer exists. Every frame previews within the same bounds.

## 2. The first and the last columns (family Q)

When a frame has more than 8 columns, the preview shows the first 4 and the last 4, with a `…` column between them in the header and every row, as Polars' own display does. The omission line is unchanged ("[… rows and N columns omitted by display limits]"). Rows are unchanged: the first 10. A workflow's derived columns, appended last, are therefore always visible.

The structural bounds are unchanged (review of the plan): at most 8 inspected columns, 10 rows, 80 input scalars per cell and 8,192 output bytes. Every assertion of those bounds stays. The layout-specific expectations of 0058's tests change with the layout, and each change is named in the evidence:
- the boundary test no longer asserts that `column8` is absent; past 8 columns it asserts the first and last 4 and the marker;
- the 200,000 × 40 frame's token count grows by the marker column (a separator and a `…` on the header and on each shown row), and its inspected-scalar count is unchanged at 8 × 10.

## Proof

- **Unit tests in `preview.rs`:**
  - every dtype the probe produces, and each dtype class above, renders within the bound;
  - a huge list cell costs the same as a small one;
  - a very large nested schema (a 2,000-deep `List`) is named within the bound;
  - the first/last column selection and its marker;
  - the four legacy dtype names unchanged;
  - 0058's bound tests unchanged and passing.
- **The probe (0123's, extended as `probes/0124`):**
  - the P-family steps and W4 flip from refused to displayed;
  - the Q-family steps and W7 flip from partial to displayed, with the derived columns visible. The probe's partial rule becomes precise: a display is partial if it hides a column the step derived or joined (any column outside the input frame's 7). Middle input columns that the preview elides, as Polars' own display does, are named in the table (`input_columns_elided`), not hidden;
  - no computed result is refused or partial for P or Q;
  - usable workflows at v2 go from W2, W3, W8 to W2, W3, W4, W7, W8;
  - before and after tables are replayable at both pins, as in 0123.
- **Surface and freeze:** unchanged. This is hand-written presentation code only, and no generated binding moves.
- **Suites and launch:** the usual suites, debug last. Launch is measured against 0123; the preview is not on the startup path.

## Out of scope, reported

- `rnx eval` shows a frame as `<::polars::DataFrame>` even with the presenter registered.
- The plain `rnx-polars` binary registers no presenter.
- The pervasive `?` friction, which gets its own design pass.

## Stop rules

- Any dtype's rendering would need to format an unbounded value: render the placeholder instead, and name it.
- A structural bound (8 inspected columns, 10 rows, 80 input scalars, 8,192 output bytes) would have to be weakened, or one of its assertions relaxed, to pass: stop. Layout-specific expectations may change with the layout, and each change is named (review of the plan).
