# rnx 0124 evidence: the preview shows every dtype, and keeps a frame's last columns

**The change** is to the hand-written preview only (`adapters/polars/src/preview.rs`). It is shared by `preview()`, `println!` and the session presenter.
- **Every dtype previews (family P).** "unsupported column dtype" no longer exists. A count (`u32`), a date, a weekday (`i8`), a rank or a list now show, within the same bounds.
- **Past 8 columns, the first 4 and the last 4 are shown (family Q),** with a `…` column between them, as Polars' own display does. A workflow's derived columns, appended last, are always visible.

**The measure (v2).** Every computed frame now displays both ways (34 of 34, up from 24), and the usable workflows go from **W2, W3, W8** to **W2, W3, W4, W7, W8**.

**Replay.** `probes/0124/replay.sh` builds the before state (the committed tree with 0123's `preview.rs`, 7629cd3) and the after state at both pins. It probes each with display and byte-compares the four tables with the committed ones. No generated binding moves, so no generator worktree is needed.

## 1. Every dtype, bounded (family P)

**Column dtypes** are named by a bounded, early-stopping formatter (review of the plan):
- `string`, `i64`, `f64` and `bool` keep 0058's spelling.
- `List` recurses as `list[…]` and stops at the scalar bound. A 2,000-deep nested `List` dtype is named within the bound (test `a_deep_nested_dtype_is_named_within_the_bound`).
- Struct, Array, Object, Categorical, Enum and any other nested dtype get a fixed name. Their fields and categories are never formatted.
- A closed list of fixed-width dtypes shows Polars' own `Display` (`u32`, `date`, `datetime[ms]`, `duration[ms]`, `time`). A zoned datetime copies its zone only up to the bound, an extension is `ext`, and anything unlisted is `opaque` (round 1, below).

**Cells.** Each takes its column's dtype by reference: a value's own `dtype()` would rebuild a struct's field list for every cell. Each is rendered within the scalar bound:
- **Integers of every width, f16, decimals and durations:** Polars' own fixed-width text. f32 and f64 use the shortest round-trip form, as f64 did.
- **Date, datetime and time:** Polars' checked conversions of the physical integer, the same text as Polars in range. Out of range, `<out of range>`. A zoned datetime is its UTC instant marked `UTC` (round 1, below).
- **Categorical and Enum:** the category's string, quoted and escaped.
- **Binary:** `b"…"`, each byte escaped, cut at 80 bytes.
- **List:** `[1, 2, 3]`, bounded in input characters across every nesting level, with every escape and token whole (round 3, below).
- **Struct, Array and Object:** a fixed `<struct>`, `<array>` or `<object>` placeholder. Their `AnyValue` variants are feature-gated and can't be named at 0.55.2.

**Found in implementation (self-review).** The first `list` rendering visited each list with its own budget (round 3 replaced the shared buffer described here with an input-character budget). A list nested deep inside a list cell grew a few characters per level and recursed once per level, so a deep enough value passed the bound. The shared buffer fixes it. The test builds a list nested 200 levels deep (deeper than the 80-level budget); it shows `[[[[…` and costs one bound. A deeper fixture is not needed, and building one in Polars is slow (2,000 levels took 88 s).

## 2. The first and the last columns (family Q)

`shown_columns(width)`: every column up to 8. Beyond that, columns 0 to 3, a `None` marker, and the last 4. The header shows `…` for the marker, and so does each row. The omission line is unchanged, and rows are unchanged (the first 10).

**The structural bounds are unchanged:** 8 inspected columns, 10 rows, 80 input scalars per cell and 8,192 output bytes. Every assertion of them stays: the long-scalar test, the byte boundary, and the 200,000 × 40 frame's `huge.1 == ROWS * COLUMNS` inspected cells.

**Layout-specific expectations that changed**, each named (review of the plan):

| test | 0058 / 0122 expectation | now |
|---|---|---|
| `preview.rs` `dimensions_and_row_column_boundaries` | `column8` absent | past 8 columns: `column0` and the last column present, `column4` absent, the ` \| … \| ` marker present; at 8 or fewer, no marker |
| `preview.rs` `inspection_is_bounded_by_structure_not_frame_size` | tokens `small + 1`; bytes `< small + 100` | tokens `small + 1 + 2 × (ROWS + 1)` (a separator and a `…` on the header and each shown row); bytes `< small + 100 + (ROWS + 1) × (" \| " + "…")`, exactly the marker's bytes on top of the old slack |
| `preview.rs` `null_strings_and_numeric_spelling` | `cell(value)` returned `Result` | `cell(value, &dtype)` returns `String`; the same spellings |
| `tests/presentation.rs` (the wide CSV) | omission line, and `c8` absent | the same omission line; `"c3": i64 \| … \| "c8": i64`, `c11` shown, `c4` and `c7` absent |
| `tests/collect_dtypes.rs` | `a_count_collects_as_uint32_and_preview_still_refuses_it` (0122 documented the gap) | `a_count_collects_as_uint32_and_previews`: the preview contains `"n": u32` |

**New tests in `preview.rs`:**
- `every_dtype_renders_within_the_bound`: all integer widths, f32, binary, null, date, datetime, duration and time through the real render, plus the legacy names and a list cell.
- `a_huge_list_cell_is_bounded`: a million elements, and a list nested 200 levels deep, each costing one bound.
- `a_deep_nested_dtype_is_named_within_the_bound`.

They pass at both pins (14 of 14 with the review controls of rounds 1 to 3, at 0.55.2 and at v2).

## 3. The probe (`probes/0124`, from 0123's)

**The partial rule is now precise.** A display is partial if it hides a column the step derived or joined, that is, any column outside the input frame's 7. The hidden set is exact: the result's columns come from its oracle text (`columns=[…]`), minus the header's names, and must equal the omission line's count (asserted). Middle input columns that the preview elides, as Polars' display does, are recorded by name (`input_columns_elided`) and are not hidden.

**`--fixed`** now also takes display families. A fixed display family must no longer refuse or cut any computed frame (asserted).

| pin, state | steps (works / blocked, + presented) | workflows compute | frames displayed both ways | partial | usable workflows |
|---|---|---|---|---|---|
| v2, before (0123's preview) | 33 / 5, +1 | 5 of 8 | 24 of 34 | 3 | W2, W3, W8 |
| v2, after | 33 / 5, +1 | 5 of 8 | **34 of 34** | 0 | **W2, W3, W4, W7, W8** |
| 0.55.2, before | 24 / 14, +1 | 2 of 8 | 19 of 23 | 2 | W2, W3 |
| 0.55.2, after | 24 / 14, +1 | 2 of 8 | **23 of 23** | 0 | W2, W3 |

At 0.55.2 there are no Rust twins, so "compute" there means the step ran, as in 0123.

**What flipped at v2:**
- **P, refused → displayed:** `w1.csv_bytes`, `w2.null_count`, `w4.parse_date`, `w4.temporal`, `w5.lazy_group_by`, `w7.rank`, and W4.
- **Q, partial → displayed:** `w6.join_eager` and `w6.join_lazy` (the joined `target` was hidden; now only the input `qty` is elided), and W7 (`running`, `roll` and `rank` were hidden; now only the input `qty`, `price` and `note` are elided).

**W4 as a user sees it** (v2, `println!`, rows 1 to 3 of 9):

```
DataFrame: 9 rows × 11 columns
[0 rows and 3 columns omitted by display limits]
"id": i64 | "date": date | "region": string | "product": string | … | "weekday": i8 | "product_uc": string | "size": string | "revenue": f64
1 | 2026-01-03 | "north" | "apple" | … | 6 | "APPLE" | "small" | 3.75
2 | 2026-01-03 | "south" | "pear" | … | 6 | "PEAR" | "small" | null
3 | 2026-01-04 | "north" | "pear" | … | 7 | "PEAR" | "big" | 12.5
```

**No display gap remains in the probe.** The display ranking is empty: nothing is refused or partial, and nothing is unattributed.

## Review round 1 (Codex): zoned datetimes, and temporal panics

**Finding (blocking).** A zoned `Datetime` is not fixed-width:
- the dtype fell through to Polars' `Display`, which writes the whole zone (`dtype.rs:1229`), and a zone can be arbitrarily long;
- a zoned cell's `to_string()` reaches `PlTzAware::fmt`, which panics ("activate 'timezones' feature") in the 0.55.2 build.

**The fix, wider than the finding.** Checking every `DataType` `Display` arm at both pins found two more unbounded ones: `Extension` (`dyn_display()`), and `Map` at v2 (it recurses). Checking the temporal cells found that Polars' `Display` for **any** date, datetime or time value `expect`s it in range (`temporal_conversions.rs:47`, `162`, `175`, `188`, `133`). So an out-of-range date or timestamp panicked the preview, zoned or not.
- **Dtype names:** only a closed `fixed_width` list reaches Polars' `Display`. A zoned datetime writes `datetime[unit, ` and then at most 80 characters of the zone. An extension is `ext`; `Map` is `nested`; anything unlisted is `opaque`.
- **Cells:** date, datetime and time go through `temporal()`. It reads the physical integer (`AnyValue::extract`, so no feature-gated variant is named) and converts it with the checked `_opt` conversions. In range the text is the same as Polars'. Out of range it is `<out of range>`. A zoned value is its UTC instant marked `UTC`, identical at both pins; the header names the zone. Anything not fixed-width and not handled above shows `<dtype>` and is never formatted.

**Controls (both pins):**
- `a_zoned_datetime_is_bounded_and_never_panics`: a million-character zone (`TimeZone::new_unchecked`, covering any zone Polars may carry) is named within the bound. A Europe/Paris column (built as a logical column, because a cast from integers drops the zone) shows `datetime[ms, Europe/Paris]` with cells `1970-01-01 00:00:00 UTC`, `null` and `2023-11-14 22:13:20 UTC`.
- `out_of_range_temporal_values_never_panic`: date `i32::MAX` and datetime `i64::MAX` ms show `<out of range>`. Polars itself nulls an out-of-range time when the column is built (`into_time`), so the time path is a defence that no public constructor reaches at 0.55.2.
- **Mutation:** with `temporal()` and the zone arm disabled, both controls fail. The long zone breaks the bound, and the date panics inside Polars (`temporal_conversions.rs:47:30`). Restored, 12 of 12 pass at 0.55.2 and at v2.

**Found while verifying round 1: two of Polars' `_opt` conversions are not fully checked** (the same source at both pins).
- `date32_to_date_opt` adds `EPOCH_DAYS_FROM_CE` (719,163) to the day number unchecked. The debug suite caught it: date `i32::MAX` panicked with "attempt to add with overflow" (`temporal_conversions.rs:53:42`). In release it wraps, and happens to return `None`.
- `time64ns_to_time_opt` casts the seconds to `u32` unchecked. So a huge nanosecond value wraps into a plausible, wrong time: `4294968296000000000` showed as `00:16:40`.

`temporal()` now range-guards both before calling Polars: a day number at most `i32::MAX − EPOCH_DAYS_FROM_CE` (Polars' own public constant), and a time within one day. The datetime conversions are checked (`TimeDelta::try_milliseconds`, and `checked_add_signed` for every unit).

**Guard controls** (`out_of_range_temporal_values_never_panic`, calling `temporal()` directly with plain integers, because Polars won't build an out-of-range time column):
- dates `i32::MAX − 719,163 + 1` and `i32::MAX` show `<out of range>`;
- times −1, 86,400 s, `4294968296000000000` and `i64::MAX` show `<out of range>`;
- noon shows `12:00:00`.

**Mutation:** without the time guard, `4294968296000000000` shows `00:16:40`. The 12 preview tests pass in release and in debug, at 0.55.2 and at v2.

**Unchanged:** every probe table (the replay byte-compares them), because in-range unzoned temporal text is identical.

## Review round 2 (Codex): a zone's characters are escaped

**Finding (blocking).** The zone arm copied the zone's characters raw. A hostile zone (the 0.55.2 `opt_try_new` accepts any string) could put a raw ESC, a newline, a tab or a bidi control into the header, which 0058's escaping contract forbids. A later cut must also not split an escape.

**The fix:**
- **Shared escaping.** `quoted`'s per-character escaping is now `escaped_into`, shared by names, string cells and zones: one policy.
- **Dtype names are bounded in input characters, as quoted strings are.** A `Name` builder writes at most 80 input characters, each whole, then `…[truncated]` once, and stops descending. A zone's characters go through `escaped_into` and count against the same budget. There is no output-side cut left, so an escape can never be split. Output is bounded by at most 80 characters, each escaped to at most 10, plus the marker.
- **Normal output is unchanged:** `datetime[ms, Europe/Paris]`, the legacy four names, `list[…]`, and the UTC cell policy.

**Control** (`a_hostile_zone_is_escaped_whole_within_the_bound`, both pins):
- the zone `ESC[2J, newline, "evil", tab, U+202E "rev", U+2066, CR, quote, backslash` names as `datetime[ms, \u{1b}[2J\nevil\t\u{202e}rev\u{2066}\r\"\\]`;
- through `render()`, the frame's text holds no raw ESC, tab, CR, U+202E or U+2066, and is exactly 3 lines (the zone's newline added none);
- a zone of 10,000 ESC characters names as the prefix, then exactly `80 − 13` whole `\u{1b}` escapes, then the marker once.

**Mutation:** with `push_escaped` replaced by a raw copy, the control fails (raw controls in the name).

**Results:**
- 13 of 13 preview tests pass in release and debug at 0.55.2, and at v2.
- The 0.55.2 suites pass, with no warnings: release 25, test-support 236, debug 237.
- The replay is byte-identical.
- The oracle is unchanged apart from the two standing unordered flips, which were restored.

## Review round 3 (Codex): list cells keep escapes whole

**Finding (blocking).** `list_into` appended each element's already-escaped text, and `list()` then cut the whole at output character 80. So the cut could split an escape:
- a one-element list of U+202E strings cut `\u{202e}` after 6 characters;
- a binary list of zero bytes cut `\x00` after its first character.

**The fix.** Round 2's `Name` builder is now a general `Bounded` writer, bounded in input characters, used by dtype names and list cells:
- an opening bracket, each separator character, each string character (escaped through `escaped_into`) and each byte (escaped as `blob` does) costs 1;
- a string's closing quote is always written, before the marker if the budget ends inside it;
- every other element (a number, a date, a placeholder) is a token, shown whole or not at all;
- when the budget is spent, the marker is written once and nothing after it.

**Audit of every remaining path into the text** (`render_into`):
- the title and the omission line are numbers;
- a column name is `quoted` (input-bounded, escapes whole);
- a dtype name, and a placeholder's name, is `Bounded`;
- a cell is `quoted`, `blob` (input-bounded), `list` (`Bounded`), a temporal token, or `bounded(to_string())`.

That last is the only output-side cut left, and it applies only to `fixed_width` scalars (numbers, decimals, durations). Their text contains no escapes, because strings and bytes are matched before it.

**Control** (`a_list_cell_keeps_every_escape_and_token_whole`, both pins, release and debug):
- a U+202E string list is `["` then exactly 78 whole `\u{202e}`, then `"…[truncated]`; one level deeper, 77;
- a zero-byte binary list is `[b"` then 78 whole `\x00`, then `"…[truncated]`; nested, 77;
- 100 × `1234`: 13 whole tokens, and the 14th, which doesn't fit, is not shown as `1`;
- a short list with escapes is shown whole, with no marker.

The earlier million-element and 200-deep list controls still pass.

**Mutation:** restoring the output-side cut (an unbudgeted rendering, then cut at 80) fails the control with Codex's example, `["\u{202e}…\u{202…[truncated]`.

**Results:**
- 14 of 14 preview tests pass in release and debug at 0.55.2, and at v2.
- The 0.55.2 suites pass with no warnings: release 26, test-support 237, debug 238.
- The replay is byte-identical.
- The oracle is unchanged apart from a standing unordered `unique` flip, which was restored.

## Surface, freeze, oracle

- **Surface and freeze:** unchanged. No generator or release file is touched (`git status` is clean under `tools/` and `probes/0108/`), so both pins generate exactly 0123's bindings.
- **Oracle:** unchanged. The debug run's only movements are the two unordered `unique` flips, within the approved policy, and the committed results are kept.

## Suites and launch

- **Production suites (0.55.2):** release default (22), release test-support (233) and debug generated plus test-support (234) all pass. The first run failed only the two layout expectations named above, and both pass after their update.
- **v2 build (`probes/0108/build.sh`):** every stage ok; `oracle: cases_failed` is the standing state. `no_default` passes, and the preview tests pass at v2.
- **Launch** against 0123 (7629cd3), default release builds, three rounds of 60 interleaved launches: median deltas −0.60, +1.25 and −0.31 ms, so no change. The preview is not on the startup path (`launch-results-*.json`).

## Next

- **The display gap in the probe is closed.** The remaining ranking is computation:
  - G, the eager group-by;
  - B, C and D, loading (CSV schema inference, the JSON reader, a path type);
  - H and I, pivot.
- **Presentation outside the probe:** `rnx eval` shows `<::polars::DataFrame>`; the plain `rnx-polars` binary registers no presenter.
- **The `?` friction** remains out of scope, for its own design pass.
