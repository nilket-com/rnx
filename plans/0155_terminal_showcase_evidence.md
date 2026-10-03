# rnx 0155: the terminal showcase, the evidence

Three offline demos, each run with `rnx run`: `demos/01_mortgage.rn`, `demos/02_orders.rn` and `demos/03_report.rn`. They use bundled data (`demos/data/orders.json`) and have checked transcripts (`demos/out/`). The copy is in `demos/README.md`, with a pointer from the main README. **No rnx code changed.**

## Checks (`tests/demos.rs`, 5 of 5 pass)

- **Each demo:** exit 0, an empty stderr, and stdout byte-identical to its transcript.
- **Every answer is parsed under its label** and compared with an independent Rust computation:
  - **the mortgage:** the annuity formula, the monthly balance recurrence, and the hand anchor 1,798.65;
  - **the orders:** `serde_json` over the data, in integer cents. That covers each region's revenue, the largest customer and runner-up, the open share, and each report row's orders, units, revenue and share, plus the totals row.
- **Controls, all rejected:**
  - two regions' revenue swapped;
  - the largest customer renamed;
  - a report row given another region's revenue;
  - a balance moved to the wrong year.
- **The copy:** each `text` excerpt in `demos/README.md` is a verbatim slice of its transcript.

## The documented commands, on a fresh checkout

- A new `git worktree` of the impl commit.
- `cargo install --path . --locked` with an empty `CARGO_INSTALL_ROOT` and `CARGO_TARGET_DIR`.
- Each documented command, with `PATH` limited to the fresh install and `/usr/bin:/bin`.

```text
fresh checkout of 602c513d180f756c46b42151d2e578d3b11b7cbe at 2026-10-03T00:33:25-05:00
$ cargo install --path . --locked   (CARGO_INSTALL_ROOT and CARGO_TARGET_DIR fresh, empty)
   Installed package `rnx v0.0.0 (FRESH/rnx)` (executable `rnx`)
warning: be sure to add `FRESH/root/bin` to your PATH to be able to run the installed binaries
$ rnx run demos/01_mortgage.rn
exit 0, stderr 0 bytes, identical to demos/out/01_mortgage.txt
$ rnx run demos/02_orders.rn
exit 0, stderr 0 bytes, identical to demos/out/02_orders.txt
$ rnx run demos/03_report.rn
exit 0, stderr 0 bytes, identical to demos/out/03_report.txt
rnx used: FRESH/root/bin/rnx
```

The impl was amended afterwards only to add this evidence file. The demos, transcripts, copy and test are unchanged since the verified commit.

## Stock display, and the Rune gaps found while writing the demos

The display was inspected before writing (plan section 1): format width, alignment and precision cover the report, so no pretty-printing change was needed.

The demos work around these gaps. None needed a core change, and each is a candidate for later, by separate decision:
- **no compound assignment through a nested index** (`rows[i][1] += x`): use a row binding;
- **no `String::repeat`:** a loop;
- **no `Iterator::position`:** a helper;
- **no `Ordering::then`:** an explicit comparator;
- **`println!()` with no arguments is refused:** `println!("")`;
- **object iteration is in hash order:** no demo prints or iterates an object directly.

## Review round 1 (Codex): the optional input file, and the payment note

**The finding:** valid same-shape data broke the order demos.
- **`02_orders`** indexed `customers[0]` and `[1]` with fewer than two customers.
- **`03_report`** divided by zero when nothing had shipped, and printed a misleading 100.0% total for an empty file.

**The fixes:**
- **`02_orders`:**
  - with no orders, it prints only the count line;
  - when nothing has shipped, it prints "Largest customer: none (nothing shipped yet)";
  - with one customer, it prints "…, the only customer".
- **`03_report`:** with no shipped revenue, every share is `-` (undefined), and no bars are drawn. The totals row shows `-` too, never 100.0%.
- **The bundled order transcripts are unchanged,** byte for byte.

**The new test,** `edge_inputs_are_handled`, runs the real executable on four temporary files: empty, all-open, one shipped customer, and cancelled-only. It asserts exit 0, an empty stderr, and those lines.

**The mortgage note:** the transcript now states that the totals use the unrounded payment, shown to four decimals (1798.6516), so 1,798.65 × 360 not matching the printed total isn't confusing. This was the only transcript change. The README excerpt was updated to match, and its verbatim-slice test passes.

**Result:** 6 of 6 tests pass.
