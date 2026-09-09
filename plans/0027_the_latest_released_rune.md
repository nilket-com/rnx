# rnx 0027: the latest released Rune

Status: proposed 2026-09-09.

## Decision

Move the exact upstream pin from Rune 0.14.1 to 0.14.2, the latest
published release checked on 2026-09-09. Keep `default-features = false`
and `features = ["std"]`, with no fork or patch. Update the lockfile,
the version reported by the binary, its metadata gate, and the README's
current compatibility note together.

An exact pin makes the tested dependency reproducible; it is not a reason
to stay on an older release. Future upgrades must repeat the compatibility
checks rather than assume that a patch version preserves every behavior
the adapter relies on.

The recursive-drop repair reported on upstream main is not in 0.14.2.
This cut does not adopt unreleased 0.15 code or claim to repair that defect.
Historical records retain the versions their measurements used.

Release source: https://github.com/rune-rs/rune/releases/tag/0.14.2

## Acceptance gates

1. Default and test-support Linux suites pass, including session semantics,
   escaped format captures, renderer and JSON boundaries, diagnostics, and
   the upstream isolation reproducer. A changed reproducer result calls for
   review; it does not authorize sharing runtime contexts.
2. Compare the lexer, format-capture parser, and value serialization paths
   against 0.14.1. In particular, record 0007's scan must not miss any newly
   introduced capture syntax.
3. All four existing port comparisons pass against the updated binary.
4. Windows and both macOS targets type-check with and without test-support.
   Cross-compilation does not satisfy their runtime acceptance gates.
5. The packaged metadata gate passes and `rnx version` names 0.14.2.
   Formatting and clippy are checked; any new warning is explained.

Validation results belong in the accompanying evidence file. License,
publication permission, and platform acceptance are unchanged.

## Forward

**The next upgrade must be tested with rnx's own feature set, not with
default features.** Upstream `main` — 0.15.0 at commit `bb8e6937`, tested on
2026-09-09 — **fails to build** with `default-features = false, features =
["std"]`, with seven errors inside Rune itself: an unresolved `anyhow` import
and an unresolved `crate::support::Context`. With default features on, the
same commit builds. rnx uses the failing combination, so a 0.15 release is
not adoptable until that combination compiles, and checking it is the first
step of that cut rather than a discovery inside it.

That commit is also where the recursive-drop repair lives: a 200,000-deep
value drops there and aborts on every published release, which is why record
0019's limitation survives this upgrade unchanged. The two facts travel
together — the fix rnx wants and the build it cannot yet use are the same
tree — and both were measured against that one commit, which will have moved
by the time anyone acts on this.

Rnx's `rust-version` is a separate question from Rune's, and the evidence
file records why.
