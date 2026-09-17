# rnx 0061: gate 2 assembly identity evidence

Status: ready for Linux review, 2026-09-17. Gate 1 is accepted and pushed at
rnx b96a94a and rnx-bench 0147c24. Gates 3–6 remain open. Evidence and the driver
are in rnx-bench results/cache-identity-0061 and probes/cache-identity.

## Implementation boundary

The project tool gains a private cache_identity module, compiled in its existing
binary/library/probe targets but not selected by the product workflow yet. Its
create function consumes the project declaration, an observed build context,
verified native inventory and Cargo lock bytes. It produces one canonical bounded
identity document and its SHA-256 key. A separate canonical_wrapper helper uses
the actual generator after canonicalizing native paths. The existing wrapper and
all current commands retain their old behaviour until shared-cache integration.

The identity contains a version and generator-policy version, canonical cache and
Cargo-home paths, optional Rustup selections, observed compiler/Cargo versions,
target/profile/features, exact canonical wrapper and main, Cargo lock digest and
native inventory with package associations, tree fingerprints and external inputs
including absent candidates. Inventory collections are sorted. Duplicate entries,
invalid paths/digests, missing direct-native associations, unsupported document
versions and the existing byte/entry/document limits refuse. Native input roots
and tool-managed cache storage cannot contain one another; native source locations
remain distinct even when their bytes match.

No project-lock digest, application source or mount enters the identity. This
exclusion is not permission to skip project source checks. The decoder validates
shape and exact canonical encoding; it does not authenticate the observations.
Later workflow integration must reconstruct and compare the live authoritative
identity before attachment and preserve full project validation before launch.
Only trusted, already-audited context/inventory values belong at construction.

Root source, manifest, lockfile, notices, kernel, adapters and server remain
unchanged. The project tool adds no dependency, public API or CLI option. The
only new generation path is private and opt-in; legacy generation stays intact.

## Actual matrix

An isolated binary imports the new key module and existing product modules
byte-for-byte. It calls the production constructor/decoder, not a Python key
projection. It uses real Cargo metadata and the Git working-tree inventory on a
small generated-wrapper-compatible runtime/adapter fixture. All twenty cases pass.
Every case is also repeated with reversed inventory lists and produces identical
canonical bytes and key. Actual generated wrapper/main equal the bytes in the
identity, and a digest computed independently over those bytes matches its key.

| Change from the baseline | Result |
| --- | --- |
| Different script | same key |
| Added mount, renamed mount, edited mapped source | same key |
| Relative spelling of the same native roots | same key |
| Symlink selecting the same cache root | same key |
| Relocated byte-identical native roots | different key |
| Registration name, builder function, plain/lifecycle hook | different key |
| Adapter bytes, runtime bytes | different key |
| Actually resolved additional native dependency | different key and Cargo lock |
| Actual native default-feature activation | different key |
| Explicit installed toolchain selection | different key |
| Previously absent Cargo configuration | different key |
| Different cache root, different Cargo home | different key |
| Original inputs restored | original key |

Eight cases build and run through their generated wrappers; outputs and executable
hashes are retained. Native edits change the key even when Cargo.lock stays equal.
The explicit toolchain case selects the currently installed compiler explicitly;
it proves presence/selection participates, not execution under another compiler.
The native feature case really compiles the changed feature graph. Alternate
target/profile and arbitrary selected-feature-string observations are synthetic
unit-test changes only, labelled separately. No cross-target build is claimed.

The fixture runtime is intentionally small and API-compatible with the generated
wrapper shape. It does not pretend to test Rune evaluation or Polars. Those real
application paths and costs remain gate 5. Recorded compiler contexts are supplied
by the fixture; selecting/validating those observations in the product command
path is part of gates 3–4, supported by gate 1's context evidence.

## Validation and provenance

Both tool configurations pass 40 tests with zero failures and two ignored each.
Three new tests cover canonical persistence/refusals, independently changed build
context fields, and inventory association/boundary checks. Strict all-targets
Clippy passes with warnings denied in both configurations. Formatting and notices
are current. Windows all-targets type-check passes, with the pre-existing unused
UNIX_EPOCH import warning in artifact tests; Windows execution is not claimed.

The probe is formatted and strict-Clippy clean. Imported-source hashes, the source
baseline plus patch, generated/native fixture sources, Cargo locks/metadata,
context records and exact identity bytes are archived. This preserves the content
that produced each key rather than recording hashes alone. The temporary projects
and their controlled build processes finish before removal; no user cache or
configuration, history, database or kernel is touched.

No shared entry or receipt is published by the product yet. Gate 3 must supply
concurrent build ownership, failure/interrupt policy and readiness publication;
gate 4 must integrate lock/receipt versions and legacy behaviour. Neither is
established by a passing identity matrix, and no performance claim is made here.
