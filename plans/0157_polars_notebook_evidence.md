# rnx 0157 evidence: the Polars notebook

**Status:** implemented, for review.

## Source and environment

- **Built and checked at `9c7737a`.** That tree equals this commit's, apart from this evidence file. Every executable was rebuilt from that clean tree before the checks below ran.
- **Toolchain and OS:** rustc 1.98.1 (48a229cea 2026-09-01), cargo 1.98.1 (797e8a9bc 2026-08-05), Linux 7.0.0-31-generic x86_64.
- **Checker environment:** Python 3.14.4 from `demos/notebooks/requirements.txt` (nbclient 0.11.0, nbformat 5.11.1, jupyter_client 8.10.0, pyzmq 27.2.0). The final runs use `-W error::RuntimeWarning`.

| file | sha256 |
|---|---|
| `demos/polars/target/release/rnx-polars-demo` (the worker) | `411b204b2646ad8fc9a8ae870775425243fead995cc04c0ecf60fe1f92ad4a23` |
| `target/release/rnx` | `290437b8fbc54a64086a60343dbc239fe7684fac1d0fc66ec97630aa4a3770df` |
| `jupyter/target/release/rnx-jupyter` | `05aedaea48c128b58ef18a2b2bc75a55c748ef1fe92fd82d7f428e050c50f8b5` |
| `demos/polars/Cargo.lock` (committed) | `5555a7c26e20e58a6135cb337017855316573baa024fda0b5b58a048207a3a8f` |
| `demos/data/sales.csv` | `d7e5cd2d045d65d9832a32c40234a2b4abab82dd2b11d3fa53c1fb66d8678751` |

**Why the worker's hash didn't change across the last amend:** rnx's `build.rs` embeds the git source identity only when the `stock-management` feature is on. The worker builds rnx without it (`default-features = false`), so its binary doesn't depend on the commit. Root `rnx` does, and its hash changed.

## The rejected route (section 5a, item 1)

`rnx project lock --manifest demos/polars/rnx.toml --offline`, with the section 2 manifest inside the checkout:
- **First,** with the new files untracked, it stopped at `rnx: fingerprint /home/me/work/rnx: untracked non-ignored native file`.
- **Then, once they were staged:** `rnx: project lock would be inside native package root; move the project`.

That guard is `tools/project/src/workflow/shared.rs`. It isn't bypassed. The manifest was removed, and `demos/polars/` became the standalone worker crate.

## The worker and its lock (section 5a, items 1 and 2)

- **Seeding and the build:** the lock was seeded from the root `Cargo.lock` (400 packages), then built with `cargo build --release --offline`. Every later build uses `--locked`.
- **The cold build** took **5 min 53.7 s**. That's one cold build of `demos/polars/target`, with the Cargo registry already downloaded, on this machine. It is the only timing claimed.
- **The comparison:** the project tool's assembly lock, generated for the same sources on 2026-10-03 (an earlier scratch project outside the checkout, assembly entry `635060ec…`, lock sha256 `402038371993df626ca177ecaf9d6350ca7b472759e4246b43b63671b3bf2c14`), has the same package names and versions entry for entry. The one difference is the root package's name (`rnx-polars-demo` vs `rnx-project-app`).
- **What that comparison proves:** the resolution only. It doesn't prove identical Cargo features or behaviour: the worker omits `project-sources`.

**The terminal reference,** run with the same worker from the checkout root and from `/tmp` with absolute paths, gives identical stdout:

```
Net revenue: $1227.50 from 20 orders (4 returned, left out)
Top region: north, $387.50, 31.6% of net revenue
```

## The checker

The demo tree's file digests were taken before and after these runs; verification and the controls wrote nothing. The controls give 21 verdicts, all as stated: 7 on the plain notebooks, 14 on the Polars one.

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
pass: an omission line where the region frame must be whole: a whole frame was required, but the preview omits (0, 0)
pass: a duplicated row in the region frame: 5 rows shown of 4
pass: a non-finite mean: not a finite float cell: 'NaN'
exit 0
demos unchanged by verify and controls
```

## Other gates

- `cargo test --locked` (Python-free): **49 test binaries, 407 passed, 0 failed.**
- `git diff --check` is clean, and gates the commit.
- `cargo fmt` was run on the worker crate.
- **Indentation:** tabs in `sales.rn`, in the notebook's code cells and in `check.py`. No line starts with four spaces.
- **No change** to rnx, the adapter, the kernel or the preview format.
