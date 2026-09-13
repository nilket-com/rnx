# Upstream enhancement draft: building the default context costs milliseconds

Ready to file against `rune-rs/rune`; the report is everything below the rule.
This is an **enhancement request**, not a defect: everything registered is
registered correctly. It is a cost, it dominates the startup of a
process-per-invocation host, and no issue reports it.

Not filed. Filing it is the operator's call. Measured on Linux on 2026-09-13,
on one machine (Intel i7-14700, kernel 7.0.0-31), Rust 1.98.1, against 0.14.2
with `default-features = false, features = ["std"]`. `main` was read, not
measured: `bb8e6937` still expands trait implementations per implementing type
and its metadata installer is unchanged from 0.14.2, but its handler
representation differs and it needs its own numbers before anything here is
claimed of it.

---

**Title:** `Context::with_default_modules` takes about 2.5 ms, and about 90% of
it is installing metadata for trait implementations

**Version:** 0.14.2, `std` only, release profile.

### The measurement

Whole processes under `hyperfine -N`, pinned to one core, 100 runs, each
returning immediately after the named phase, so each figure is a process-level
delta rather than an in-process clock:

| after | mean | delta |
| --- | --- | --- |
| exit at once | 0.56 ms | the process floor |
| `Context::with_default_modules()` | 3.7 ms | **+3.1 ms** |
| `context.runtime()` | 3.7 ms | ~0 |
| compiling `pub fn main() { 42 }` | 3.7 ms | ~0 |
| running it | 3.7 ms | ~0 |

Compiling and running a trivial program are below the noise. The context is
effectively all of it. A separate in-process breakdown across the 33 default
modules, timing each module's construction, installation and drop:

| work | mean |
| --- | --- |
| construct module descriptions | 0.239 ms |
| install them into the context | 2.258 ms |
| drop the descriptions | 0.024 ms |

Installation is about 90%. By module, the largest are `ops` (0.591 ms) and
`iter` (0.586 ms) — about 47% together — then `collections::hash_set` (0.223),
`string` (0.190) and `collections::hash_map` (0.147). Within those two,
installing trait implementations is 92% of `ops` and 88% of `iter`. Installing
the `Iterator` implementations for its 21 types alone takes about 0.719 ms and
adds 1,387 entries to the function map and 715 metadata records.

### Where it goes

`compile/context.rs`, `install_trait_impl` runs the trait's handler once per
implementing type. Each default method the handler defines for that type goes
through `TraitContext::function_inner` into `Context::install_associated`, and
each of those, per entry:

- clones the container's `TypeInfo`;
- builds an `ItemBuf` by extending the type's item with the method name;
- inserts that item into `names`;
- clones it again into `item_to_hash`;
- pushes a `ContextMeta` and indexes it in `hash_to_meta`;
- inserts the handler into `functions`.

and `raw_function` allocates a fresh `Arc<FunctionHandler>` for every type for
every default method, though the closure's code is the same for all of them.
That is roughly half a microsecond per entry, 1,387 times, for `Iterator` alone.

### What would close it, and one thing that would not

The obvious fix — register one shared handler per default method instead of one
per implementing type — is **not** a drop-in change, and we want to say so
rather than propose it naively: the default-method closures capture
type-specific handlers. `count`, for example, captures that type's `NEXT`. Any
sharing scheme has to keep that dispatch or replace it.

Two others look more tractable from outside:

1. **Lighter metadata when `doc` is off.** `context.rs` has 42 `cfg(feature =
   "doc")` sites, but the non-doc path still builds an `ItemBuf` per entry,
   inserts it into `names`, clones it into `item_to_hash`, and pushes a
   `ContextMeta`. A host that will never render documentation pays for all of
   it.
2. **Expanding a type's default methods on first lookup** rather than at
   installation, so a script that never iterates never pays for 21 types'
   worth of iterator methods.

### What it is worth

A host that runs one script per process pays this on every invocation. For rnx
it is about three quarters of `rnx eval 42`: 3.9 ms, of which the context is
about 3.1. That is still fast enough to feel instant, so this is an
optimisation and not a complaint — but it is the whole of the startup cost, and
it is invisible from outside without instrumenting the crate.

### Prior art checked

Targeted searches on 2026-09-13 found related design history, including #671,
but no issue reporting the startup cost of building the default context.
