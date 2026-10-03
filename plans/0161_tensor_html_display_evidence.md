# rnx 0161 evidence: HTML display for Candle tensors

**Status:** implemented, for review.

## Source and environment

- **Checked at `535793c`,** with the review fix (R1) amended in: this commit's tree adds the reserve fix and its sweep test in `display.rs`, and this file.
- **The browser check** is rnx-bench (`probes/jupyter-html-0161`, `results/jupyter-html-0161`), rerun on the rebuilt worker after the fix.
- **Environment:** rustc 1.98.1 and cargo 1.98.1 on Linux 7.0.0-31-generic x86_64. The checker ran in `demos/notebooks/requirements.txt` (Python 3.14.4), with `-W error::RuntimeWarning`; the browser check in rnx-bench's pinned JupyterLab 4.6.3 and Playwright Chromium.
- **The combined worker** was rebuilt from this tree: `demos/candle/target/release/rnx-candle-demo`, sha256 below. Its hash is the one the browser check's journal records. Root `rnx` and `rnx-jupyter` are unchanged since 0159.

```
cc0e576ba9a22634405f3d5f3ba608893bdf1af683c1496dad08dd0ca70c90c9  demos/candle/target/release/rnx-candle-demo
c7d28e51864d8d6c045cb60a12f8af59a13cf9f9f91c5300b10547d27925085a  adapters/candle/tests/data/display_text_0161.json
```

## The text display is unchanged (section 4a, item 1)

- **`render` is untouched.** The HTML is built by `shown`, a separate reading that mirrors it.
- **Before any change,** the 26 cases' text was captured with `print_text` into `tests/data/display_text_0161.json`. `text_is_unchanged` compares every case byte for byte.
- **The captured quirks, kept as they were:**
  - `zero-front` is `Tensor[f32; 0x3x4] (the [0] slice)` with no `(empty)`;
  - `zero-middle` is `… (the [0] slice) (empty)`;
  - `unsupported-bf16` is a `(BF16 values not shown)` line.

## Tests (`adapters/candle`, `cargo test --offline --no-fail-fast`): 21 binaries, 102 passed, 0 failed

- **`html_shows_what_the_text_shows`,** for every case:
  - the HTML passes `rnx::present::html_allowed` and stays inside capacity with no omission marker;
  - its `<small>` equals the text's first line;
  - status and header-only cases have no table, and the unsupported note is the same line;
  - for value cases, the column indices are `0..n` plus `…`, the row indices `0..m`, the values equal the text's cell for cell, and the `…` row is in its place.
- **`html_goldens`:**
  - exact HTML for rank 0, a long rank 1, a fitting rank 2, the zero-front and zero-middle statuses, and the unsupported note;
  - the rank-6 header and slice note;
  - the f32 extremes spelled `3.4028235e38`, `1.1754944e-38`, `1e-45`, `-0.0`, `NaN`, `inf` and `-inf`.
- **`html_bounds_and_cuts`,** after review:

```
rank-2 fixture 2688 bytes, rank-6 fixture 2721 bytes, calculated worst 2833, capacity 16320
```

  - The calculated worst (`WORST`: a rank-6 header with six 20-digit dimensions and the slice note, 8 × 8 of the 24-byte longest f64 Debug text, indices, ellipses and tags) is 2,833 bytes. Both measured fixtures fit under it whole, with no marker.
  - **R1:** the cut reserve was `</tbody></table>` plus the marker, but a cut right after the head emits `<tbody></tbody>`, overrunning by seven bytes (Codex: a 12 × 12 f64 at capacity 246 gave 253 bytes).
  - The reserve is now `<tbody></tbody></table>` plus the marker and `</div>`. The test sweeps **every** capacity from the smallest fragment to the whole form, for five shapes (rank 2, rank 6, a 12 × 12 of zeros, rank 1 and rank 0); each result is within capacity, allowed, closed, marked, and a prefix of the whole form.
  - With the old reserve temporarily restored, the sweep fails at exactly `246: 253 bytes`.

- **`tests/display_alloc.rs`,** one test, with the peak taken after the input exists. Both inputs are views of four stored values, never materialized (a dense equivalent is 4,000,000,000 bytes):

```
broadcast 1,000,000 x 1,000 (a view of 4 values): peak 5608 bytes above base; text 445 bytes, HTML 1351 bytes
offset transposed 1,000 x 999,997 (a view of 4 values): peak 5608 bytes above base; text 444 bytes, HTML 1350 bytes
```

  The computed bound is 64 KiB. Each form shows the 8 × 8 corner, with `…` for the left-out rows and columns.

## The notebook and checker

- **`05_candle_model.ipynb` was regenerated** (`--generate --candle --only 05_candle_model`). Its `x` and `y` results gain `text/html`; nothing else changed. 06 is untouched.
- **`check.py`:** `parse_tensor_html` reads the header and shape, the column indices, the row indices, the f32 values and the elided row. Both forms must agree exactly (by f32 value) before the recomputed corner is compared.
- **Results:** all 13 notebook-and-worker pairs pass. The demo tree's digests were taken before and after; verification and the controls wrote nothing.
- **The controls give 57 verdicts,** all as stated. The 10 new ones:
  - an HTML value one f32 step away;
  - the same value in another spelling (accepted);
  - a row index changed;
  - a column index changed;
  - the elided row removed;
  - a row duplicated;
  - a value missing;
  - the header shape changed;
  - an attribute injected;
  - HTML without its text/plain.

```
$ check.py --polars --candle
ok: 01_mortgage on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on plain rnx: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone)
ok: 03_report on plain rnx: 2 runs match the stored outputs and the terminal reference
ok: 04_polars_sales on the Polars worker: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone); every frame and answer matches sales.csv
ok: 01_mortgage on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 03_report on the Polars worker: 2 runs match the stored outputs and the terminal reference
ok: 05_candle_model on the Candle worker: 3 runs match the stored outputs and the terminal reference (one kernel restarted: new pid, earlier state gone); every result matches the recomputed model
ok: 06_candle_search on the Candle worker: 2 runs match the stored outputs and the terminal reference; every ranking matches the sentence-transformers reference
ok: 01_mortgage on the Candle worker: 2 runs match the stored outputs and the terminal reference
ok: 02_orders on the Candle worker: 2 runs match the stored outputs and the terminal reference
ok: 03_report on the Candle worker: 2 runs match the stored outputs and the terminal reference
ok: 04_polars_sales on the Candle worker: 2 runs match the stored outputs and the terminal reference; every frame and answer matches sales.csv
exit 0
$ check.py --polars --candle --controls
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
pass: two rows swapped in the region frame: by_region: text/plain and text/html show different frames
pass: an elided row where the region frame must be whole: 5 rows shown of 4
pass: a literal … cell where no row is elided: not a simple string cell: '…'
pass: a duplicated row in the region frame: 5 rows shown of 4
pass: a non-finite mean: not a finite float cell: 'NaN'
pass: one HTML cell changed: by_region: text/plain and text/html show different frames
pass: two HTML rows swapped: by_region: text/plain and text/html show different frames
pass: an injected <script>: no allowed HTML form: '<div><small>shape: (4, 4)</small><table><thead><tr><th>region</th><th>units</th><th>revenue_cents</th><th>orders</th></tr><tr><td>string</td><td>i64</td><td>i64</td><td>u32</td></tr></thead><tbody><tr><td><script>alert(1)</script></td><td>16</td><td>38750</td><td>6</td></tr><tr><td>west</td><td>19</td><td>31250</td><td>5</td></tr><tr><td>south</td><td>14</td><td>27000</td><td>5</td></tr><tr><td>east</td><td>12</td><td>25750</td><td>4</td></tr></tbody></table></div>'
pass: an injected onerror attribute: no allowed HTML form: '<div><small>shape: (4, 4)</small><table><thead><tr><th>region</th><th>units</th><th>revenue_cents</th><th>orders</th></tr><tr><td>string</td><td>i64</td><td>i64</td><td>u32</td></tr></thead><tbody><tr><td onerror=alert(1)>north</td><td>16</td><td>38750</td><td>6</td></tr><tr><td>west</td><td>19</td><td>31250</td><td>5</td></tr><tr><td>south</td><td>14</td><td>27000</td><td>5</td></tr><tr><td>east</td><td>12</td><td>25750</td><td>4</td></tr></tbody></table></div>'
pass: an HTML row missing: 3 HTML rows shown of 4
pass: text/html without its text/plain fallback: an HTML result without its text/plain fallback
pass: 05 unaltered against the recomputed model: accepted
pass: 06 unaltered against the reference: accepted
pass: 05 against a changed fc2.bias: result 3: tensor (64, 1) [[0.765625], [0.640625], [0.484375], [0.51953125], [0.36328125], [0.40625], [0.703125], [0.9296875]], expected (64, 1) [[1.015625], [0.890625], [0.734375], [0.76953125], [0.61328125], [0.65625], [0.953125], [1.1796875]]
pass: the changed weights refused by their pinned hash: mlp.safetensors differs from the bundled asset
pass: a shown score changed (05): scored: text/plain and text/html show different frames
pass: a tensor shape changed (05): result 1: the tensor's text/plain and text/html differ
pass: an HTML tensor value changed by one f32 step (05): result 1: the tensor's text/plain and text/html differ
pass: the same tensor value in another valid spelling in HTML (05): accepted
pass: an HTML tensor row index changed (05): tensor HTML row 1: ['9', '0.25', '-2.0', '0.5']
pass: an HTML tensor column index changed (05): tensor HTML column indices: ['', '0', '1', '3']
pass: the HTML tensor's elided row removed (05): tensor HTML has 8 rows for shape (64, 3)
pass: an HTML tensor row duplicated (05): tensor HTML has 10 rows for shape (64, 3)
pass: an HTML tensor value missing (05): tensor HTML row 0: ['0', '0.0', '-3.0']
pass: the HTML tensor header shape changed (05): result 1: the tensor's text/plain and text/html differ
pass: an attribute injected into the tensor HTML (05): no allowed tensor HTML: '<div><small>Tensor[f32; 64x3]</small><table onclick=x><thead><tr><th></th><th>0</th><th>1</th><th>2</th></tr></thead><tbody><tr><th>0</th><td>0.0</td><td>-3.0</td><td>0.0</td></tr><tr><th>1</th><td>0.25</td><td>-2.0</td><td>0.5</td></tr><tr><th>2</th><td>0.5</td><td>-1.0</td><td>1.0</td></tr><tr><th>3</th><td>0.75</td><td>0.0</td><td>0.125</td></tr><tr><th>4</th><td>1.0</td><td>1.0</td><td>0.625</td></tr><tr><th>5</th><td>1.25</td><td>2.0</td><td>1.125</td></tr><tr><th>6</th><td>1.5</td><td>3.0</td><td>0.25</td></tr><tr><th>7</th><td>1.75</td><td>-3.0</td><td>0.75</td></tr><tr><th>…</th><td>…</td><td>…</td><td>…</td></tr></tbody></table></div>'
pass: a tensor's text/html without its text/plain (05): an HTML result without its text/plain fallback
pass: a tensor value changed (05): result 3: the tensor's text/plain and text/html differ
pass: two ranked rows swapped (06): question 0: text/plain and text/html show different frames
pass: a ranked row missing (06): 2 rows shown of 3
pass: a ranked row duplicated (06): 4 rows shown of 3
pass: an HTML-only score change inside the reference tolerance (06): question 0: text/plain and text/html show different frames
pass: a text-only score change inside the reference tolerance (06): question 0: text/plain and text/html show different frames
pass: the same score in another valid spelling in HTML (06): accepted
pass: an HTML ranked title changed (06): question 0: text/plain and text/html show different frames
pass: a shown reference score shifted by 2e-5: question 0: rows differ:
pass: a reference made from another model revision: search-reference.json was made from a different model
pass: a reference ranking with a duplicate article: question 1: the ranking is not every article exactly once
pass: questions.csv changed after the reference was made: search-reference.json was made from a different questions.csv
pass: the model directory missing (names the setup command): model file config.json is missing from /tmp/rnx-0160-ref-iesut6jw/no-model: run sh demos/candle/fetch-model.sh from the checkout root
exit 0
demos unchanged by verify and controls
```

## The browser (rnx-bench `ebe406a`)

For each of the light and dark themes:
- the committed `05_candle_model.ipynb`, opened untrusted (all seven code cells `trusted: false`, asserted), shows 5 rendered tables: 3 frames and 2 tensors;
- each table's DOM cells (`th` and `td`, head and body) equal the cells of the stored `text/html` it renders;
- run-all shows them again, and the saved bundles are `[html+plain, html+plain, plain, html+plain, html+plain, html+plain]`;
- no output has a script.

The console noise is the same two named allowances as 0159. Screenshots are in `results/jupyter-html-0161/`.

## After review (R1)

The worker was rebuilt (`cc0e576b…`). All 13 notebook-and-worker pairs and the 57 controls pass again, the demo tree is unchanged, the Candle suite passes 102 of 102, and the browser check passes again in both themes on this worker hash. Notebook outputs are unchanged: at the real capacity nothing is cut.

## Cost, measured and not asserted

**Showing a tensor:** a release build, one core, 20,000 displays per input, three rounds, through `display_forms`:
- 64 × 3: 6.7–7.0 µs for text and HTML together (163 + 647 bytes);
- the 1,000,000 × 1,000 view: 14.4 µs (445 + 1,351 bytes).

The text path is unchanged.

**`05_candle_model` run-all,** kernel start included, warm, 5 runs: median 0.54 s (0.49–0.57), against 0160's 0.55 s.

## Other gates

- `cargo fmt --check` passes for the adapter.
- `git diff --check` is clean, and no added line is indented with four spaces.
- No core, kernel or Polars change.
