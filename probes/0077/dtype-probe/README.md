# Record 0077 dtype probe

Which routes build a series of the five dtypes whose 0076 fixtures failed
setup, under the adapter's feature set (`polars =0.55.2`, `lazy`, `csv`,
`parquet`). Run:

    CARGO_TARGET_DIR=target/0077-dtype-probe/target cargo run -q --release --manifest-path probes/0077/dtype-probe/Cargo.toml

`result.txt` is the output on 2026-09-22 (each route's panic is caught
and reported as PANIC).
