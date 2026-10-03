# rnx 0155: a terminal showcase, three offline demos

**Status:** plan.

**The user (2026-10-03):** move to marketing materials, starting simple, "showing stuff off in a very simple terminal, pure rnx only". Notebooks, Polariton, Polars and Candle come later. The scope was agreed with Codex over chatd.

**What this record makes:** three short demos.
- **Each is runnable** as `rnx run demos/NN_name.rn` from the repository root.
- **No adapters, no network, no notebook or UI.** Just the native `rnx` executable and tiny bundled data.
- **Each prints a visible answer,** has a checked transcript, and comes with a line or two of copy saying what it shows.

## 1. The stock display, inspected first

Codex asked for this before any pretty-printing proposal. It was checked on today's `rnx` (`/home/me/.cargo/bin/rnx`).

**What's already there:**
- `println!` and `format!` support width, alignment, precision and zero-padding: `{:>8}`, `{:<8}`, `{:^8}`, `{:8.2}`, `{:>10.3}`, `{:08.2}`.
- `{:?}` prints vectors and objects readably.
- **So the aligned report needs no core change,** and 0155 changes no rnx code. If a demo turns out to need one, it comes back to Codex as a separately bounded decision.

**A hazard:** an object prints and iterates in hash order, not insertion order. 0152 saw it flip between runs (`{alpha…zeta}` printed as gamma, delta, zeta, epsilon, alpha, beta).
- **So no demo prints or iterates an object directly.** Every keyed output is sorted explicitly. Records are read into vectors, and fields are accessed by name.

## 2. The demos

| demo | what it shows | the answer it prints |
|---|---|---|
| `01_mortgage.rn`: a useful calculation | Rune as a calculator with a real language: functions, a loop, floating point, formatted numbers. No dependencies. | For a 300,000 loan at 6.0% a year over 30 years: the monthly payment, the total paid and total interest, and the remaining balance after years 1, 5, 10, 20 and 30. |
| `02_orders.rn`: reading JSON | JSON in, answer out, in one short script: read a file, filter, group, sort and summarise. | From `demos/data/orders.json` (about a dozen small orders): shipped revenue per region, the largest customer, and the share of orders still open. |
| `03_report.rn`: an aligned report | A readable terminal report from ordinary format strings, with no table library. | The same orders as a fixed-width report: region, orders, units, revenue, share, and a proportional `#` bar, with a totals row. |

**The two order demos share data but show different things.** `02` answers questions; `03` is about presentation.

**How the demos are written:**
- Each one is **under about 40 lines.**
- Input is deterministic, and the bundled data is committed.
- Output comes from fixed format specifiers.
- There's no time, no randomness and no environment dependence.
- Each takes an optional data path as its first argument, defaulting to `demos/data/orders.json`, so the documented commands run from the repository root.

## 3. Checks (Codex: the transcript must prove the answer, not just stability)

A new test, `tests/demos.rs`, runs the built `rnx` (`CARGO_BIN_EXE_rnx`) on each demo from the repository root and requires:
- **exit status 0 and an empty stderr;**
- **a stdout byte-identical** to `demos/out/NN_name.txt`, the committed transcript.

**Independent answer checks,** computed in Rust from first principles, not from the demo's output:
- **The mortgage:** the payment by the annuity formula P·r / (1 − (1 + r)^−n), with r = 0.06 / 12 and n = 360.
  - The test computes it in f64 and asserts that the transcript contains it, formatted as the demo formats it.
  - The same goes for the total paid, the total interest and the year-end balances.
  - **One hand-computed anchor is also asserted:** the well-known payment, 1,798.65 a month.
- **The orders:** the test parses `demos/data/orders.json` with `serde_json` and computes:
  - per-region shipped revenue;
  - the largest customer;
  - the open share;
  - each region's order and unit counts, and the totals row.

  It asserts each one appears in the relevant transcript.
- **Boundary cases in the data,** chosen now:
  - an order with zero quantity;
  - an order whose status is neither shipped nor open (`cancelled`), which counts in no revenue;
  - two customers tied on revenue except for one cent, so the largest is unambiguous;
  - a region with only open orders, which shows 0.00 revenue.

## 4. Copy and the install path (cold and warm kept apart)

**`demos/README.md`:**
- **Setup, once:** the existing `cargo install` line, separately.
- **The three demos:** each with its exact command, one or two lines on the benefit, and an excerpt of its real output, copied from the checked transcript.

**The repository README** gets a short "Try it in a terminal" pointer to `demos/README.md`.

**No speed claims in 0155.** If a time is ever quoted, it's measured and labelled as cold (the first run after install) or warm.

## 4a. Folded in from Codex's acceptance (no new review cycle)

**1. Getting the data.** `cargo install` doesn't install `demos/`, so the copy documents the whole path:
- **Clone** the repository and `cd` into it;
- **install** `rnx` from that checkout (`cargo install --path . --locked`);
- **then run** each demo from the checkout's root.

These exact commands are verified on a **fresh checkout** of the impl commit:
- a new `git worktree`;
- `rnx` installed into a temporary `CARGO_INSTALL_ROOT` from that checkout;
- each documented command run;
- each output compared with its transcript.

**2. The metrics, defined:**

| metric | definition |
|---|---|
| money | integer cents throughout the order demos (qty × `unit_cents`), so the one-cent gap is exact |
| revenue | shipped orders only; open and cancelled orders contribute 0 |
| the largest customer | the highest total shipped revenue; ties go to the earlier name |
| region order counts (`03`) | all statuses, shipped, open and cancelled alike |
| units (`03`) | shipped units only |
| the open-share denominator | all orders, every status |
| row order | `02`: revenue descending, then name; `03`: region name, ascending |

**The mortgage's output states its assumption:** a constant yearly rate with monthly payments, and totals that exclude fees, taxes and insurance. That explains the calculation; it isn't a caveat added later.

**3. Assertions are bound to labels.** The test parses each transcript line into fields: a region, customer or year, with its named value. It compares each field with the independently computed value for that label; a bare number found anywhere isn't enough.
- **A control proves the binding:** the checker is also run on a copy of a transcript with two regions' revenue values swapped, and on one with a customer's name changed. Both must be **rejected**.

## 5. Out of scope

- Any rnx code change, including pretty-printing (stock display suffices here).
- `:dep polars` and Candle: the next showcase records, with their cold build or download costs stated.
- Notebooks and Polariton.
- Performance claims.
