# Record 0057 gate 1: mapped sources through the bounded loader

2026-09-16. Plan `73fd306`; gate 1 accepted and pushed at rnx `c8fb102`, bench `d7bc333`.
Gates 2–6 remain open. Raw checks and reproduction commands are in rnx-bench
`results/package-loader-0057/` and `probes/package-loader/`.

## What changed

The empty `project-sources` feature enables an internal mount table in
program::Loader. A component-wise longest prefix selects a physical root; an
exact mount loads only root/mod.rn. Descendants keep mod.rn before .rn. A missing
mounted file does not fall back to an application-local file. Ordinary loads
retain the pinned candidate rule and its missing-module wording. Both paths
call the same candidate/file/read implementation and retain the same source
snapshots. The exhaustion check precedes even mount resolution.

No CLI source-map option, capability command, manifest parser or public Rust API
was added. Mounted loaders are constructed by internal tests at this checkpoint.
There is no new route into eval, sessions, worker, config or server compilation.
The internal constructor's temporary unused-code allowance is labelled with this
staging reason, not presented as an exercised product entry point.

The separate tools/project workspace holds the private graph-expansion core.
It receives already-parsed nodes from a callback. It canonicalizes manifest paths,
reads each distinct manifest once, checks cycles against active ancestry and
expands each importing edge, including diamonds. It checks depth, manifest and
mount limits before a further read, and names the offending edge. A parser still
has to validate aliases and bound reads before giving it nodes; that is gate 2.
It has no external dependencies, binary, fingerprinting or Cargo orchestration.

## Observations

Seven new root tests plus the four existing loader tests pass:

- An external root with nested and inline file modules preserves self/super;
  renaming the alias leaves files unchanged. At the root, both super::marker
  and crate::marker call the application's marker. A nested mod.rn wins over
  its .rn sibling.
- A three-edge mount chain uses the longest prefix. Both application-local
  and package-local colliding candidates lose. Removing the mounted entry
  refuses naming that physical entry rather than taking the local candidate.
- A diamond reads the same physical source twice under different logical names.
  Its two returned structs have different Rune type hashes, and their fields
  render by name. Exact injected allowance succeeds; one byte less refuses.
- A reference without a declaration opens only the entry and fails. An inline
  module keeps its own body instead of loading the mapped file.
- Runtime and compile faults carry the nested dependency's source. Runtime
  position is line 2; missing-method recovery uses its retained text. A named
  enum variant declared there renders its field. These assertions exercise
  source identity/position and rendering functions, not a new CLI transcript.
- Mount validation covers component-wise matching, absolute roots, duplicate
  prefixes, exact count/depth bounds and invalid/whitespace-padded identifiers.
- At the real 8 MiB constant, two aliases of a source smaller than the allowance
  exceed it in aggregate. Entry and two reads are the only three opens; the
  sticky refusal prevents another open. Existing understated-size-reader and
  pinned-loader fidelity tests also pass unchanged.

Six tool tests pass: a diamond has two shared mounts but one manifest read;
ancestry cycles refuse, including a lexical sub/../ back edge; 64 manifests,
16 edges of depth and 256 mounts succeed at their exact boundaries, and the
next edge refuses before another manifest read. The application counts as one
of the 64 manifests. No version solving or type unification is inferred.

The graph and loader tests meet at the documented expanded mount representation;
end-to-end manifest-to-handoff integration is intentionally not claimed yet.

## Regression and scope

Default suite: 375 passed. Test-support suite: 418 passed. Combined
project-sources/server-runtime/test-support: 431 passed, including five compile-fail
doc tests. All ran sequentially under TERM=xterm and with one test thread. Tool:
six tests pass; formatting is clean in both workspaces; tool clippy passes with
warnings denied; root notices are current.

Root all-target clippy exits 101 on two existing denied non-octal-permissions
lints in tests/config.rs and tests/fs.rs. A detached 73fd306 worktree checked
under the same compiler reproduces the same sorted diagnostic set, including
17 distinct warning occurrences (14 library, two additional library-test and
one REPL test warning). No diagnostic names a new file. It is inherited failure,
not a clean root clippy claim; both raw logs are retained.

The root lockfile, notices, library exports, kernel, adapter and server package
are unchanged. The tool's graph-only Windows type check is not a root Windows
build or Windows execution. No mapped CLI, toolchain/fingerprint validation,
atomic lock workflow, native assembly product or performance gate is passed by
this checkpoint. The next implementation work is gate 2.
