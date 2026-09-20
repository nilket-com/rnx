# 0069 gate 1: the wrapper name, the build kind and the identity

Status: the isolated prototype passes on Linux, ready for review before the
product port (gate 2). Production is unchanged: this commit adds only this
evidence. The exact candidate source is
`rnx-bench/probes/shared-build-seam/candidate.patch` against rnx `339c6e3`
(SHA-256 `d24998660197bb2a…`, 1,001 lines, `tools/project` only); `prepare.py`
archives the baseline and applies the patch in its own repository, and
Git-source projects address a bare origin made from the baseline tree by
file URL. Results are in `rnx-bench/results/shared-build-seam-0069/`.

## What changed, and one correction to the plan's numbering

The plan called the new wrapper "generator 3" and the identity "format 3".
In the code the Git-source identity (`new_identity.rs`) is already format
3, generator 3; the path-runtime identity (`cache_identity.rs`) is format
2, generator 2 and is untouched, because path assemblies never share. So the
digest-named wrapper is **generator 4** and the Git-source identity carrying
a build kind is **format 4**; retained 3/3 identities keep their reader. The
plan's decisions are otherwise implemented as written:

- `shared_build: bool` on both native declaration structs, default false,
  serialized only when true, copied by the schema projection and preserved
  by the catalogue's comparison (the catalogue writes `false` until gate 3).
- `generate::wrapper_name`: `rnx-app-` + full BLAKE3 over the framed
  preimage (domain tag `rnx-wrapper-name-1`, length-prefixed manifest with
  the placeholder name, length-prefixed `main`). `git_wrapper` names the
  wrapper this way; `git_wrapper_retained` is generator 3's exact recipe;
  `git_wrapper_for(generator, …)` regenerates whichever an identity
  recorded, and the Git-source verify path uses it, so a retained lock is
  checked against its own recipe. `executable_name` reads the package name
  the entry's executable will have.
- `cache_identity::Build` (`{"kind":"private"}` or `{"kind":"shared",
  "key":…}`) as `Context.build: Option<Build>`, absent from formats 2 and 3
  (their bytes stay canonical), required by format 4 and validated together
  with the name: a format-4 document whose name is not the digest of its own
  wrapper is refused.
- `cache_identity::build_key`: BLAKE3 over the framed preimage with domain
  tag `rnx-build-key-1`: `rustc -vV`, `cargo --version`, target,
  profile, the sorted features (counted), canonical cache root and Cargo
  home, and the admitted configuration (counted). `admitted_settings`
  reads the inventory's authenticated configuration files after the 0067
  policy check and produces the *effective* values as Cargo merges them:
  the linker from the highest-precedence file that sets it, `rustflags`
  joined in ascending precedence with higher-precedence items later and
  repeats kept; precedence is Cargo's search order (Cargo home lowest, a
  deeper ancestor of the build directory higher), and in one directory a
  `config` file is read *instead of* its sibling `config.toml`, which then
  contributes nothing — Cargo selects, it does not merge. The review's two
  counterexamples — two files whose effective linkers differ (`cc` against
  `clang`) but which a sorted, deduplicated set made equal, and an empty
  `config` masking a sibling `config.toml` naming clang so that Cargo home's
  linker is the effective one — are the swap and masking controls below.
- `git_sources::shared_build_eligible`: Git runtime, every native Git,
  every native declaring; decided in `resolve_git` and recorded in the
  identity before anything is built.
- `cache_entry` reads the executable by the wrapper's package name.

## What the probe proves

Six drivers, all passing.

**Vectors** (`vectors.json`). For formats 1 and 2, plain and lifecycle:
`shared_build = false` canonicalizes to the bytes of the omitted field;
`true` adds `"shared_build":true` and composes with `presentation`; a
string, an integer, an unknown sibling and a duplicate key refuse with the
reader's own messages. The baseline tool refuses a Git declaration carrying
the field before publishing a lock: "unknown field `shared_build`, expected
one of `path`, `git`, `rev`, `package`, `builder`, `hook`,
`presentation`".

**Identity** (`identity.json`). Eight Git-source locks with the candidate
tool, every identity format 4, generator 4, wrapper `rnx-app-` + 64 hex:
two projects with the same Polars declaration get the same name and the same
assembly key; adding `shared_build = true` keeps the wrapper name and
changes the build kind to `shared` and the key; dropping `presentation`
changes the name; Polars + PostgreSQL both declared is shared, one declared
is private; a Git runtime with no natives is shared; a path native carrying
`shared_build = true` is accepted, recorded in the lock, and private.
Every eligible assembly on this machine gets the same build key
(`899a714b862a…`): one directory per context.

**Compatibility** (`compatibility.json`). A runtime-only Git project
locked and built by the baseline tool (identity 3/3, `rnx-project-app`)
runs under the candidate (`42`). With its artifact, target and ready
document removed, the candidate's launch refuses and its `build` rebuilds
the entry with the generator-3 name into the entry's private target,
creating no shared directory, and the lock bytes are unchanged; it runs
again. The first candidate build of this probe refused that rebuild
("generated shared assembly changed") because verify regenerated the wrapper
with the new name — the recipe-by-generator regeneration is the fix, and it
is in the patch.

**Control** (`control.json`). The probe's wrong-binary control against the
tools' real generated wrappers, built with the exact Cargo command into one
shared build directory: the baseline's two wrappers both named
`rnx-project-app` — presenting first (308 entries), then plain: **0
entries, same bytes, presents** (the wrong executable, reproduced). The
candidate's two wrappers, `rnx-app-7645b0…` and `rnx-app-5202fb…`:
presenting first (308), plain **1 entry, opaque**, presenting again 0
entries and the same bytes as its first build; and in the reverse order,
plain first (308), presenting **1 entry, presents**.

**Checks** (`checks.json`). Tool formatting; strict clippy in both
configurations; tool suites 67 and 68 (sixteen new tests: wrapper-name
framing, generator recipes, build-key sensitivity to each input and to
nothing else, path identities never carrying a build kind, format 3 decoding
without a build kind, format 4 requiring a valid one and the matching name,
the mixes refused, the eligibility rule, and two configuration controls on
real files — swapping which file sets `cc` and `clang` changes the effective
linker and the key while the same effective linker from another arrangement
shares it, `rustflags` repeated across two files differ from one file's
and keep their join order, an empty `config` beside a clang `config.toml`
leaves Cargo home's linker effective so that changing it changes the key
while an unmasked `config` naming clang wins, and the same masking for
`rustflags`); root default suite 390.

## Qualifications and next gate

Gate 1 records the build kind; it does not build in a shared directory —
that, the coordination lock, the executable scan and the refusal are gate 2,
and the catalogue's declarations wait for gate 3. The probe drives the
wrong-binary control with Cargo directly, as the accepted probe did; the
product's own shared build is gate 2's to prove. Git acquisition in the
fixture runs online against a file-URL origin, as every first `:dep` does.
No cost claim is made.
