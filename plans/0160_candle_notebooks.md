# rnx 0160: Candle notebooks: a model on frame data, and semantic search

**Status:** plan.

**The user (2026-10-03):** "Ohhh candle notebooks next." These follow the Polars notebook (0157), its table display (0158) and HTML tables (0159).

**The cut:** two short Jupyter walkthroughs built on what the Candle adapter already ships, using the same conventions as `04_polars_sales`:
- a locked example worker;
- bundled data;
- a fresh-kernel run-all checked against independent expectations;
- outputs stored and compared.

| notebook | what it shows | download |
|---|---|---|
| `05_candle_model.ipynb` | a frame becomes a tensor, a small model scores it, the scores come back as a frame | none |
| `06_candle_search.ipynb` | semantic search with a real pretrained model (all-MiniLM-L6-v2): embed articles, rank them for questions, show the ranking as tables | the model, once (about 90 MB, pinned and hash-checked) |

**Out of this record:** HTML or rich display for tensors (they keep 0129/0134's bounded text display), GPU, generation (Qwen) and NLI notebooks, any Candle, Polars or core API change, and `:dep` in a cell.

## 1. The worker

**`demos/candle/`** is a standalone crate like `demos/polars`:
- its own `[workspace]`, `publish = false` and a committed `Cargo.lock`, built with `--locked`;
- relative path dependencies on rnx (`default-features = false, features = ["count-allocations"]`), `adapters/polars` and `adapters/candle`;
- `main` registers both adapters' builders and presenters, as a project-tool executable with both would.

**It's a superset of the Polars worker,** so all six notebooks run on it, and the checker proves that. The README documents building it, `rnx-jupyter install --rnx <it> --replace`, switching back, and the measured cold build time.

## 2. `05_candle_model`: a model on frame data

This is 0129's cross-adapter workflow, as a walkthrough:
1. Load `demos/data/features.csv` into a frame (an HTML table in the notebook).
2. `df.to_dense([...], "f32")`, then `candle::Tensor::from_dense`, shown with the tensor's bounded text display (dtype, shape and corner).
3. `candle::Mlp::load("../data/mlp.safetensors")`; the model's own description.
4. `model.forward(x)`, shown as a tensor.
5. `df.with_dense(y.to_dense(["score"]))`: the scores back as a column, shown as a table, then the top rows by score.

**The data:** `features.csv` (64 rows) and `mlp.safetensors` (3 → 8 → 1, fixed weights, about 1 KB) are generated deterministically by 0129's twin and committed with their SHA-256. The notebook says plainly that the weights are fixed to show the plumbing, not trained on anything.

**The independent check:** `check.py` reads the safetensors file itself, in pure Python (`json` header and `struct` floats), and runs Linear, ReLU and Linear in float32 arithmetic emulated with `struct` rounding. Every score shown in the frames must equal its result, row by row, in the displayed `f32` spelling.

## 3. `06_candle_search`: semantic search

1. **Data:** `demos/data/articles.csv`, about 24 short help-desk articles (id, title, text), written for this record and committed. Also `demos/data/questions.csv`, 6 questions.
2. **Load the model:** `candle::TextEncoder::load("../models/all-MiniLM-L6-v2")`, with its description.
3. **Embed** the articles (the tensor's display shows `[24, 384]`), then the questions.
4. **Rank:** `candle::similarity`, scores joined back onto the articles, then the top 3 per question, sorted, as tables. A final cell prints the best article per question.

**The model** is fetched once by a committed `demos/candle/fetch-model.sh`:
- it's 0131's script, pinned to revision `1110a243…`, with every file's SHA-256 checked;
- it writes to `demos/models/` (gitignored);
- rnx makes no network call, and after the fetch everything runs offline.

**The independent reference:**
- an rnx-bench probe runs sentence-transformers (PyTorch) at the same pinned revision over the same CSVs, in 0131's pinned environment;
- it commits `demos/data/search-reference.json`: every question's full article ranking and cosine scores, plus the environment versions.
- `check.py` requires the notebook's displayed rankings to equal the reference order exactly, and its scores to agree within `1e-5`. 0131 measured an agreement of 6.7e-7.
- Ties are refused when the data is built: adjacent reference scores in any shown top 3 must differ by more than `1e-4`, so order can't flip on float noise.

**Setup failures:** if the model directory is missing, `check.py` fails with the setup command. It doesn't skip.

## 4. The checker and controls

- **Reused:** `check.py`'s notebook table and conventions from 0156–0159:
  - fresh kernels and a restart;
  - stdout equal to the terminal reference (a `demos/candle/*.rn` script per notebook, run by the same worker);
  - stored-output comparison of every MIME type;
  - the frame text and HTML parsers;
  - `--generate` separate from verification.
- **The structural parsers are extended** to read Candle's tensor text display (dtype, shape and corner values) for the shown tensors in this example only.
- **New controls:**
  - one MLP weight changed in a temporary copy, so the pure-Python expectation disagrees;
  - a shown score changed;
  - two ranked rows swapped;
  - a reference score shifted by `2e-5`, which must fail;
  - the model directory missing, which must fail with the setup message;
  - a tensor shape changed in the shown text.

## 5. Copy and cost

- **The notebook README:** both notebooks, the model fetch, the worker and kernel setup, and what is checked against what.
- **Measured once, with the method stated:** the worker's cold build time, and per notebook the run-all wall time warm. No speed claims beyond those.

## 6. Out of scope

- Rich (HTML) tensor display: a separate record if wanted.
- GPU, generation, NLI and re-ranking notebooks.
- Training in the notebook.
- Any API change.
- Non-Linux kernels.
