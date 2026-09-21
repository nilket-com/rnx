# rnx 0071: `polars::version()`

Status: ready for review.

## Problem

The user asked how to print the Polars version from a session. The adapter
exposes no such function; the only way to learn it is to read
`adapters/polars/Cargo.toml`, which pins `polars = "=0.55.2"`.

The occasion was a wish to compare against Python Polars. Rust and Python
Polars use separate version tracks; their numbers alone don't establish
release age or feature parity, so 0.55 against 1.x says nothing by itself.
What a session can honestly report is the Rust crate version it was built
against.

## Decision

`polars::version() -> String` returns `polars::VERSION`, the crate's
`CARGO_PKG_VERSION`, which is what the assembly compiled. It is a plain
function with no arguments, listed in the adapter's catalogue with the
other functions, documented in the adapter README, and asserted by the
presentation suite against the pinned version so a bump that forgets one
side fails.

Nothing is read at run time; the string is fixed at build time, which is
the truth of an assembled executable. No stock rnx change: the function
lives in the adapter and is reached by `:dep polars` like the rest.
