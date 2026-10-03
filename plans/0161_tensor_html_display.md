# rnx 0161: HTML display for Candle tensors in Jupyter

**Status:** plan.

**The user (2026-10-03):** "Tensor html display." These follow 0159 (HTML tables for DataFrames) and 0160 (the Candle notebooks, where tensors still show as text).

**The cut:** a top-level Candle `Tensor` in a notebook cell also gets a `text/html` form: the same bounded corner of values the text display shows, as a small table with row and column indices. It uses 0159's path unchanged: `register_html`, the worker's `text_html` and the kernel's allowlist.

**Unchanged:**
- the text display, byte for byte;
- the prompt, `println!` and `format!`;
- every other type, including the `Dense` embedding block (a core rnx type whose display is a summary, not values).

**Out of scope:** CSS, heatmaps or colour scales, more values than the text shows, HTML for `Dense`, `TextEncoder` or other Candle types, and any core or kernel change.

## 1. Today

`adapters/candle/src/display.rs::render` (records 0129 and 0134) reads only a bounded 2-D view:
- **rank 0:** the value;
- **rank 1:** the first 8 values;
- **rank 2:** the 8 × 8 corner;
- **rank 3 to 6:** the corner of the first 2-D slice, labelled `(the [0, …] slice)`.

It reads with `narrow`, copying at most 64 values, and formats each in its dtype's own form. Text output is at most 2,048 bytes, and its `…` marks rows or columns left out.

## 2. The design

**One shared view.** `render`'s reading is factored into `view(t) -> View`:
- the header (`Tensor[f32; 64x3]`) and the slice note;
- either the corner's cell texts, plus whether rows or columns were left out, or a status (`(empty)`, `(values unavailable)` or the unsupported-dtype note).

`render` (text) and a new `render_html` both build from it. The text output is unchanged byte for byte, and existing tests pin it. Nothing beyond the corner is read.

**The markup:** no attributes, inside 0159's allowlist.
```html
<div><small>Tensor[f32; 64x3]</small><table>
<thead><tr><th></th><th>0</th><th>1</th><th>2</th></tr></thead>
<tbody><tr><th>0</th><td>0.0</td><td>-3.0</td><td>0.0</td></tr> … <tr><th>…</th><td>…</td>…</tr></tbody></table></div>
```
- **Rank 2 and the shown slice of higher ranks:** column indices in the head, a row index in each row's `<th>`, and a final `…` row or `…` column where values were left out (as in the text).
- **Rank 1:** column indices and one row, with no row index.
- **Rank 0:** a single cell, with no indices.
- **Empty, unavailable or unsupported:** the `<small>` header with the same status text, and no table.

**Escaping and bounds:**
- every header, index and value is HTML-escaped character by character;
- values are the same dtype text the text display uses;
- at most 64 values are read;
- the HTML is built from whole rows with the closing and marker reserved, as 0159's frame writer does, within the same `HTML_BYTES` usable capacity. A worst case (8 × 8 of the longest f64 text) is computed and stated in the evidence; it fits whole with a wide margin.

**Registration:** `present` adds `register_html::<Tensor>(…)` beside the text presenter. That's the adapter only, with no core, worker or kernel change.

## 3. The notebook and checker

- **`05_candle_model.ipynb` is regenerated:** its two tensor results (`x` and `y`) now store `text/html` beside `text/plain`. The notebook text, data and answers are unchanged. 06 is unchanged.
- **`check.py`:**
  - a narrow tensor-HTML reader (the `<small>` header, the indices, the values and the `…` row and column);
  - the HTML and text forms of each tensor must agree exactly (values by f32), as 0160's `check_forms` requires for frames;
  - the agreed corner must equal the recomputed expectation;
  - row and column indices must be exactly `0..n` for the shown corner.
- **New controls:**
  - an HTML tensor value changed (one f32 step);
  - an HTML index changed;
  - the HTML `…` row removed;
  - a tensor's HTML header shape changed;
  - an injected attribute (refused by the allowlist);
  - text/plain removed from a tensor result.

## 4. Tests

In `adapters/candle`:
- **Goldens:** rank 0, rank 1 (short and longer than 8), rank 2 (fitting, wider than 8, taller than 8, both) and rank 3.
- **Statuses:** empty, unsupported dtype, NaN and ±inf as text.
- **Agreement:** each case's text output is byte-identical to before, and the HTML describes the same corner.
- **Allowlist:** every output passes `rnx::present::html_allowed`. The worst case, 8 × 8 values of maximal f64 text, fits whole within the capacity.
- **Bounded reads:** a large tensor's HTML reads only the corner, through the same `narrow`. The allocation peak for a 1,000,000 × 1,000 f32 tensor is shown in the evidence.

**The browser:** rnx-bench's 0159 probe is generalized, or a sibling added, to open `05_candle_model` in JupyterLab, light and dark. It requires:
- five rendered tables (three frames and two tensors) whose DOM text equals the expectations;
- no script in any output;
- the saved notebook opened untrusted;
- a run-all.

**Cost:** the time to show a tensor before and after, and run-all for 05, as before.

## 4a. Folded in from Codex's acceptance (no new review cycle)

1. **The text renderer is preserved as it was,** not refactored. `render` is left untouched, and the HTML is built by a separate `shown` reading that mirrors it.
   - 26 cases' text output was captured *before* any change into `adapters/candle/tests/data/display_text_0161.json`, and `text_is_unchanged` holds it byte for byte.
   - The cases: ranks 0–6, an offset view and a transposed view, zero axes at the front, middle and end of rank 3, zero-length rank 1 and 2, bf16 (unsupported), f64, i64, u32, u8, f32 and f64 extremes, and the worst f64 case.
   - The quirks are kept: a zero leading axis prints only its header and slice note, and an unsupported dtype prints a values-not-shown line.
   - HTML statuses carry the same header, slice identity and reason, with no table.
2. **A calculated worst case, from the existing Debug formatting:** a rank-6 header with six 20-digit dimensions and the four-index slice note, 8 × 8 of the longest f64 Debug text (24 bytes), indices, both ellipses and tags: **2,833 bytes** (`WORST`), against a usable 16,320, below `Output`'s marker room. Generic truncation never fires.
   - The measured fixtures are a rank-2 of 2,688 bytes and a rank-6 of 2,721.
   - The extremes (finite limits, subnormals, signed zero, NaN, ±inf) keep their spelling and pass the allowlist.
   - After review (R1): every cut reserves the largest closing any state needs (`<tbody></tbody></table>`, the marker and `</div>`). The head-to-body transition had overrun a requested capacity by seven bytes. Every capacity from the smallest fragment to the whole form is now swept, for five shapes.
3. **The allocation control uses views,** not dense data: a broadcast 1,000,000 × 1,000 view and an offset, transposed 1,000 × 999,997 view of four stored values. The peak is measured after the input exists, against a computed bound of 64 KiB. Only the corner is read (`narrow`, then a copy of at most 64 values), and the large tensors are never materialized.
4. **The checker binds the tensor table's structure:** the header and full shape, column indices `0..n`, row indices `0..m`, dimensions, values, and the `…` row in its place. Both forms are parsed independently and their f32 values must agree exactly before the expected corner is compared.
   - New controls: missing and duplicated values and rows, as well as the listed mutations.
   - Another valid spelling of the same f32 is accepted.
   - The 04 and 05 frame checks and 06's Dense-only result are unchanged.
5. **The browser probe** (`probes/jupyter-html-0161`, adapted from 0159) keeps the non-empty all-untrusted assertion, the light and dark themes, and the saved and run-all checks. Each rendered table's DOM cells must equal the cells of the stored HTML it renders, which the checker verifies independently.
   - The mixed-worker verifier and controls are rerun after rebuilding the combined worker.
   - The after-push check uses a clean checkout and the documented setup, covering 05's two tensor bundles and 06 unchanged.
   - Timings are measurements only.

## 5. Copy

The notebook README and the Candle adapter's docs say tensors show as small indexed tables in Jupyter, beside the unchanged text form.
