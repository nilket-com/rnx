# rnx 0031 evidence: inventory before integration

Checked by Codex on 2026-09-13. This is source-inspection evidence for a
proposed integration record, not an implementation or performance result.

## Sources and provenance

The module inventory was read from the published
[rune-modules 0.14.2 archive](https://static.crates.io/crates/rune-modules/rune-modules-0.14.2.crate),
whose SHA-256 is:

```text
fc8b73d4c6711eeb4dc57ea2cbf7b8b7ef2640edd19045f7c116441a0e713178
```

The relevant files are `Cargo.toml`, `src/lib.rs`, and the files named for
each module under `src/`. All nine optional modules listed by the record
are present: HTTP, JSON, filesystem, time, TOML, random, base64, process,
and signal. This inventory does not imply that a reported scratch benchmark
installed that exact set; its manifest and commands are needed to establish
that independently.

The async budgeting check used the locally cached published Rune 0.14.2
source, `src/runtime/budget.rs`. It implements `Future` for `Budget<T>`;
`poll` installs the stored budget, polls the wrapped future, saves the
remaining count, and restores the outer budget through its guard. Its
documentation explicitly limits enforcement to VM instructions unless a
native function cooperates.

The current host was checked in `src/runner.rs`, `src/session.rs`,
`src/json.rs`, and `src/main.rs`, together with records 0005, 0019, 0021,
0029, and 0030. No host implementation was changed during this review.

## Findings that affect the proposed record

1. **Registered HTTP verbs differ from implemented methods.** The module
   constructor registers `Client::get`, `post`, `put`, `delete`, and `head`.
   A Rust `patch` method exists but its metadata is not installed. Script
   coverage must test registration, not count Rust methods.
2. **HTTP is a starting point, not yet rnx's bounded operation.** The
   published module exposes no timeout setter or body-size-limit parameter.
   Its text and JSON response helpers delegate to reqwest, and its bytes
   helper collects chunks. This review does not establish reqwest's complete
   default policy; the host must explicitly select and test the desired
   deadline and decoded-body limits rather than assume they are supplied.
3. **Filesystem breadth is absent.** The filesystem module registers only
   async `read_to_string`; its implementation delegates to Tokio without
   rnx's existing read cap.
4. **Time is not datetime.** The time module supplies Duration, Instant,
   sleep, interval, and related operations. No wall-clock/calendar type is
   supplied there.
5. **JSON has an existing rnx contract.** `src/json.rs` bounds the recursive
   serialization walk and refuses cycles and unsupported values before
   invoking serde. Installing another serializer without that walk would
   expose a materially different behavior. Parsing remains separate work.
6. **A future-aware budget primitive exists upstream.** That reduces the
   amount of new machinery potentially needed, but does not prove rnx's
   sliced execution, diagnostics, or interrupt handling work after conversion.
7. **The session ceiling is sampled.** It checks tracked live allocation
   request bytes between commands; it is not an in-flight download cap.
   Existing accounting cannot justify unbounded new body-reading helpers.

## Preserved adoption measurements

The initial 3.5/3.9/4.3 ms report had no raw export. Claude subsequently
preserved both scratch crates and reran them in `rnx-bench` at commit
`53fc3bd554befb5b55cb25aacebf07a0a96c9c1b`. Use that committed rerun:

- `probes/companion-modules/`: source, manifest, and lockfile for adoption.
- `probes/context-phases/`: source, manifest, and lockfile for phase probing.
- `probes/run.sh`: release builds and exact hyperfine commands.
- `results/probes.json`: raw observations and exit codes.
- `results/probes.md`: rendered timing table.
- `results/probes_versions.txt`: Rust version, host, CPU, date, binary sizes.

These paths are relative to that benchmark repository and commit, not to
rnx. It has no published remote referenced here. To inspect the retained
export locally, for example:

```sh
git -C ../rnx-bench show 53fc3bd554befb5b55cb25aacebf07a0a96c9c1b:results/probes.json
```

Nano, Intel i7-14700, Linux 7.0.0-31-generic, Rust 1.98.1; both crates pin
Rune 0.14.2 and the companion probe pins rune-modules 0.14.2. Release profile,
`taskset -c 4 hyperfine -N --warmup 10 --runs 100`:

| Companion-probe mode | Mean ± standard deviation |
| --- | ---: |
| `none`: return immediately | 0.524 ± 0.016 ms |
| `context`: default context | 3.509 ± 0.032 ms |
| `nohttp`: add seven selected modules | 3.668 ± 0.012 ms |
| `modules`: add HTTP as the eighth | 3.816 ± 0.026 ms |
| `run`: also prepare and execute an async function | 4.242 ± 0.115 ms |

The seven are JSON, filesystem, time, TOML, process, random, and base64.
Signal is not enabled or installed. The probe's set is an experiment, not
the chosen shipping set in 0031; in particular it includes process.

The final mode constructs `context.runtime()`, compiles a Rune function,
creates a current-thread Tokio runtime with `enable_all()`, calls
`vm.async_call`, and prints 42. Its entire Rune function is:

```rune
pub async fn main() { let x = time::Duration::from_millis(1); 42 }
```

It neither sleeps nor performs I/O. The 0.426 ms difference from `modules`
is a compound workload difference, including output and cleanup, not the
isolated cost of starting Tokio. As with record 0030, process-level deltas
include destruction. All companion modes use the same compiled binary;
`nohttp` does not remove HTTP dependencies from that binary.

The saved sizes are 13.8938 MiB for the companion probe and 7.75674 MiB for
the separate context-phases probe. The latter does not establish a matched
before-build for the former. These replace the unpreserved 9.2/13.9 MiB
size comparison for purposes of this evidence.

Inspection with `cargo tree --locked --offline -e features -i rune` in the
companion crate shows that rune-modules depends on Rune with default
features enabled. Cargo therefore enables Rune `default` and `emit` there,
despite the probe's direct Rune dependency specifying
`default-features = false`. The context-phases probe only requests `std`.
Adoption must inspect the resolved graph, not just the direct dependency
line. Neither existing rnx's feature choice nor the companion crate's
default-features setting suppresses that transitive request.

Codex inspected the committed sources and exports; every one of the ten
exported modes has 100 observations and all recorded exit codes are zero.
This review did not independently rerun the timings. The harness builds
through a pipeline ending in `tail` without `pipefail`; a failed build could
therefore leave an older executable available. That is a reproducibility
gap to harden, not evidence that the saved run used a stale executable.
Binary hashes, resolved feature-tree exports, build-time measurements, and
link inspection are not in the cited versions file. Real pending I/O,
cancellation, memory behavior, and integrated rnx remain unmeasured here.

## Matched emit-feature comparison

Benchmark commit `c920bfccfab91cc6b0119ee2e08fedaeef9adb4d` adds
`probes/context-phases-emit/` and `results/probes_emit.json`/`.md`.
Codex compared its `src/main.rs` with `context-phases/src/main.rs`: they
are byte-identical. The manifests differ in package name and enabling Rune
`emit`; common resolved dependency names and versions match in their
lockfiles. Both pin Rune 0.14.2. The recorded run used 100 observations per
mode, all with zero exit codes; the team reports the same Rust 1.98.1,
nano, pinned CPU conditions. These exported means were inspected, not
independently rerun:

| Mode | `std` mean ± standard deviation | `std,emit` mean ± standard deviation |
| --- | ---: | ---: |
| Return immediately | 0.483 ± 0.069 ms | 0.463 ± 0.017 ms |
| Default context | 3.499 ± 0.413 ms | 3.425 ± 0.031 ms |
| Compile, execute 42, print | 3.666 ± 0.032 ms | 3.696 ± 0.018 ms |

The context-only `std` result has substantial outliers; its lower `emit`
mean is not evidence of an optimization. The run result has a small positive
mean difference of 0.030 ms, about 0.8%. Standard deviations alone do not
prove equivalence or establish that every difference is noise. The supported
conclusion is that this probe shows a small effect, not that `emit` is
universally runtime-free. It does not exercise emitting diagnostics.

The probe README records sizes of 7.76 and 7.89 MiB, an approximately
0.13 MiB difference. This controls the feature comparison better than the
different companion/phase programs do. It still does not decompose the
entire companion binary's growth into reqwest, Tokio, modules, and other
code. The original 4.6 MiB growth report is not established by this rerun.

The new lockfile packages, excluding the probe itself, are
`codespan-reporting` 0.11.1, `termcolor` 1.4.1, `unicode-width` 0.1.14,
and `winapi-util` 0.1.11. The last includes target-specific behavior;
record 0029's target-aware workflow determines the actual notice set.
Rune defaults enable `std` and `emit`, not `doc`. rnx's custom diagnostics
remain its user-facing path regardless of the newly resolved emission code.

With the published rune-modules dependency as it stands, disabling defaults
on rnx's direct Rune dependency cannot remove the transitive `emit` request.
Vendoring is not the only possible route to a smaller feature set: an
upstream dependency change or independent adapters can change that graph.
No such change is required or implemented by this evidence update.

## Validation performed

- Read the published source and checked HTTP registrations against its
  module constructor.
- Compared the proposed execution and memory requirements with rnx's
  existing synchronous paths and records.
- Reviewed both probe sources, manifests, the preserved timing observations,
  and the companion crate's resolved Rune features.
- Checked the documentation diff for whitespace errors.

No implementation tests or new benchmark runs were performed for this
documentation-only record. Its implementation acceptance gates remain open.
