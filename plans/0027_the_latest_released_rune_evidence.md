# rnx 0027 evidence: the latest released Rune

Measured on Linux on 2026-09-09, using the existing Rust 1.95 toolchain.
This records compatibility with released Rune 0.14.2, not with upstream main.

## Dependency and source review

The exact pin moves to `=0.14.2`; Cargo updates `rune`, `rune-core`, and
`rune-macros` from 0.14.1 to 0.14.2. Other locked dependency versions are
unchanged. The feature set remains `default-features = false, features =
["std"]`. The binary prints:

```text
rnx 0.0.0
rune 0.14.2
```

Compared the downloaded release sources against 0.14.1:

- `src/macros/`, including format-capture parsing, is unchanged.
- `src/parse/lexer.rs` changes an existing backslash arm to a guarded match
  arm in the character-literal scanner. It introduces no new escape or
  capture syntax. The session's escape/capture controls run in the suite.
- `src/runtime/value/serde.rs` is byte-identical, preserving the traversal
  assumptions behind the JSON pre-walk.
- `src/runtime/function.rs` is byte-identical. The separate-context control
  and shared-context failure reproducer both still pass. Runtime contexts
  remain per input.

### The whole diff, and what the review covered

The four files above are the ones rnx's assumptions rest on, and checking
them is not the same as reading the release. `diff -rq` over the two crate
sources gives **17 changed files**, all of them below, each opened:

| Path | What changed |
| --- | --- |
| `src/compile/v1/assemble.rs` | **Behavioural.** `if` and `match` where every branch diverges now assemble as diverging (`Asm::diverge`) instead of converging; a fallback that converges is tracked separately. |
| `src/compile/v1/needs.rs` | Nested `if` folded into a guarded match arm on `AddressKind::Scope`. |
| `src/parse/lexer.rs` | The character-literal backslash arm folded into a guarded arm. No new escape or capture syntax. |
| `src/ast/expr.rs`, `src/grammar/grammar.rs` | The same fold, on the precedence-group check. |
| `src/grammar/classify.rs` | The same fold, in expression classification. |
| `src/ast/vis.rs`, `src/runtime/format.rs` | `impl Default` replaced by `#[derive(Default)]` with `#[default]` on the variant. |
| `src/fmt/format.rs` | An `#[allow(unused_variables, unused_assignments)]` and the same arm fold. Rune's own formatter; rnx does not use it. |
| `src/modules/any.rs`, `core.rs`, `tuple.rs`, `test.rs` | Imports only: `docstring` and `T` no longer imported. The module surfaces are unchanged. |
| `src/lib.rs` | Documented minimum Rust **1.87 → 1.88**; a `cfg_attr` for a nightly rustdoc feature removed. |
| `src/tests.rs`, `src/tests/macros.rs`, `src/tests/external_constructor.rs` | Rune's own tests: imports and a `vec![]` removed. Not compiled by a dependent. |

**No behavioural changes were identified in review that affect what rnx
relies on.** That is the claim this evidence supports, and it is narrower
than "the release changes no behaviour": `assemble.rs` does change
behaviour, in code generation for all-diverging branches, and the review's
basis for calling it immaterial here is that it makes divergence more
accurate rather than less, and that the suite and the four ports compile and
run scripts through that path.

The review read the diff of every file listed. It did not audit the
semantics of the divergence change beyond that reasoning, and it did not run
any upstream test suite.

### The Rust minimum

`lib.rs` raises Rune's documented minimum from 1.87 to 1.88 in this release.
That is one dependency's floor, and rnx's `rust-version = "1.95"` is
unchanged by it: what has to compile is the whole dependency set, and record
0026 measured that set failing at 1.88 — in `rustyline`, on the unstable
`file_lock` feature, not in Rune. So the two numbers are answers to different
questions, and the manifest comment now says which is which.

### What this release does not deliver

Historical 0.14.1 measurements are left intact. In particular, moving to
0.14.2 does not deliver the recursive-drop fix from unreleased upstream
main, and does not expose Rtti's field names.

The source comments that cite `rune-0.14.1` for a specific file or behaviour
keep that citation — it is where the claim was derived — and now record what
was re-verified on 0.14.2 beside it: `runtime/value/serde.rs`,
`runtime/budget.rs`, `runtime/value/rtti.rs`, `runtime/function.rs`,
`runtime/vm_error.rs`, `runtime/protocol.rs` and `ast/item_struct.rs` are all
byte-identical between the two releases. The struct-literal ordering comment
is the exception and says so: the compiler did change, so what supports that
claim is record 0019's rendering matrix, which runs in the suite.

## Validation

- `cargo test --locked --quiet`: 232 passed.
- `cargo test --locked --quiet --features test-support`: 237 passed.
- Both runs include the package metadata round trip and all session,
  renderer, JSON, diagnostic, capture, and upstream reproducer gates.
- The classifier fixture matches `expected.out` byte for byte, with empty
  standard error and exit zero.
- The summarizer's 19 comparisons, confinement check's eight cases, and
  graft verifier's nine cases pass against the updated binary. The latter
  uses the small regression lineage, not a new 1,346-commit timing run.

- `cargo check --locked --all-targets --target TARGET`, with and without
  `--features test-support`, passes without warnings or errors for
  `x86_64-pc-windows-msvc`, `x86_64-apple-darwin`, and
  `aarch64-apple-darwin`.
- `cargo clippy --locked --all-targets --features test-support` succeeds
  with the existing 11 warnings (including duplicate reports across binary
  and binary-test targets only once).
- `cargo fmt --check` and `git diff --check` pass.

No Windows or macOS runtime acceptance is claimed. No adapter code changes
were needed; the implementation changes only the dependency selection and
the places reporting or asserting that selection.
