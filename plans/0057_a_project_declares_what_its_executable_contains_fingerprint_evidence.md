# 0057 gate 3: source fingerprints and Cargo input audit

Status: implemented for review. Gates 1 and 2 are accepted and pushed. Product
lock/build/run, receipt publication and build-environment handling remain gates
4 and 5; no root execution path changes in this gate.

## What is measured

The project tool has private source-tree and Cargo-inventory cores. The source
core visits the real graph, hashes distinct roots once, and retains manifests
outside roots. The native core consumes the generated assembly's Cargo metadata,
keeps every local-package association and fingerprints Git-tracked working-tree
bytes under each distinct root. Registry/git identities remain Cargo's.

Tests exercise changes, additions, deletion, rename, Unix executable mode, dirty
tracked bytes, ignored files, untracked refusal, missing tracked files, Git
submodule entries, symlinks, FIFO and non-Unicode names. Injected allowance tests
exercise exact combined-root byte/entry boundaries, the next entry, empty-directory
accounting and refusal before an over-allowance file read. The public numeric
limits remain 100,000 entries and 512 MiB. A hand-encoded digest pins framing.

A real three-crate Cargo fixture has wrapper -> adapter -> transitive. Mutating
only the transitive source changes its executable from 42 to 43 and its content
identity, with byte-identical Cargo.lock and unchanged adapter tree identity.
A shared-root fixture retains two associations while hashing the root once.
A source diamond likewise hashes once; changing an external source manifest
changes that identity without changing either source tree.

## Ancestor audit

Cargo metadata's workspace_root is the generated wrapper's workspace, not a list
of every dependency's inherited workspace. Therefore the inventory walks package
ancestry too, records Cargo.toml candidates, and follows explicit package.workspace
redirects. The real fixture inherits workspace.package.version from outside both
native package roots; a dirty edit to that ancestor changes the inventory.

Cargo config is discovered from the command's working directory and ancestors,
plus Cargo home. Both spellings are recorded conservatively. Toolchain-file
candidates are recorded as well; compiler/Cargo version outputs remain assembly
identity fields. Absent files are explicit entries, so creation is detected.
The actual adapter metadata resolves exactly rnx and rnx-postgres as local
packages; its repository audit snapshot is archived in rnx-bench.

One supported-input restriction is explicit rather than hidden: a Cargo config
with include causes a named **stop**, before build. Recursive config inclusion
is not accounted for by this implementation. This honors the record's stop rule;
a project using it needs a follow-up policy/review, not an unrecorded input.
The present repository has no such config. Environment/CLI overrides and selected
flags must still be constrained/recorded by the forthcoming build orchestration;
this gate does not claim that file hashing alone establishes build identity.

The audit follows [Cargo configuration discovery](https://doc.rust-lang.org/cargo/reference/config.html),
[workspace inheritance](https://doc.rust-lang.org/cargo/reference/workspaces.html)
and [rustup override precedence](https://rust-lang.github.io/rustup/overrides.html).
Machine toolchain selection is checked by effective version outputs, not inferred
from toolchain files alone. Arbitrary trusted build-script/proc-macro reads,
ignored files and machine inputs remain outside the non-hermetic promise.

## Validation and scope

Results and commands: `rnx-bench/results/package-fingerprints-0057/`.
Final tool suite: 26 passed, zero failed, two deliberately ignored integrations.
The actual repository audit separately passes: two package associations, two
native roots, 35 external candidates and two present external files. It inventories
332 rnx files (4,914,924 bytes at snapshot) and 21 adapter files (517,330 bytes).
Formatting, clippy with warnings denied, notices and Windows type-check all pass.
Tool suite, clippy with warnings denied, formatting, notices and Windows type check
are rerun after final code edits. The actual repository audit runs separately,
with new source files staged so the native untracked-file rule applies honestly.
No root Rust source, manifest, lockfile or notices change. No kernel, adapter or
server code changes; their runtime suites are not rerun for this private tool
change. The inherited gate-2 runner integration remains separately runnable.

Adding pinned sha2 0.10.9 changes only the tool's graph. The refreshed all-platform
notices list 100 registry packages, 63 shipped license texts and the same six
unavailable texts. Windows is type-checked only. No startup or speedup claim.

The audit snapshot precedes the final evidence/status prose and therefore is
not advertised as a product lock for the final commit. Conditions record the
implementation file hashes exercised. Gates 4 through 6 stay open.
