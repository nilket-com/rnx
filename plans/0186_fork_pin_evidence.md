# rnx 0186 validation

The implementation pins the eight active dependency graphs to Rune
`f98eb825934d4994478a1759cf340e90d8c5f12f`. Execution remains on the stock VM;
this update makes no native-tier or wall-clock performance claim. The invalid
screen 2 has not been repeated. Claude accepted the source and subsequent
fixture/Jupyter amendments before the implementation commit.

## Completed checks

Commands ran with `cargo +stable`, locked dependencies, and the existing shared
Cargo target. Root formatting and Jupyter formatting pass, as does `git diff --check`.

| Check | Result |
| --- | --- |
| Root `test --locked --no-fail-fast` | 478 passed, 2 ignored |
| Root `test --locked --features project-sources,test-support --no-fail-fast` | 543 passed, 3 ignored |
| Root `clippy --locked --all-targets --features project-sources,test-support` | Pass with existing warnings |
| Jupyter full tests and build | 27 passed; build passes |
| Jupyter `clippy --locked --all-targets` | Pass |
| Standalone `check --locked --all-targets` | All seven active workspaces pass |
| Plain notebooks 01–03 | Generate, verify, negative controls pass |
| Polars notebook 04 and plain notebooks on the Polars worker | Generate, verify, negative controls pass |
| Final verification of retained notebook goldens with both workers | Pass |
| Polars callbacks | All nine pass |

Root counts sum the completed per-target test summaries, including doctests.
Standalone workspaces are tools/project; adapters/polars, candle, postgres;
servers/http-postgres; and demos/polars, candle. The Polars callbacks used
`--features test-support --test callbacks -- --test-threads=1` with dependency
`profile.dev.package."*".opt-level=2` to reuse the demo build cache; debug assertions
remain enabled. Candle is compile-only coverage: no model download, pretrained
search execution, or Candle notebook regeneration was performed. Database and HTTP
workspaces were checked without running external services.

An exploratory strict Clippy invocation with `-D warnings -A deprecated` reports
15 findings. The identical 15 messages reproduce on the clean 0185 baseline with
`test-support`; normal Clippy passes. Two default-deny test permission literals
were corrected from `0` to `0o000`, with unchanged semantics. This is not a claim
that the repository is warning-free.

## Golden and dependency review

- Sorted Object Debug and strict bool: no committed demo/notebook output changes.
  The full root suite, including demo answers and diagnostic tests, passes.
- D1 wording: no demo/notebook golden delta was required.
- D3 type paths: diagnostic assertions now expect `std::...` without leading `::`.
  Quoted panic text remains literal. Method names are proved from structured
  receiver/method hashes, including `counts.values().sum()` end to end.
- Other: tab excerpts and caret prefixes share four-column tab expansion;
  mixed whitespace, wide characters and escaped controls have focused coverage.
  Root version metadata and Jupyter's strict worker requirement/banner follow the
  new pin. Identity tests reject stale or wrong worker identities. Feature-gated
  project fixtures now use Rune's current Arc and SourceLoader APIs.
- Notebook regeneration changed only adjacent same-name stdout transport chunk
  boundaries in 01 and 02. The existing checker's output normalization proves
  complete notebook equivalence. Generated copies are archived locally; original
  golden bytes were retained and reverified. Notebooks 03 and 04 are byte-identical.
  No numerical, content, ordering or metadata golden change is committed.
- Cargo generated all eight lock updates. Normalizing the Rune revision leaves no
  package additions, removals or version changes. Cargo also changes some Windows
  dependency-edge selections among versions already locked. Regenerated notices
  add three selected Windows packages; licence texts are unchanged. The two
  pre-existing unpublished internal-package notice gaps remain unchanged.
- Historical probe locks and licence-source provenance are preserved. Manifest
  features and default execution routing are unchanged. No new product branding,
  hosted service dependency, binaries, build targets or raw receipts are included.

## Retained records and limits

Local records are in `/home/me/work/.reviews/0186-rnx-pin1`, including exact command
logs, workspace and lock audits, the Clippy baseline comparison, notebook audit,
negative controls and generated notebook copies. Initial failures are preserved:
obsolete method call sites, expected path/version/notice assertions, old project
fixture APIs, non-octal test literals, and Jupyter's stale exact worker requirement.
Corrected attempts pass; those original records were not rewritten.

The implementation is in the dedicated rnx-0186 worktree based on origin/main
`927be8b382193d0c13a4dfa9d80834efdde92e2e`. The user's separate checkout with unpushed
main commits is untouched. One shared target was reused. The old clean 0185
worktree is retained because frozen Rune replay evidence names its corpus path.
Push authorization is separate; this packet and commit are local until reviewed.
