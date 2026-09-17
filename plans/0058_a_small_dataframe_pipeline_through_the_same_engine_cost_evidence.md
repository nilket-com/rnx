# 0058 gate 5: measured launch costs and native ownership

Gate 5 is ready for review on Linux. Gates 1–4 are accepted and pushed at rnx
9911612 and bench 7e7c5fb. Gate 6 remains open. Production code, all manifests,
lockfiles and dependency graphs are unchanged by this gate.

## The result, including the slower product

Two interleaved repeats, thirty samples per product/workload/repeat. Medians
in milliseconds; peak RSS is the median of three separate pipeline launches.

| Product | Initialization | Full tiny pipeline | Pipeline peak RSS |
| --- | --- | --- | --- |
| Python Polars 1.44.2 | 96.71 / 96.80 | 99.87 / 99.99 | 86.5 MiB |
| Ordinary rnx-polars | 6.76 / 6.73 | 11.44 / 11.47 | 45.9 MiB |
| Verified rnx-project run | 150.76 / 150.11 | 155.10 / 154.81 | 48.7 MiB |
| Generated artifact directly | 6.61 / 6.69 | 10.88 / 10.85 | 48.6 MiB |
| Feature-aligned direct wrapper | 6.77 / 6.80 | 11.49 / 11.45 | 46.4 MiB |

For this tiny end-to-end workload on this host, ordinary direct rnx-polars is
about 8.7 times faster than Python. Verified project launch is about 1.55 times
slower than Python. These are product wall-clock observations, not a compute or
language-boundary attribution. The generated/direct difference isolates roughly
144 ms of project verification and launch work, including source inventory,
artifact hashing and map publication. It is not all source-hashing time.
The smaller difference between direct wrapper shapes is not assigned a cause.

The project fingerprints 6,977,812 bytes across its distinct source/native trees,
plus ancillary Cargo/config inventory, and verifies its roughly 107 MB executable.
The source-map feature is the only dependency-feature difference between the
ordinary and generated graphs. The additional aligned direct build enables it;
all dependency package versions/features then match the generated assembly,
excluding the wrapper packages. Generated-direct uses the exact project artifact,
entry file and source map. Nothing is inferred from two supposedly identical
but unverified binaries.

## What the stopwatch contains

`rnx-bench/probes/polars-cost/main.rn` and `main.py` implement the same two modes.
Initialization actually registers/imports Polars and constructs a literal; it
is not version or help. The pipeline creates CSV, validates its raw header,
rewinds the same handle, reads with an explicit schema, filters, groups/sums,
sorts, collects, writes uncompressed create-new Parquet, flushes/closes, reads it
back, compares previews and collects again. Both return the same small preview.
Python's two-row iteration is presentation only, not a query UDF. The query
itself stays in the native engine in both products.

The harness pins one allowed CPU (recorded in conditions) and one Polars thread
before process creation. Both modes are warmed. Python uses normal bytecode
caching; 182 site-package bytecode files are present after warm-up. Order is
interleaved with a fixed seed. Each of the 600 corrected observations is retained
both in the journal and samples.json; they compare exactly. No corrected samples
or outliers are dropped. Summary p95 and raw RSS observations are also retained.
The clock includes no-shell spawn/capture/wait overhead. Fresh output directories
are created and removed outside timing. These are warm filesystem/process
launches, not cold-cache measurements, and flush/close is not fsync durability.
GNU time observes maximum RSS of each launch including native threads, not the
sum of concurrent process-tree RSS.

A rejected preliminary run inherited PYTHONDONTWRITEBYTECODE=1 from correctness
fixtures. It was stopped when that mismatch was noticed, before its values were
used. Its complete first block is preserved under preliminary-no-bytecode; the
interrupted second block had not yet been saved. The corrected driver journals
every completed observation and permits normal cached imports. No engine version,
compiler flag or thread count was selected after inspecting which product won.

## Setup cost and provenance

With cached crate downloads, no network and two Cargo jobs, the empty-target
release build took 505.48 seconds. Warm rebuild: 0.19 seconds. The resulting
107,400,144-byte binary exactly matches the accepted ordinary artifact's hash on
this host. That is an observation, not a general reproducible-build guarantee.
Private Python venv creation took 0.613 seconds; installing the exact cached
wheels took 0.479 seconds. Network download time is not measured. The Python
site-packages tree after warm-up is 188,833,673 bytes including bytecode, in
addition to its interpreter and standard library; comparing the interpreter
file alone with a statically assembled executable would be misleading.

The exact gate-one wheel hashes and all 213 installed Python/shared-library code
files match again. Python is 3.14.4; Rust is 1.98.1. The Rust crate's revision
and Python tag still differ. Wheel build_info exposes its version, not a full
compiler/allocator configuration. The standard wheel is fixed by hash; rnx uses
its counting System allocator. CPU information, known CPython build settings,
flags and hashes are recorded. No same-revision wheel/native pair was built, so
boundary-only attribution and matching allocator/CPU-dispatch claims remain
explicitly unproved. In particular, subtracting initialization time from pipeline
time would not turn this into an engine or VM benchmark.

## Allocation, thread lifetime and Ctrl-C

The separate ownership fixture reads two million i64 pairs from CSV in a session
with a 16 MiB ceiling. Tracked live requests rise from 1,842,020 to 33,872,258
bytes. A marker after read returns is printed within the same input, proving the
native call and following script code ran before the boundary sample latched
the ceiling. The next input refuses. Reset drops tracked live requests to
1,864,373 bytes. This proves native allocation visibility and the sampled
ceiling's limitation without an OOM experiment; it does not count mapped memory
or every allocation outside Rust's allocator.

/proc snapshots show no call-owned rnx-polars engine thread and no retained CSV
handle after return or reset. Polars' polars-0 and polars-unmap-0 threads persist
through reset, separately identified rather than labelled leaks. Process exit
reaps them with the session.

The collect probe observes a native engine thread still present after a 50 ms
checkpoint, then sends SIGINT. All calls finish their native work. The ordinary
file that completes in the same poll retains successful completion (exit 0), as
the runner's existing Finish policy specifies. The variant with a subsequent
await prints COLLECT_RETURNED, then reports interruption (exit 130) without
reaching AFTER_AWAIT. That run ends about 312 ms after the signal; the finish-only
run ends about 321 ms after it. These are observations, not cancellation gates
or bounds. A control without a signal completes normally. Every process has an
external timeout plus kill/reap cleanup and is confirmed gone.

## Artifacts and remaining work

Scripts and README: `probes/polars-cost/` in rnx-bench.
Raw samples, journal, graphs, locks, setup logs, hashes, wheel provenance,
ownership observations and summary: `results/polars-cost-0058/`.
Ordinary SHA-256 remains
`bb2a24526f10b3a0f0255c7173117a5aeeb2d4ce6133477c094d67a830806907`.

The Python scripts parse, both workloads verify every measured output, the
journal has all 600 corrected samples, and ownership/provenance checks pass.
No Rust production code changed; root suites, final notices work and Windows
checks remain gate 6. Project locks preserve identities from the measured tree
before this evidence/README edit; rerunning requires the explicit lock/build
steps, as usual for a tracked native path dependency.
