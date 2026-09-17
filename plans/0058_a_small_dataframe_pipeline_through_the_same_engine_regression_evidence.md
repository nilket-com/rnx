# 0058 gate 6: final Linux regression and remaining qualifications

Ready for review after accepted gate 5, rnx fc366b2 and bench 5a0099b, both
pushed. This gate changes adapter documentation and its licence inventory, plus
this record's evidence. It changes no Rust production or test source, dependency
manifest, lockfile or root notice. It does not waive the qualifications below.

## Checks actually run

Root configurations run serially with one test thread on Rust 1.98.1/Linux:

| Configuration | Passed | Failed | Ignored |
| --- | ---: | ---: | ---: |
| Default | 375 | 0 | 0 |
| test-support | 418 | 0 | 0 |
| All features | 437 | 0 | 0 |
| Adapter release/default | 6 | 0 | 0 |
| Adapter release/test-support | 7 | 0 | 0 |

The root packaged-manifest/README checks pass in the suites. Root and adapter
formatting pass. Adapter all-targets release Clippy with warnings denied passes
in both configurations. Root notices and adapter notices checks pass. A fresh
ordinary release build succeeds and selfcheck passes. No new tests duplicate
the accepted contract/notebook/timing fixtures; those remain gates 1–5 evidence.

Strict root Clippy does not pass. Running the baseline's non-strict mode yields
exactly the same nineteen severity/message/source-location triples as the
retained 0057 baseline at 07f6709, including its two deny-by-default permission
literal errors. The JSON comparison and both current logs are retained. No
suppression or unrelated lint repair is introduced here.

## Stock isolation

Git comparison against 4f2fcb5 (closed 0057, before the Polars record) finds no
change to root src/tests/scripts/third-party, Cargo.toml, Cargo.lock, root
THIRD-PARTY-NOTICES.md, jupyter, adapters/postgres, servers or tools. The root
default Cargo tree including development dependencies is byte-identical to the
retained 07f6709 baseline after checkout-path normalization. Polars appears in
neither the root manifest nor default graph. Manifest and lock hashes are saved.

Gate 6 calls for matched stock startup if root source changes. None changed, so
there is no additional timing run and no new stock speed claim. Gate 5 remains
the assembled-product measurement, including the slower verified project launch.

## Licence-text work and its limit

alloc-stdlib 0.2.4 ships no licence text. Its exact published source revision
ae42d22078b98549e987d2f03d12df7b984fde47 has a repository LICENSE, now saved in the
adapter's fallback directory with URL, revision and SHA-256 in SOURCES.tsv. The
offline notices generator verifies that digest; its content matches a text
already in the inventory, so the distinct-text count remains 215.

Two package-wide texts remain unavailable: polars-parquet-format 0.1.0 and
syntree 0.18.0. Complete recursive upstream tree listings were checked at the
published revisions recorded in licence-audit.json. syntree contains no named
licence text there. parquet-format contains a nested varint-specific LICENSE,
which is not substituted for a package-wide grant. Their Cargo declarations are
recorded, but no generic text or later revision is used to pretend the missing
texts were recovered. Notices now report 325 packages, 215 distinct texts and
two missing texts. This is a current inventory, not complete licence clearance.
Both rnx and the adapter remain unpublished as before.

## Windows

The full adapter all-targets check for x86_64-pc-windows-msvc fails in psm and
ring native build tooling on this Linux machine, which has no lib.exe. The raw
failure is saved. No dependency was stubbed or feature removed to manufacture a
pass. Consequently full Windows type-check is not established, and Windows
execution was not performed. This gate supports Linux only.

## Artifacts and follow-up

rnx-bench/probes/polars-regression/check.py replays the checks serially into a
fresh output directory without overwriting accepted evidence. Commands, results,
suite logs, notices output, default tree, baseline lint comparison, native-build
failure and upstream licence audit live in results/polars-regression-0058/.

Project verification remains a separate follow-up. Gate 5 measured roughly
144 ms over generated-direct. A receipt shortcut keyed only by path, size and
inode misses same-size in-place edits; it changes content-verification coverage
even in a trusted directory. Source fingerprints also cost time independently.
That design must state its cache assumptions and full-verification option, test
in-place edits as well as replacement, and measure residual input cost before
promising a few-millisecond launch. Nothing here weakens 0057's run checks.
