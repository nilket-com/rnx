# ADR 0211: Canonical engine workspace and path dependencies

Status: Accepted

After ADR 0208's pure history import, make the rnx repository the canonical
engine and host workspace. Keep rnx at the root and Rune sources under
`rune/`. Make engine crates explicit root members with local path
dependencies; remove the competing nested engine workspace definition. Keep
independent adapters and historical probe workspaces where required, using the
same canonical engine source for active builds. Ordinary rnx default-member
selection must remain narrow.

This is a separate record because 0208's implementation merge must remain a
byte-exact import. Each record has one signed plan and one implementation;
fixes amend the unpublished implementation rather than create extra records.

Choose root resolver and profiles from explicit compatibility comparisons.
Preserve locked versions, selected features, engine benchmark behavior and CLI
compiler recipes. In particular, imported engine bench no-LTO/debug settings
must not silently become rnx thin-LTO release behavior. No automatic all-features
unification, dependency upgrades or performance rebaselining is authorized.

Replace active git pins with canonical engine paths and source guards. Require
one engine source in every active lock graph; reject external git engines,
duplicate sources and escaping paths. Build provenance identifies monorepo
revision, engine subtree and dirty state without a self-referential revision.
Audit generated host templates, verifier cwd assumptions and source-location
maps. Preserve upstream licensing, notices and attribution byte-for-byte.

Review metadata/lock/profile changes before expensive compatibility checks.
Use durable exact-identity receipts, controls before costly work, no implicit
failure retries, and completion notifications rather than polling. Migration
changes compiler/dependency context even when runtime bodies do not change;
retain meaningful fresh-host/engine and existing-path performance checks.

After acceptance, publish through normal fast-forward updates and verify the
canonical remote. Review redirect material and archive the standalone Rune
repository read-only, retaining hashes and tags. Deletion requires a distinct
later explicit user instruction. ADR 0209's root ket-style checker follows with
per-repository grandfather cutoffs before the first ADR 0206 plan. Do not rewrite
published history, including rnx bc0fada, to satisfy the later checker.
