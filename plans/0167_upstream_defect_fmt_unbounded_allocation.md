# Release request: unfinished-macro allocation fix already on main

**Status:** unfiled; filing is the user's call.

Rune 0.14.2, Rust stable on Linux x86_64. The formatter is called through
its public API, without a Context, runtime, compiler fork or macro execution.
The fmt error-recovery option is explicitly false.

Minimal Cargo.toml:

```toml
[package]
name = "rune-fmt-repro"
version = "0.0.0"
edition = "2024"
[dependencies]
rune = { version = "=0.14.2", default-features = false, features = ["std", "fmt"] }
```

src/main.rs:

```rust
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut sources = rune::Sources::new();
    sources.insert(rune::Source::new("repro", r#"pub fn main(){println!("x","#)?)?;
    let mut options = rune::Options::default();
    options.parse_option("fmt.error-recovery=false")?;
    let output = rune::fmt::prepare(&sources).with_options(&options).format()?;
    println!("{}", output[0].1);
    Ok(())
}
```

Build first, then run the executable under an OS memory and time limit:
`prlimit --as=536870912 --cpu=2 -- timeout 5s target/release/rune-fmt-repro`.
Do not run the unrestricted reproducer on a machine with important processes.

Expected: a syntax error with bounded work and memory for this tiny input.
Observed in rnx's bounded raw-formatter probe: allocator/address-space limit
terminates formatting with `Failed to format source`; the strict parser used
by rnx fmt rejects it before calling the formatter. Retain measured memory
figures in the 0167 evidence. The report concerns input-proportional bounds,
not a claim of arbitrary code execution or a reproduced unbounded wall time.

Upstream status checked 2026-10-04: the public formatter at
`bb8e69372353c50e271c9f115bc771c77aa6b83e` rejects the unfinished macro
with one diagnostic, about 7.5 MB RSS and 0.10 seconds under a 1 GB virtual
memory / 5-second wall bound (Claude's scratch check, replay retained with
0167's benchmark evidence). Error recovery was explicitly false. The fix
commit was not bisected; do not attribute it to a particular change.
This request is to release the existing fix, retaining the 0.14.2 reproducer
for regression coverage. It does not claim the defect remains on main.
