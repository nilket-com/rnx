# Formatter silently completes malformed source with recovery disabled

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
    sources.insert(rune::Source::new("repro", "pub fn main(){foo(1,")?)?;
    let mut options = rune::Options::default();
    options.parse_option("fmt.error-recovery=false")?;
    let output = rune::fmt::prepare(&sources).with_options(&options).format()?;
    println!("{}", output[0].1);
    Ok(())
}
```

Expected: formatting fails because the call and function are unfinished.
Observed: successful output `pub fn main() {
    foo(1)
}
`, adding delimiters.
Error recovery is false. A caller cannot treat a successful format result as
proof that the original source was valid. rnx uses strict ast::File parsing
before formatting and token-fidelity validation afterwards.

Upstream status checked 2026-10-04: this still reproduces at
`bb8e69372353c50e271c9f115bc771c77aa6b83e`, including explicit
`fmt.error-recovery=false` (Claude's scratch check). For main's API, attach
`rune::Diagnostics::new()` with `.with_diagnostics(&mut diagnostics)` to the
same prepare call; successful output still closes the unfinished call and
function. Thus this remains a live defect report, unlike the allocation fix.
