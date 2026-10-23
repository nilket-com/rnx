# rnx 0191 evidence

On strix (taskset -c 0-7,10-31), Rust 1.98.1, with the committed lock:

- `cargo build --release --locked --bin rnx` succeeds, and `-v` shows rustc
  invoked with `-C lto=thin`.
- `rnx eval "1 + 2"` prints 3; demos/01_mortgage.rn, 02_orders.rn and
  03_report.rn exit 0.
- Root suite (`--locked --no-fail-fast`): 51 test binaries, 478 passed, 0
  failed, 2 ignored.

Build-time and runtime figures come from Rune P10's audited measurements
(.reviews/0185-p10-report1/RECOMMENDATION.md), not from this run.
