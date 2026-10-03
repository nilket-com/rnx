# rnx 0159: HTML tables for DataFrames in Jupyter

**Status:** plan.

**The user (2026-10-03):** "first the console text edition, then the html afterwards. Work with Codex on it." The console edition is 0158. Codex's requirements for this record:
- keep `text/plain` as the fallback;
- add `text/html` for top-level DataFrame notebook results, built from frame data and not by parsing the text;
- make the smallest presenter, worker and kernel change, with ordinary script printing unchanged;
- keep the same bounds, honest omission markers and whole escape tokens;
- escape everything, with no scripts, event handlers or remote resources;
- semantic, restrained, theme-independent markup;
- independent content checks, hostile and bound controls, and a real JupyterLab/browser check.

**The cut:** the 04 sales notebook's frames render as HTML tables in JupyterLab, and the text table from 0158 stays alongside as `text/plain`.

## 1. The path today

1. A top-level value's presenter writes bounded text into `present::Output`.
2. `present::finish` makes it terminal-safe within a byte budget.
3. The worker replies `{"type":"settled", …, "text_plain": …}` (`src/worker.rs`).
4. The kernel checks `text_plain` (a string of at most 16,384 bytes, `jupyter/src/worker.rs:501`) and publishes `execute_result` with `{"text/plain": …}` only (`jupyter/src/kernel.rs:502`).

HTML needs a second, separately bounded form carried through the same path.

## 2. The core change (rnx and the kernel): additive and small

**`present::Presenters::register_html::<T>(fn(&T, &mut Output) -> Result<(), String>)`.**
- A second registry, keyed the same way, refused on duplicates the same way.
- `register`, `Extensions::present` and every existing signature are unchanged.
- An adapter's existing `present(&mut Presenters)` hook registers both, so generated project executables and `demos/polars` need no change.

**The worker,** only for a top-level value with an HTML presenter:
- **The budget:** the presenter writes into an `Output` of `HTML_BYTES` = 16,384 bytes (the existing render bound).
- **The check:** the result must pass `present::html_allowed`, a strict allowlist checker:
  - tags only from `div`, `small`, `table`, `thead`, `tbody`, `tr`, `th` and `td`, with **no attributes at all**, properly nested and closed;
  - text with no raw `<` or `>`;
  - `&` only in `&amp;`, `&lt;`, `&gt;`, `&quot;` and `&#39;`.
- **On success** it's sent as `"text_html"`; if anything fails, `"text_html": null`. The failure can't affect `text_plain`, the evaluation or the session.
- **Everything else,** whether the REPL, `rnx run`, `eval`, `println!`, or a value with no HTML presenter, behaves exactly as now.

**The kernel:**
- It accepts an optional `text_html`: null, or a string of at most 16,384 bytes. An absent field means null, so an older worker still works.
- It **re-runs the same allowlist check** (a copy in the kernel crate, tested against the same cases).
- It publishes `{"text/plain": …, "text/html": …}` only when the HTML passes; otherwise `text/plain` alone, as now.
- An older kernel ignores the field.

**No styling, scripts, `<style>`, classes or attributes are sent.** JupyterLab's own rendered-HTML table style applies, so the table follows the user's theme. Because there are no attributes, there's nothing for an event handler or a URL to live in.

## 3. The adapter: an HTML writer over the same bounded cells

`adapters/polars/src/preview.rs` gains `render_html_into`. It reuses 0158's extraction unchanged: `shown_columns`, `shown_rows`, `cell`, `dtype`, `plain` and the `cfg(test)` read counter. The text is never parsed.

**The markup:**
```html
<div><small>shape: (24, 6)</small><table>
<thead><tr><th>month</th>…</tr><tr><td>string</td>…</tr></thead>
<tbody><tr><td>2026-07</td>…</tr>…<tr><td>…</td>…</tr>…</tbody></table></div>
```
- Its shape follows Python Polars' `_repr_html_`: the shape in `<small>`, names in `<th>`, and the dtype row in the head.
- The elided row and column are `…`, at the same positions as the text table.

**Escaping, in two layers:**
1. 0124's scalar policy first: controls, bidi and line separators become `\u{…}`.
2. Then HTML escaping of `&`, `<`, `>`, `"` and `'` on the result.

Each scalar is escaped whole, and no entity is ever split.

**Bounds:**
- The same rows, columns and scalars: at most 10 rows and 8 columns are read, at most 80 scalars per cell.
- HTML output is at most 16,384 bytes, built from whole rows.
- If a row doesn't fit, the table is closed properly (`</tbody></table>`), then `<small>[preview byte limit; remainder omitted]</small></div>`. The closing tags and marker are reserved before every row, so the output is always well-formed and never looks complete when it isn't.

## 4. The notebook

- **Regenerated:** `04_polars_sales.ipynb`. Each frame cell now stores both `text/plain` (unchanged from 0158) and `text/html`. The analysis, data, answers and markdown are unchanged.
- **`check.py`:**
  - **The 0158 checks are kept:** the text parser, the CSV expectations and the controls.
  - **The HTML is checked independently** with Python's `html.parser`. A second allowlist implementation must accept it. Every frame's `<small>` shape, header names, dtype row and cells, with the elided row by position, must equal the same CSV expectations, and the `text/plain` and `text/html` of each result must describe the same frame.
  - **Stored-output comparison** covers both MIME types, as it already compares all `data`.
- **New controls:**
  - an HTML cell changed;
  - two HTML rows swapped;
  - a `<script>` and an `onerror=` attribute injected, which the allowlist refuses;
  - an HTML table with a row missing;
  - `text/html` present but `text/plain` removed, which is refused (the fallback must remain).

## 5. Tests

**In the core:**
- **The allowlist** accepts the adapter's real outputs. It refuses:
  - every disallowed tag, and any attribute, including `on*`, `style`, `href`, `src` and quoted `>`;
  - unclosed or misnested tags, comments, CDATA and `<!doctype>`;
  - stray `<` or `>`, unknown or numeric entities other than `&#39;`;
  - and anything over the budget.
- **The worker:** a test presenter producing invalid HTML yields `text_html: null`, with `text_plain` intact and the session alive. A value with no HTML presenter yields no field change.
- **The kernel:** a settled reply with valid, invalid, oversized and absent `text_html` publishes the right MIME bundle, and an older-shape reply still works.

**In the adapter:**
- **Hostile names and cells** (`<script>`, `<img src=x onerror=…>`, `</td>`, `&amp;`, quotes, `javascript:`, controls and bidi characters) come out as text only, and the core allowlist accepts the result.
- **Shape:** goldens for the user's two reference frames as HTML, empty frames, the 10/11-row and 8/9-column boundaries, and omitted hazardous columns never read (0158's controls repeated for HTML).
- **The HTML byte cap:** well-formed and closed, with the marker, including when the very first row can't fit.
- **The allocation control** extends `tests/preview_alloc.rs` with the HTML render of the hostile 1,000,000 × 1,000 frame, under a bound computed the same way and stated in the evidence.

**The browser,** in a rnx-bench probe because a browser doesn't belong in the public checker:
- JupyterLab 4.6.3 with Playwright Chromium, from rnx-bench's existing pinned `jupyter-transport` environment;
- a temporary kernelspec on the Polars worker;
- **Opened and run:** after open, run-all and wait, the 04 notebook shows five rendered `<table>` elements whose header and cell text equal the expectations, and no script runs (no console error or dialog, and the DOM has no `script` inside outputs).
- **The committed notebook opened without running** (untrusted, so JupyterLab's sanitizer applies) shows the same tables.
- **Screenshots and the DOM text** are kept as evidence.

**Costs:** worker launch and the time to show a frame, before and after, as measured in 0158, against 0068's budget. `cargo test` and the adapter suite stay green.

## 6. Copy

- **The notebook README** says frames show as HTML tables in Jupyter, with the text table as the fallback, and that rnx sends no styling or scripts.
- **The adapter README** documents the HTML form and its bounds.

## 7. Out of scope

- CSS, themes or interactive tables.
- HTML for Series, LazyFrame or any other type.
- `display_data` or other MIME types.
- Candle tensor display.
- Polariton integration.
- VS Code or other front ends, except as noted if tried.
- Any change to the text table.
