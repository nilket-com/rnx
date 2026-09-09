# rnx 0028 evidence: the provenance inventory, and how to reproduce it

Measured on Linux on 2026-09-09 against the locked dependency set, with
`rune` at 0.14.2 (record 0027). Every figure in record 0028 comes from a
command here. A first version of this inventory scanned the registry cache
by crate name and reported a notice count of zero; the scan below is over
the resolved package directories instead, and finds four.

## Commands

```sh
# the resolved set, and the directory of each package
cargo metadata --locked --format-version 1 | jq -r '.packages[] | "\(.name) \(.version) \(.manifest_path)"'

# per target, the classes: TRIPLE is the target
cargo tree --locked --target TRIPLE -e normal,no-proc-macro --prefix none --no-dedupe   # non-proc-macro normal
cargo tree --locked --target TRIPLE -e normal --prefix none --no-dedupe                 # adds proc-macro crates and their subtrees
cargo tree --locked --target TRIPLE -e normal,build --prefix none --no-dedupe           # adds build dependencies
cargo tree --locked --target TRIPLE -e normal,build,dev --prefix none --no-dedupe       # adds dev dependencies
cargo tree --locked --target all -e all -i CRATE                                        # why a crate is present
```

Each list is `sed 's/ (\*)$//'`, `sort -u`, with rnx itself removed. Features
are rnx's defaults: `count-allocations` on, `test-support` off. Adding
`--features test-support` changes no dependency; it gates code, not crates.

## Scope: 91 in the lockfile, 63 and 64 in a build

`Cargo.lock` resolves **91 packages**, which is a superset of any one
build: it includes rnx itself and every optional or other-target crate the
graph can reach. Twenty-four of the 91 are outside both measured graphs —
**23 dependency packages, plus the root package `rnx`, which is excluded from
these counts rather than absent from the build it is the root of**. The 23:

- `aho-corasick v1.1.5`
- `cc v1.4.5`
- `find-msvc-tools v0.1.12`
- `generator v0.8.9`
- `lazy_static v1.5.0`
- `loom v0.7.2`
- `matchers v0.2.0`
- `nu-ansi-term v0.50.3`
- `regex-automata v0.4.18`
- `regex-syntax v0.8.11`
- `rustversion v1.0.23`
- `scoped-tls v1.0.1`
- `sharded-slab v0.1.7`
- `shlex v2.0.1`
- `simdutf8 v0.1.5`
- `thread_local v1.1.10`
- `tracing v0.1.44`
- `tracing-core v0.1.36`
- `tracing-log v0.2.0`
- `tracing-subscriber v0.3.23`
- `valuable v0.1.1`
- `windows-result v0.4.1`
- `zerocopy-derive v0.8.56`

The two targets resolve as follows. "Non-proc-macro normal" is the class that
**identifies candidates for distribution review**; it does not establish what
an artifact contains. It is a graph query, not the linker's view, and the two
are not interchangeable.

| Class | Linux | Windows |
| --- | --- | --- |
| non-proc-macro normal | 49 | 51 |
| proc-macro subtree only (`proc-macro2`, `quote`, `syn` ×2) | 4 | 4 |
| procedural macros themselves | 7 | 7 |
| build dependencies only | 3 | 2 |
| development dependencies only | 0 | 0 |
| **total participating** | **63** | **64** |

The seven procedural macros are `musli-macros`, `pin-project-internal`,
`rune-alloc-macros`, `rune-macros`, `rune-tracing-macros`,
`rustyline-derive` and `serde_derive`. They run in the compiler and are not
linked into the artifact; neither are `proc-macro2`, `quote` or `syn`,
which reach the graph only through them.

Build-only: `autocfg 1.5.1` and `version_check 0.9.5` on both targets, plus
`cfg_aliases 0.2.2` on Linux alone — it is a **build dependency of `nix`**,
which `rustyline` takes on unix targets only. Dev-only is empty because
rnx's one dev-dependency, `unicode-width`, is already a normal dependency.

## Licenses by class

### Non-proc-macro normal, Linux (49)
```
     33 MIT OR Apache-2.0
      8 MIT
      4 Apache-2.0 OR MIT
      1 Unlicense OR MIT
      1 BSD-2-Clause OR Apache-2.0 OR MIT
      1 Apache-2.0 OR BSL-1.0
      1 (MIT OR Apache-2.0) AND Unicode-3.0
```

### Non-proc-macro normal, Windows (51)
```
     35 MIT OR Apache-2.0
      7 MIT
      3 Apache-2.0 OR MIT
      2 BSL-1.0
      1 Unlicense OR MIT
      1 BSD-2-Clause OR Apache-2.0 OR MIT
      1 Apache-2.0 OR BSL-1.0
      1 (MIT OR Apache-2.0) AND Unicode-3.0
```

The two Boost-licensed crates, `clipboard-win 5.4.1` and `error-code 3.4.0`,
are Windows-only and arrive through `rustyline`. Every crate in every class
carries a license field; the only package without one is rnx, by record
0026's decision.

### The crates with terms beyond MIT-or-Apache

| Crate | License | Class |
| --- | --- | --- |
| `unicode-ident 1.0.24` | `(MIT OR Apache-2.0) AND Unicode-3.0` | non-proc-macro normal, both targets — a **direct dependency of `rune`**, not only of the macro toolchain |
| `ryu 1.0.23` | `Apache-2.0 OR BSL-1.0` | non-proc-macro normal, both |
| `zerocopy 0.8.56` | `BSD-2-Clause OR Apache-2.0 OR MIT` | non-proc-macro normal, both |
| `memchr 2.8.3` | `Unlicense OR MIT` | non-proc-macro normal, both |
| `clipboard-win 5.4.1`, `error-code 3.4.0` | `BSL-1.0` | non-proc-macro normal, Windows only |
| `cfg_aliases 0.2.2` | `MIT` | build-only, Linux only |

## The notice scan, corrected

The first scan globbed the registry cache by crate name —
`~/.cargo/registry/src/*/NAME-*` — which matched **196 directories**, because
the cache holds every version of every crate this machine has ever built, from
any project. It also covered `-e normal` edges only. It reported no notice
files, and that conclusion was wrong.

The corrected scan walks `dirname` of each of the 91 resolved
`manifest_path`s and looks for `NOTICE*`, `AUTHORS*`, `COPYRIGHT*` and
`THIRD*`, case-insensitively. It finds four:

| Path | What it is | Class |
| --- | --- | --- |
| `cfg_aliases-0.2.2/NOTICES.md` | A third-party attribution: the `cfg_aliases!` macro reuses code from `tectonic_cfg_support::target_cfg!`, and the file carries that project's MIT terms in full | build-only, Linux |
| `unicode-width-0.2.2/COPYRIGHT` | Restates the crate's own dual MIT-or-Apache licensing; adds no third-party terms | non-proc-macro normal, both |
| `unicode-segmentation-1.13.3/COPYRIGHT` | The same | non-proc-macro normal, both |
| `rune-alloc-0.14.2/third-party/` | A directory containing one file, `.gitignore`. Not a notice; matched on its name | non-proc-macro normal, both |

So one real third-party notice exists in the resolved set, and it is in a
build dependency reached only on unix, through `nix`, through `rustyline`.
**Whether a build-time dependency's attribution must accompany a distributed
binary is a separate question, and finding the file does not answer it.** What
is established is that the file exists and what it says.

## What the license texts actually require

Read from the crates themselves, not summarised from memory.

**Boost Software License 1.0** (`ryu/LICENSE-BOOST`, and the two Windows
crates) requires that the copyright notices and the entire license statement

> must be included in all copies of the Software, in whole or in part, and
> all derivative works of the Software, unless such copies or derivative
> works are solely in the form of machine-executable object code generated by
> a source language processor.

So Boost is **not** "permissive with no notice requirement", which is what an
earlier draft of record 0028 said. It carries a notice requirement with an
exception, and the exception is exactly the binary-only case: a distributed
executable is covered by it, a source distribution is not.

**Apache-2.0 §4** requires, independently of any `NOTICE` file, that

> (a) You must give any other recipients of the Work or Derivative Works a
> copy of this License

with §4(d) adding the `NOTICE`-propagation duty **only** where a `NOTICE`
exists. The absence of `NOTICE` files therefore removes nothing: distributing
under Apache terms still means supplying the license.

**MIT** requires its copyright and permission notice in "all copies or
substantial portions of the Software". **Unicode-3.0** requires that its
notice "appear with all copies of the Data Files or Software, or ... in
associated Documentation", which is satisfiable in either place.

Each dual-licensed crate lets the distributor choose one side, and the choice
changes which of these applies. That choice belongs to the license decision,
which record 0001 decision 7 owns; this file records what each option costs,
not which to take.
