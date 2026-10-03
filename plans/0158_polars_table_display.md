# rnx 0158: a Polars-style table layout for DataFrame previews

**Status:** plan, revision 2 (after Codex's review of db67295).

**The user (2026-10-03):** "first the console text edition, then the html afterwards. Work with Codex on it." The reference is ipython with Python Polars:

```
shape: (3, 2)
┌─────┬─────┐
│ k1  ┆ k2  │
│ --- ┆ --- │
│ i64 ┆ i64 │
╞═════╪═════╡
│ 1   ┆ 4   │
│ 2   ┆ 5   │
│ 3   ┆ 6   │
└─────┴─────┘
```

**The cut:** the bounded DataFrame preview (record 0124) keeps its extraction (which rows, columns and scalars are read, and how each is escaped and bounded) and gets **a Polars-style table layout** around it. It covers the prompt, notebook cells and `frame.preview()`.

**Out of this record:** HTML (the next record), configuration (`POLARS_FMT_*` or any wider bounds), Series and LazyFrame display, and any rnx core or kernel change.

## 1. Why not Polars' own `Display` (revision 1, withdrawn)

Revision 1 proposed enabling `fmt_no_tty` and calling `impl Display for DataFrame`. A probe showed the look matches, but Codex's review found it isn't a bounded or safe replacement. Checking the environment and capping the final string doesn't fix that.

**Work and allocation happen before any cap** (`polars-core` 0.55.2 `fmt.rs`):
- it builds `fields()` for the **whole width**;
- each displayed row's `str_value` maps over **all columns**, omitted ones included, before the first and last are selected;
- structs, lists and full dtype text, nested metadata and zones included, are formatted without a shared budget;
- `catch_unwind` can't contain allocation exhaustion or stack overflow.

**Escaping and temporal handling would regress:**
- strings, categories and field names reach the table raw, without 0124's control and bidi escape policy;
- native `AnyValue` formatting re-enters the out-of-range and zoned-datetime panics 0124 avoids;
- a caught panic still runs the panic hook, which writes to stderr.

**The probe's other findings:**
- `POLARS_FMT_MAX_ROWS=-1` made a 1M-row render unbounded;
- a malformed `POLARS_TABLE_WIDTH` panics in Polars;
- some float and separator settings also have process-global setters.

So Polars' formatter isn't used. **No Polars feature, inventory or binding changes;** `fmt_no_tty` stays off. The user's samples are visual and golden references, and byte-identical output to Python Polars for every dtype or Unicode case isn't claimed.

## 2. The design

**Extraction is unchanged:** 0124's bounds and readers, the only code that touches frame data.
- **Bounds:** `ROWS` 10, `COLUMNS` 8, `SCALARS` 80 and `BYTES` 8192. No bound widens.
- **Columns:** `shown_columns` reads at most 8: all of them up to 8, otherwise the first 4 and last 4. **Omitted columns are never read.**
- **Cells:** `cell`, `dtype`, `list`, `temporal` and the `Bounded` writer keep their contracts:
  - whole-token escapes and a shared input-scalar budget across nesting;
  - checked temporal conversions with `<out of range>`;
  - a zone escaped and counted;
  - opaque `<dtype>` placeholders for anything not on the closed `fixed_width` list.
- **Rows, the one change in what is read:** like Polars, a frame taller than `ROWS` shows its **first 5 and last 5 rows** with a `…` row between, instead of the first 10. Still at most 10 rows are read, by index.

**The cell text changes in exactly two ways:**
- **Top-level strings and categories are shown unquoted, as Polars does.**
  - They're still escaped a scalar at a time with 0124's policy: backslash, `\n`/`\r`/`\t`, every other control and every line, paragraph, embedding, override or isolate character as `\u{…}`.
  - Only the surrounding quotes and the `\"` escape are dropped, so a quote character shows as itself.
  - Strings inside a list cell stay quoted, as Polars shows them.
  - Column names follow the same unquoted, escaped rule.
- **A cut scalar ends in `…` rather than `…[truncated]`,** after the same 80-input-scalar bound. That's Polars' marker. A cut cell is told apart from an elided column's cell because it has content before the `…`.

**Kept spellings:** the dtype row keeps 0058/0124's names (`string`, `i64`, `f64`, `bool`, `list[…]`, …). Polars writes `str`; renaming is a contract change left for its own record.

**The layout** is a small bounded writer of our own, with no comfy-table. It reproduces Polars' default `UTF8_FULL_CONDENSED` look:
1. `shape: (H, W)`, with Polars' `_` thousands grouping (`(1_000_000, 4)`).
2. `┌─┬─┐`, then a name row, `---`, a dtype row, `╞═╪═╡`, the data rows and `└─┴─┘`. Rows are separated by `│` at the edges and `┆` inside, with one space of padding.
3. Each column's content width is the largest display width of its name, `---`, its dtype and its shown cells. That's at least 3, which matches the sample's `│ k1  │`.
4. An elided column is a `…` column, with `…` in the name and data rows and blanks in the `---` and dtype rows. An elided row is a `…` row.
5. Everything is **left-aligned, numbers included, as Polars' default is.**
6. **Display width:** `unicode-width` 0.2.2 (already in the lock through Polars) becomes a direct, pinned dependency. Wide East Asian characters count 2 and combining marks 0. Escapes are ASCII, so every control and bidi character has a known width.

**Bounds on the intermediate:**
- Cell texts are computed first, at most (`ROWS` + 3) × (`COLUMNS` + 1) of them, each at most 80 input scalars, at most 10 bytes per escaped scalar, plus the marker.
- So the cells take under 128 KiB in the worst case, and the padded lines are emitted one at a time.
- The evidence states the bound computed from the code, and a test measures it with the allocation-peak hook.

**The byte cap and partial output:**
- Each table line is pushed through the existing `Capped` writer as **one whole token**.
- If a line doesn't fit, the existing `[preview byte limit; remainder omitted]` marker follows, and the bottom border is never written. A cut table can't look complete, and no escape or line is split.
- The session presenter's outer budget keeps its own marker, as now.

**Removed:** the header `DataFrame: R rows × C columns` and the `[N rows and M columns omitted by display limits]` line. The shape gives the true dimensions, and the `…` row and column show where the elisions are. Omission counts can be derived from the shape and the bounds.

**Unchanged:**
- **No environment variable is read.** Nothing new can panic: the layout does arithmetic on bounded widths, and every cell comes from the unchanged readers.
- Series, LazyFrame and other presenters, `frame.schema()`, every API, and the session's presenter contract.

## 3. Tests

**Golden references:**
- the user's two ipython frames, built in Rust, render byte for byte equal to the pasted text, through `present` and through `preview()`;
- the 24-row sales frame shows its first 5 and last 5 rows;
- a 12-column frame shows 4, `…`, then 4.

**Bounds and safety (structural, with no wall-clock assertions):**
- **Hostile text:** names and cells with every escaped class (controls, bidi isolates and overrides, line and paragraph separators, backslashes, quotes) show only escapes.
  - A test checks that no raw control or bidi character reaches the output.
  - Escapes are never split, including at the 80-scalar cut and at the byte cap.
- **Zones:** a hostile zone in a datetime dtype is escaped and counted in the dtype row.
- **Omitted hazardous columns:** a 12-column frame whose omitted columns 5–8 hold an out-of-range datetime, a deep nested list, a huge string and a zoned datetime renders without reading them. A reader-count test hook, `cfg(test)` only, proves it, along with the allocation peak.
- **Out-of-range temporals** show `<out of range>` and don't panic.
- **Deep and wide nesting:** 0124's deep and long nested lists (deeper than 80 levels, longer than 80 elements) are bounded by the shared budget, visiting at most 80 levels.
- **Width:** CJK and combining-mark cells align by display width.
- **The byte cap:** a frame whose lines overflow `BYTES` ends with whole lines, then the marker, and has no bottom border.
- **Allocation:** in the `dense_alloc` pattern (one test per binary, so no parallel test moves the peak), rendering the hostile frames above and a 1,000,000 × 1,000 frame stays under the computed intermediate bound.
- **The existing preview and presentation tests** are carried over to the new layout. Every bounds assertion is kept, and only expected text changes.

**The 0157 notebook:** `04_polars_sales.ipynb` is regenerated. `check.py`'s parser reads the box layout (shape, borders, names, `---`, dtypes, rows, the `…` row and column), with the same exactness rules and controls as 0157. The analysis, data and answers are unchanged.

**Cost, measured and disclosed, not asserted in tests:**
- stock `rnx` start-up and Polars-worker launch, before and after, in 0068's method and ±1% budget;
- showing the sales frame and a 1M × 1,000 frame, with the shape, machine and method stated, against 0068's ≤ 0.15 ms.

`cargo test`, the oracle and `git diff --check` stay green.

## 3a. Folded in from Codex's acceptance of revision 2 (no new review cycle)

1. **Unquoted text is written at the input-scalar writer** (`plain`, via `escaped_into(…, quoted: false)`), not by stripping a quoted string. A backslash is always escaped, so a backslash and quote (`\\"`), a quote (`"`), a backslash and `n` (`\\n`) and a newline (`\n`) all stay distinct. Lists and bytes keep their quoting, and truncation stays whole-token. Tests cover each ambiguous input.
2. **All live intermediates are counted,** not just cell payloads: string and vector capacities, names, dtypes, widths, one padded line or border at a time (box-drawing characters are 3 bytes each) and the output. The bound is computed from the code and measured with the allocation peak after the frame exists. The marker-and-no-bottom-border rule also covers the very first border.
3. **Empty frames are defined:** 0 × 0 and a zero-column frame of any height render `┌┐ ╞╡ └┘`, and zero rows with columns renders the header and both borders, as Polars does. Tests cover 1 column, exactly 10 rows, 11 rows, exactly 8 columns and 9 columns, and none reads a nonexistent cell. The notebook checks expect the head and tail selection.
4. **One width API:** `UnicodeWidthStr::width` on the exact emitted text. There's a CJK, combining-mark and ZWJ-emoji control. Alignment is claimed for unicode-width's model only, and everything is left-aligned.
5. **`…` is a display convention.** The checker identifies row elision structurally, from the shape and position, and refuses a `…` anywhere else. A literal `…` name or cell renders as itself, with a control on each side.

## 4. Copy

- **The notebook README's Polars section** shows the new table and says the display is a Polars-style layout over rnx's bounded preview, not Polars' own formatter, with no `POLARS_FMT_*` configuration.
- **The adapter's module docs** are updated to match.

## 5. Out of scope

- HTML tables and `text/html` in the kernel (the next record).
- `POLARS_FMT_*` or any wider bounds.
- Renaming `string` to `str`.
- Series display.
- Terminal-width detection.
- Any rnx core or kernel change.
