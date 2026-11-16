# Property tests

Ordinary tests use seed 2, the default PRNG backend and no failure database,
independently of CI detection. Its wall-clock health check is disabled so
host load cannot change a verdict; case-count health checks stay enabled.
The committed lockfile pins the generator implementation. Every property uses the shared runner; the policy test rejects
raw settings, attribute-based bypasses, missing properties and reduced counts.

Run the ordinary gate with:

```sh
cargo +1.98.1 test --locked --manifest-path rune/hegel_tests/Cargo.toml
```

Each property emits `HEGEL_CASE_DIGEST`, its name, case/draw counts and a SHA-256
digest of the actual ordered top-level draws. Floating-point draws bind their
bits, including NaN payloads. The map roundtrip canonicalizes the drawn map for
the digest, then still tests conversion of a Rust `HashMap`.

Random exploration is an explicit development profile. Choose a seed and an
optional minimum case count:

```sh
RNX_HEGEL_SEED=42 RNX_HEGEL_CASES=1000 cargo +1.98.1 test --locked --manifest-path rune/hegel_tests/Cargo.toml --features explore
```

Each property retains its existing lower bound, so the formatter still runs
2,000 and 5,000 cases even if a lower minimum is requested. Omit
`RNX_HEGEL_CASES` to keep every original count. Ordinary builds reject these
overrides, and the gate does not enable `explore`. Keep every discovered
failure as an unconditional regression example before changing the seed.
