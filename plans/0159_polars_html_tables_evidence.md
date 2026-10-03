# rnx 0159 evidence: HTML tables for DataFrames in Jupyter

**Status:** implemented, for review.

## Source and environment

- **Built and checked at `e53773e`.** This commit's tree is that one plus this file and, after review, one test renamed (`a_near_limit_rich_reply_fits_the_frame`, with its printed line). No production source differs.
- **The browser check** is rnx-bench `1307ec7` (`probes/jupyter-html-0159/browser.py`, `results/jupyter-html-0159/`).
- **Toolchain and OS:** rustc 1.98.1 and cargo 1.98.1 on Linux 7.0.0-31-generic x86_64. The notebook checker ran in `demos/notebooks/requirements.txt` (Python 3.14.4), with `-W error::RuntimeWarning`. The browser check ran in rnx-bench's pinned `probes/jupyter-notebook/.venv` (JupyterLab 4.6.3, Playwright 1.62.0, Chromium 151.0.7922.34).

| file | sha256 |
|---|---|
| `target/release/rnx` | `2fb0a2ba3b9b05c5208e8da28771e5d4415bd96402779542837e54a6bbb5eee5` |
| `jupyter/target/release/rnx-jupyter` | `14d6bb50de7460d10956f20a45df75b84257b1ef160ae7dd56b0fc1a49785b57` |
| `demos/polars/target/release/rnx-polars-demo` | `c03ccf3731f5f2bae4e3c6cec894a2ad129bf3682071d4a93d64fc236b7c5579` |
| `tests/fixtures/html_allowlist.json` | `688aea9f785f6b26b936082660cdeb06517481e2044ca1b27b7895ab7b858202` |

The kernel and worker hashes are the ones the browser check recorded in its journal. No lock file changed.

## What changed

**Core (`src/`):**
- `present::Presenters::register_html` and `present_html`, a second registry. Existing APIs and generated mains are unchanged.
- `present::HTML_BYTES` (16,384) and `present::html_allowed`.
- `Session::present_html`.
- The worker's settled reply gains `"text_html"`: null, or an allowed form, only for a successful top-level result with an HTML presenter.
- The `rnx_test` fixtures gain HTML forms for every handled failure, plus the maximal transport form.

**Kernel (`jupyter/src/html.rs`):** a copy of the allowlist, and `result_data`, which publishes `text/plain` plus `text/html` only when allowed. An absent, null, refused, oversized or non-string field gives text alone, never a protocol failure, and HTML never stands alone.

**Adapter:**
- `preview::cells` is the 0158 extraction, factored out and shared, with the text bytes unchanged.
- `render_html` and `render_html_within` are the writer, with its own usable capacity (`HTML_BYTES - 64`) and reserved closing.
- `present` registers both forms.
- A `#[doc(hidden)] preview_html` exists for the allocation control.

## Budgets (section 5a, item 1)

- **Each form** is at most 16,384 bytes, so the decoded bundle is at most 32,768.
- **The general bound:** the decoded MIME bytes are at most 32,768. JSON writes any source byte in at most six bytes (`\u00XX`), whatever the escaping policy, so the two forms serialize to at most 196,608 bytes. The settled reply's other fields (type, id, epoch, input, state and a null failure) are bounded and small, so every legal settled reply stays below the unchanged 262,144-byte worker frame.
- **A near-limit fixture:** `a_near_limit_rich_reply_fits_the_frame` drives `rnx_test::test_loud()`. Its text is at the bound with escapes, and its HTML is at the bound, made entirely of `\` and `"` (each doubled by JSON):

```
near-limit settled reply: text 16381 + html 16337 bytes decoded, 50897 bytes as JSON
```

That's under a fifth of the frame, consistent with the general bound above.

## Tests

- **Root, `cargo test --locked --features test-support --no-fail-fast`:** 49 binaries, 456 passed, 0 failed.
  - New in `src/present.rs`: `the_html_allowlist_corpus` (the shared corpus: 10 accepted and 32 rejected, plus 100,000-deep nesting and oversize) and `html_forms_are_optional_and_discarded_on_failure`.
  - New in `tests/presentation_worker.rs`: `an_html_form_is_optional_and_discarded_on_failure`. It covers an error after a prefix, overflow and refused markup, each followed by a successful evaluation, plus no HTML for plain values, containers, units or a failed presenter. Also `a_near_limit_rich_reply_fits_the_frame`.
- **Kernel, `cargo test --locked`:** 3 binaries, 25 passed, 0 failed. New: the same corpus, and `a_refused_or_absent_html_form_leaves_text_alone`.
- **Adapter, `cargo test --features test-support --no-fail-fast`:** 56 binaries, 285 tests, of which 284 passed and 1 failed on the first full run.
  - The failure was `generated::support::materialize_tests::a_trusted_len_only_return_materializes_by_its_upper_bound`, in generated code this record doesn't touch.
  - It passed 4 of 4 reruns of the library tests. It is the shared materialize-limit race between parallel tests (seen before, at 0088). This is a pre-existing flake, noted and not fixed here.
  - New HTML tests in `preview.rs`:
    - `the_users_reference_frames_as_html`;
    - `hostile_text_is_never_markup` (`<script>`, `<img onerror>`, `</td></tr></table>`, entities, quotes, `javascript:`, controls and bidi);
    - `html_shape_and_boundaries_follow_the_text_table` (empty, zero-column, 1/10/11 rows and 8/9 columns, the same reads as the text);
    - `html_never_reads_omitted_columns`;
    - `html_cuts_are_whole_closed_and_marked`: cuts in the head, at the first body row (with capacity derived from the real head) and at a later row; at the real bound, a maximal hostile head and rows give an allowed, closed, marked fragment within capacity.
  - `preview_alloc.rs` now also measures HTML: `hostile 1,000,000 x 1,000 as HTML: peak 123748 bytes above base, 15999 bytes shown`, under the same 512 KiB bound.
- **Unrelated oracle noise:** full adapter runs again rewrote `oracle-results.json` with `unique` row-order noise. The file was restored.

## The notebook

`04_polars_sales.ipynb` was regenerated with `--generate --polars`. Every frame result now stores `text/plain` (identical to 0158) and `text/html`.

`check.py` adds:
- an independent Python copy of the allowlist, which agrees with the shared corpus on every case;
- an `html.parser` reading of each form, checked against the same CSV expectations with structural elision;
- a requirement that every HTML result sits beside `text/plain`.

The demo tree's digests were taken before and after these runs; verification and the controls wrote nothing. The controls give 28 verdicts, all as stated: 7 plain, 21 Polars. The new ones are:
- an HTML cell changed;
- two HTML rows swapped;
- an injected `<script>`;
- an injected `onerror` attribute;
- an HTML row missing;
- `text/html` without `text/plain`.

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
pass: one HTML cell changed: by_region (HTML): rows differ:
pass: two HTML rows swapped: by_region (HTML): rows differ:
pass: an injected <script>: no allowed HTML form: '<div><small>shape: (4, 4)</small><table><thead><tr><th>region</th><th>units</th><th>revenue_cents</th><th>orders</th></tr><tr><td>string</td><td>i64</td><td>i64</td><td>u32</td></tr></thead><tbody><tr><td><script>alert(1)</script></td><td>16</td><td>38750</td><td>6</td></tr><tr><td>west</td><td>19</td><td>31250</td><td>5</td></tr><tr><td>south</td><td>14</td><td>27000</td><td>5</td></tr><tr><td>east</td><td>12</td><td>25750</td><td>4</td></tr></tbody></table></div>'
pass: an injected onerror attribute: no allowed HTML form: '<div><small>shape: (4, 4)</small><table><thead><tr><th>region</th><th>units</th><th>revenue_cents</th><th>orders</th></tr><tr><td>string</td><td>i64</td><td>i64</td><td>u32</td></tr></thead><tbody><tr><td onerror=alert(1)>north</td><td>16</td><td>38750</td><td>6</td></tr><tr><td>west</td><td>19</td><td>31250</td><td>5</td></tr><tr><td>south</td><td>14</td><td>27000</td><td>5</td></tr><tr><td>east</td><td>12</td><td>25750</td><td>4</td></tr></tbody></table></div>'
pass: an HTML row missing: 3 HTML rows shown of 4
pass: text/html without its text/plain fallback: an HTML result without its text/plain fallback
exit 0
demos unchanged by verify and controls
```

## The browser (rnx-bench `1307ec7`)

**Setup:** a temporary Jupyter environment (the user's registry is untouched), the kernel installed on the Polars worker, and each theme set by a JupyterLab settings override.

**Per theme:**
1. The committed notebook is opened as saved. It's untrusted (JupyterLab reports every code cell `trusted: false`), so its sanitizer applies, and it shows **5 rendered tables whose DOM text equals the CSV expectations**.
2. All outputs are cleared and all cells are run. The 5 tables show again, plus the answer.
3. The notebook is saved, and every saved `execute_result` has exactly `["text/html", "text/plain"]`.
4. Throughout, there is no `script` inside any output.

| theme | applied | cell colour | page background |
|---|---|---|---|
| light | JupyterLab Light | rgba(0, 0, 0, 0.87) | rgb(189, 189, 189) |
| dark | JupyterLab Dark | rgba(255, 255, 255, 0.87) | rgb(97, 97, 97) |

So the attribute-free tables take each theme's own colours. Screenshots are kept: `saved-light.png`, `run-light.png`, `saved-dark.png` and `run-dark.png`.

**Console errors, all logged and each explained:**
- `No active debugger session`: JupyterLab's debugger asks every kernel, and rnx's has none.
- One 404 on `/lsp/status`: the language-server extension is disabled, as in the 0047 probe.

Any other console error, page error or failed request fails the check.

## Cost, measured and not asserted in tests

**Method:** one core, `POLARS_MAX_THREADS=1`, 20,000 renders per frame and three rounds, through the adapter's doc-hidden hooks in a release build. The frames are 0158's (sales 24 × 6, generated 200,000 × 12).

| frame | text µs (bytes) | HTML µs (bytes) |
|---|---|---|
| sales 24 × 6 | 11.0 (1,689) | 9.1 (1,261) |
| 200,000 × 12 | 14.8 (2,221) | 12.6–12.7 (1,880) |

- **The text path is unchanged from 0158.** Only the notebook worker adds the HTML render, so a notebook frame costs about 20–28 µs, inside 0068's 0.15 ms.
- **Worker launch** (hyperfine, 200 runs, pinned): 16.1 ± 0.3 ms, against 0158's 16.0.

## Other gates

- `cargo fmt --check` passes for the root, the kernel and the adapter.
- `git diff --check` is clean.
- No added line is indented with four spaces.
- No Polars feature, inventory or lock change.
