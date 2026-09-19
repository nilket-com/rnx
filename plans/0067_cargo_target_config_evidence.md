# 0067 follow-up: the user's clang/mold configuration

Slim's ordinary Cargo home contains:

```toml
[target.x86_64-unknown-linux-gnu]
linker = "clang"
rustflags = ["-C", "link-arg=-fuse-ld=mold"]
```

The configuration audit rejected `target` before dependency preparation could
build Polars. The session survived correctly, but no Polars extension was loaded.

## Change

Both the retained workflow and shared/Git workflow now use the same config
validator. It admits only `linker` as a nonempty string and `rustflags` as an
array of strings under that exact target. Previously permitted network/registry
and terminal tables remain accepted. Other target fields and top-level settings
remain refused; the error names the file and key and does not suggest moving the
configuration into the project. Config bytes remain authenticated external inputs
in assembly identity. External compiler/linker binaries remain unrecorded,
including the default compiler/linker: this is not a hermetic-build claim.

The scratch preparation error includes checkout override advice only when Cargo
Git acquisition failed. Config policy, compilation and startup errors do not get
that unrelated advice. Dirty/unknown coordinate refusals retain their recovery.

## Validation on nano, not slim

- Root default suite: 376 passed, zero failures, serial.
- Management suites: 54 default / 55 test-support passed, two ignored in each.
- Strict management clippy across all targets in both configurations; root and
  management formatting; management notices current. No dependencies changed.
- Unit controls admit the exact section above, refuse other target triples,
  `runner`, malformed field types and unsupported top-level tables, and retain
  override advice only for acquisition failures.
- A real Git-sourced Polars project at published revision 0a420d4 locks, builds
  and runs the CSV/filter/group/sum/sort/Parquet round trip with clang and mold.
- Two consumers use the same native coordinates, cache root and Cargo home.
  Their assembly identities differ only in the config file's external-input
  record when the section is present versus absent.
- Adding `[patch.crates-io]` refuses before publishing a lock. The config is
  restored before build and stays present through launch.

The repeatable driver is `rnx-bench/probes/cargo-target-config/check.py`; it uses
an isolated Cargo home (copied Git cache, shared existing registry cache), a fresh
assembly cache and retained logs. On this host clang and mold were absent, so
Ubuntu packages clang-21, mold and libmimalloc3 were downloaded and extracted
under the fixture, with their bin/library directories supplied to the test only.
The real tools report clang 21.1.8 and mold 2.40.4. No system package or user Cargo
config was changed. The first cold fixture build took 113 s; no linker speedup
is claimed. A second cold run validates the checked-in driver and final binary.

Final release tool SHA-256:
`91dca9741ba1928eab3337f13ab903c90a1e8ea76c069a04a7b9d483b989e3c1`.

This checkpoint is for review. Slim needs the published fix installed before its
existing Cargo configuration will work through `:dep`; no change was made to slim.
