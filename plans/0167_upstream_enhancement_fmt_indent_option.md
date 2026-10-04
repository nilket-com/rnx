# Release request: configurable formatter indentation already on main

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
    sources.insert(rune::Source::new("repro", "fn a(){let x=1;}")?)?;
    let mut options = rune::Options::default();
    options.parse_option("fmt.error-recovery=false")?;
    let output = rune::fmt::prepare(&sources).with_options(&options).format()?;
    println!("{}", output[0].1);
    Ok(())
}
```

The released 0.14.2 output uses four spaces, hardcoded as INDENT in
rune/src/fmt/mod.rs; its public Options has no indent option. Upstream commit
`20b26957` (2026-07-20) already implements configurable indentation and LSP
formatting options. At main `bb8e69372353c50e271c9f115bc771c77aa6b83e`,
`options.parse_option("fmt.indent=tab")` produces real TAB indentation
(Claude's 2026-10-04 scratch check). This is a request to release that work,
not to implement an already-existing option.

rnx needs structural TAB bytes while preserving whitespace in multiline strings,
templates and comments. Its span-aware post-processing is additional machinery
that an upstream option could avoid. This is a formatting request, not linting
or a request to change Rune's syntax or default style.
