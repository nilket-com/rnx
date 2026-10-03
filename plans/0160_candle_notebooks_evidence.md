# rnx 0160 evidence: Candle notebooks

**Status:** implemented, for review.

## Source and environment

- **Checked at `e6cfdef`.** This commit's tree is that one plus this file.
- **The reference** is rnx-bench `b1a0e5e` (`probes/candle-search-0160`, `results/candle-search-0160`).
- **The worker** was built from this tree. Root `rnx` (`2fb0a2ba…`) and `rnx-jupyter` (`14d6bb50…`) are the binaries recorded at 0159 (`e53773e`); no root or kernel source has changed since.
- **Environment:** rustc 1.98.1 and cargo 1.98.1 on Linux 7.0.0-31-generic x86_64. The notebook checker ran in `demos/notebooks/requirements.txt` (Python 3.14.4), with `-W error::RuntimeWarning`.

| file | sha256 |
|---|---|
| `demos/candle/target/release/rnx-candle-demo` | `a8aebe485f448de9a5b815da0cc2c9a63b4ebc68651ea84dc65cacb2e0a1167e` |
| `demos/candle/Cargo.lock` | `0467520c5d2ba7233873ee807746860d0f74d02edad3cc78edbe74b1982048d0` |
| `demos/data/features.csv` (pinned in `check.py`) | `cf586d5a5577e5c2406dd15aa28ae6fbe9bbb336b581e934a26d06e1e05ebe38` |
| `demos/data/mlp.safetensors` (pinned in `check.py`) | `5d9a064f78af2f3ebe126a30dfd5ce488d850b84a59a8669874c0c045e4d5bfb` |
| `demos/data/articles.csv` | `3a6349b89813b5ed17035bca55eb758f28d773013163fa69c19a897376084d34` |
| `demos/data/questions.csv` | `6fc338b214a7abbb6334f17cd9a984669ec6c56a0881de9e7c92eb91113dd6c3` |
| `demos/data/search-reference.json` | `2c680a47eeb49b6a94865705cbad78b325d624ced7bb941244a92b9b4739b70a` |

**The MLP assets:** written by 0129's twin (`twin0129 write`). Three runs gave identical bytes, but only the hashes are relied on; the writer isn't claimed byte-reproducible.

## The worker

- **What it is:** `demos/candle/` is a standalone locked crate. Its lock was seeded from `demos/polars/Cargo.lock`, resolved offline, and is committed. `main` registers both adapters' builders and presenters.
- **The cold build:** 6 min 6.9 s (`real`) for an empty `demos/candle/target`, with the Cargo registry already downloaded, measured once on this machine.

## The model and the reference

- **`sh demos/candle/fetch-model.sh`** took 6.3 s (`real`) and fetched 88 MB. All six SHA-256 checks were OK.
- **The reference environment** is `probes/candle-search-0160/.venv`, created fresh (torch 2.14.1+cpu, sentence-transformers 6.1.0, transformers 5.18.0, tokenizers 0.23.2, numpy 2.5.3; frozen in `requirements.txt`).
- **Its output:** the largest token count seen is within the 256-token limit; that number is recorded in the reference's `method`.
- **Reproducibility:** a second run's output was byte-identical to the committed one (`cmp`).

The top 3 per question and the smallest shown gap (ranks 1/2, 2/3 and 3/4):

```
0 [(0, 0.6512), (2, 0.4597), (3, 0.4116)] min gap 0.0481
1 [(2, 0.4478), (3, 0.4083), (0, 0.257)] min gap 0.0394
2 [(18, 0.5593), (17, 0.3482), (19, 0.3053)] min gap 0.0362
3 [(9, 0.4276), (10, 0.3706), (7, 0.2874)] min gap 0.0417
4 [(21, 0.449), (6, 0.3917), (0, 0.2444)] min gap 0.0156
5 [(7, 0.6199), (9, 0.3571), (8, 0.3063)] min gap 0.0008
```

**This is the model's own behaviour, not tuned:**
- for the lost-phone question it ranks "Turn on two-step sign-in" (0.448) above "Lost your phone with the authenticator" (0.408);
- for the deleted-by-mistake question, "Delete your account" (0.449) above "Restore a deleted note" (0.392).

The notebook says so. The corpus is an authored showcase, not a benchmark.

## The checker

`--polars --candle` runs all 13 notebook-and-worker pairs: the plain three on all three workers, 04 on the Polars and Candle workers, and 05 and 06 on the Candle worker. The demo tree's digests were taken before and after; verification and the controls wrote nothing. The controls give 47 verdicts, all as stated: 7 plain, 21 Polars and 19 Candle.

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
pass: a tensor shape changed (05): result 1: tensor (65, 3) [[0.0, -3.0, 0.0], [0.25, -2.0, 0.5], [0.5, -1.0, 1.0], [0.75, 0.0, 0.125], [1.0, 1.0, 0.625], [1.25, 2.0, 1.125], [1.5, 3.0, 0.25], [1.75, -3.0, 0.75]], expected (64, 3) [[0.0, -3.0, 0.0], [0.25, -2.0, 0.5], [0.5, -1.0, 1.0], [0.75, 0.0, 0.125], [1.0, 1.0, 0.625], [1.25, 2.0, 1.125], [1.5, 3.0, 0.25], [1.75, -3.0, 0.75]]
pass: a tensor value changed (05): result 3: tensor (64, 1) [[0.765625], [0.6406260132789612], [0.484375], [0.51953125], [0.36328125], [0.40625], [0.703125], [0.9296875]], expected (64, 1) [[0.765625], [0.640625], [0.484375], [0.51953125], [0.36328125], [0.40625], [0.703125], [0.9296875]]
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
pass: the model directory missing (names the setup command): model file config.json is missing from /tmp/rnx-0160-ref-ayra8x6s/no-model: run sh demos/candle/fetch-model.sh from the checkout root
exit 0
demos unchanged by verify and controls
```

**Separately:**
- with `demos/models/all-MiniLM-L6-v2` moved aside, `--only 05_candle_model` passes, and `--only 06_candle_search` fails with `model file config.json is missing from …/demos/models/all-MiniLM-L6-v2: run sh demos/candle/fetch-model.sh from the checkout root`;
- the model was restored afterwards.

## After review (R1)

- **The gap:** each search frame's two forms were checked against the reference separately, each within `1e-5`, so text and HTML could disagree and still pass. Codex showed it by changing one HTML score.
- **The fix:** `check_forms` now parses both forms and requires them to agree exactly (f32 cells by value) before the agreed frame is compared with the expectation. It's used for every frame in 04, 05 and 06.
- **New controls:**
  - an HTML-only and a text-only change to the next f32 above a shown score: inside the reference tolerance, at the column's width, both refused;
  - the same value in another valid spelling: accepted.
- **No outputs, scores, reference or tolerance changed.**

## Cost, measured once, with the method stated

Run-all for each notebook through nbclient, kernel start included, warm, 5 runs:
- `05_candle_model`: median 0.55 s (0.52–0.59);
- `06_candle_search`: median 1.29 s (1.19–1.39), model load and all embeddings included.

## Other gates

- `cargo test --locked --test demos`: 6 passed.
- No rnx, adapter or kernel source changed.
- `cargo fmt --check` passes for the worker.
- `git diff --check` is clean, and no added line is indented with four spaces.
