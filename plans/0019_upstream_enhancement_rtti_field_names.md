# Upstream enhancement draft: no public way to ask a value its field names

Ready to file against `rune-rs/rune`; the report is everything below the rule.
This is an **enhancement request**, not a defect: nothing misbehaves, and the
information simply is not reachable.

Checked before filing, on 2026-09-09: no open or closed issue asks for this —
a search for `Rtti` returns three unrelated ones (#90, #424, #877) — and the
accessor is absent in 0.14.1, in 0.14.2, and on `main`. The companion draft,
`0019_upstream_defect_recursive_drop.md`, is **not** to be filed: that defect
is already fixed on `main`, which the same search found.

---

**Title:** Expose `Rtti`'s field names, so a host can render a struct value it
did not declare

**Version:** 0.14.1 and 0.14.2; also absent on `main` (0.15.0 at `bb8e6937`).

The reproducer below was built and run as its own crate, with `rune` as its
only dependency: the four lines it prints and the compile error are that run's
output, copied, from 0.14.2.

### What is missing

`Rtti` already holds the mapping a host needs:

```rust
// crates/rune/src/runtime/value/rtti.rs
pub struct Rtti {
    pub(crate) kind: RttiKind,
    pub(crate) hash: Hash,
    pub(crate) variant_hash: Hash,
    pub(crate) item: ItemBuf,
    /// Mapping from field names to their corresponding indexes.
    pub(crate) fields: FieldMap<Box<str>, usize>,
}
```

`item()`, `type_hash()` and `type_info()` are public. `fields` is not, and has
no accessor, so a host can check a name it already knows but cannot ask which
names exist.

### Reproducer

One file, `rune` as the only dependency:

```rust
use rune::runtime::TypeValue;
use rune::{Context, Diagnostics, Source, Sources, Vm};
use std::sync::Arc;

const SCRIPT: &str = r#"
pub struct P { code, note }
pub fn main() { P { code: 7, note: "seven" } }
"#;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let context = Context::with_default_modules()?;
    let mut sources = Sources::new();
    sources.insert(Source::memory(SCRIPT)?)?;
    let mut diagnostics = Diagnostics::new();
    let unit = rune::prepare(&mut sources)
        .with_context(&context)
        .with_diagnostics(&mut diagnostics)
        .build()?;
    let mut vm = Vm::new(Arc::new(context.runtime()?), Arc::new(unit));
    let value = vm.call(["main"], ())?;

    if let Ok(TypeValue::Struct(s)) = value.as_type_value() {
        println!("item path : {}", s.rtti().item());
        println!("type hash : {}", s.rtti().type_hash());
        println!("arity     : {}", s.data().len());
        println!("get(code) : {}", s.get("code").is_some());
        for name in s.rtti().fields() {
            println!("{name}");
        }
    }
    Ok(())
}
```

Without the `for` loop it prints what the public surface can answer:

```
item path : P
type hash : 0x467f80bed6994967
arity     : 2
get(code) : true
```

With it, it does not compile:

```
error[E0599]: no method named `fields` found for reference `&Arc<Rtti>` in the current scope
  --> src/main.rs:27:24
   |
27 |         for name in s.rtti().fields() {
   |                              ^^^^^^ private field, not a method
```

On `main` the same call fails the same way, with Rune's own `Arc`:

```
error[E0599]: no method named `fields` found for reference `&rune::sync::Arc<Rtti>` in the current scope
   |                              ^^^^^^ private field, not a method
```

(The reproducer as written needs one unrelated change to build against `main`,
where `Vm::new` takes `rune::sync::Arc` rather than `std::sync::Arc`. The
error above is emitted either way.)

### What a host has to do instead

Parse the declarations itself and verify each candidate name against the
value, because a name that cannot be enumerated can still be checked:

```rust
let fits = candidate.len() == s.data().len()
    && candidate.iter().all(|name| s.get(name).is_some());
```

That works, and it is what rnx does, but it needs the source text of the
declaration — which the host has only for code it compiled itself. Worse, the
obvious key for such a table is not an identity: `type_hash()` is the hash of
the item path, so a struct `a::P` from one unit and a different `a::P` from
another are indistinguishable through the public surface. Verification is what
makes the result correct rather than the lookup.

There is also no way to identify the unit a value's type came from, which
would be the alternative fix.

### Suggested direction

Either of these would remove the need for the workaround:

- `pub fn fields(&self) -> impl Iterator<Item = (&str, usize)>` on `Rtti`,
  which is a read of data already there.
- Or a per-unit identity on `Rtti` a host can compare, so a host-side table
  can be keyed by something that actually distinguishes two declarations.

The first is smaller and is what a renderer wants: names in declaration order,
for a value it did not declare.
