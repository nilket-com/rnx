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

## 5a. Folded in from Codex's acceptance (no new review cycle)

1. **Acquisition and verification are distinct.**
   - `fetch-model.sh` is the only network step.
   - `check.py` checks the six model files' hashes before 06 runs, in both `--generate` and verification; a missing or drifted file fails by name with the setup command, with no download and no skip.
   - `--only 05_candle_model` runs 05 without the model, while the full Candle run fails if it's absent.
   - The README states the model's source, revision, license (Apache-2.0) and size.
2. **The search reference is pinned and validated.**
   - The rnx-bench probe (`probes/candle-search-0160`, committed with its frozen `requirements.txt`) records the exact CSV bytes, the six model hashes, the text construction (articles' `text` column, questions as written), pooling, normalization, the 256-token limit and the largest token count seen, the batch size and the environment versions.
   - The reference validates itself (unique identities, complete rankings, finite scores, no truncation), and `check.py` validates it again, including its provenance, before using it.
3. **The rank-stability gate covers ranks 1/2, 2/3 and 3/4** for every question, with gaps of more than `1e-4`. Ties order by article id, and numeric scores are sorted before any conversion to text. The shown identities, titles and rows must equal the reference, with scores within `1e-5`. The corpus is labelled an authored showcase, not a held-out benchmark.
4. **The MLP's exactness is established for these fixed assets only.** Every product and every subset sum is a float32 exactly, so GEMM order and FMA can't change a result; this isn't claimed for models in general.
   - Displayed f32 text is read back as the f32 it rounds to and compared by value.
   - The safetensors reader validates names, shapes, dtypes and offsets.
   - The bundled assets are pinned by hash. Numeric determinism isn't confused with serialization determinism: 0129's writer isn't claimed byte-reproducible.
5. **Parsing is extended narrowly:**
   - an `f32` dtype;
   - a title pattern used only for 06 (04 keeps its simple strings);
   - an f32 matrix parser for tensor text.

   Every expected result must appear exactly once, in order. Text and HTML must describe the same frame.

   The controls:
   - `fc2.bias` changed so the checked result changes;
   - a strictly-more-than-`1e-5` shifted shown reference score;
   - missing and duplicate ranked rows;
   - a duplicate article in a reference ranking;
   - a changed `questions.csv` and a changed model revision in provenance;
   - plus the listed ones.

6. **After the implementation review (R1):** text and HTML forms of one frame must agree exactly before the agreed frame is compared with an expectation or tolerance.

## 5. Copy and cost

- **The notebook README:** both notebooks, the model fetch, the worker and kernel setup, and what is checked against what.
- **Measured once, with the method stated:** the worker's cold build time, and per notebook the run-all wall time warm. No speed claims beyond those.

## 6. Out of scope

- Rich (HTML) tensor display: a separate record if wanted.
- GPU, generation, NLI and re-ranking notebooks.
- Training in the notebook.
- Any API change.
- Non-Linux kernels.
