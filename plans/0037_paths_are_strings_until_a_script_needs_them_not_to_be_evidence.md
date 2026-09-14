# rnx 0037 evidence: paths are strings

Implemented and measured by Codex on nano, Linux, Intel i7-14700,
2026-09-14, Rust 1.98.1, Rune 0.14.2. Plan commit: `fba261e`.
Raw evidence and the measurement driver are in rnx-bench commit `64b8783`,
`results/path_0037_*` and `scripts/measure_path_0037.py`.

## Implementation and gates

`src/path.rs` installs eight free functions into the shared context, available
in run, eval and the session, including after reset. No dependency or feature
changes. Each function is present in the host-function inventory with help;
version and command-line help still return before context construction.

All transformations delegate to std::path on Unicode input. Conversion back
uses an invariant assertion, not lossy conversion: these operations remove
components or concatenate valid strings and ASCII separators, and cannot
introduce invalid UTF-8 or an unpaired Windows surrogate. `with_extension`
checks std::path::is_separator before calling Rust, even for paths without a
filename. A refusal is a catchable string naming the path and extension with
escaped spelling. No function accesses a filesystem or resolver; that claim
is established by reading the module, not inferred from a fixture's success.

Five Linux integration gates cover the contract table, Unicode reassembly,
extension removal and separator refusal, lexical NUL handling, existence
independence, and session help/reset. A unit gate pins exactly eight names
and compiles each reference. Windows has an additional integration gate for
rooted and drive-relative parts, absolute paths and verbatim normalization.
The existing Windows suite discovers it automatically.

One final spelling correction preceded the plan commit: reassembling `/a/b`
on Windows produces `/a\\b`, retaining the existing leading slash. The plan
and gate use that spelling. This follows the ordinary relative-append branch
of [Rust's PathBuf implementation](https://doc.rust-lang.org/src/std/path.rs.html),
which appends a main separator without rewriting the base. Windows answers
remain source-derived expectations, not execution results on this machine.

| check | result |
| --- | --- |
| TERM=xterm-kitty cargo test --locked | 278 passed, zero failed |
| TERM=xterm-kitty cargo test --locked --features test-support | 316 passed, zero failed |
| cargo build --release --locked | passed |
| scripts/third-party-notices.sh --check | unchanged; passed |
| new source/test rustfmt and git diff --check | passed |
| isolated Windows module/unit-gate type check | passed |

The full suites ran sequentially so their different feature builds could not
replace the executable while another suite used it. Notices retain 121
packages, 87 texts, 13 fetched and the existing one unresolved entry.

The isolated check is rnx-bench's `probes/fs-portability`, extended to include
the actual path module. It does not execute Windows tests or build the whole
application; the previously recorded MSVC TLS toolchain limitation remains.
Windows execution is unverified.

## Before and after

Before is the default-feature release binary at the plan commit (product
source from 0036), built with `cargo build --release --locked` and copied
before editing. Its hash matches 0036's measured after binary. After is the
same build command with the path module. Both hashes, sizes, versions, exact
commands, complete output checks and memory output are preserved in
`results/path_0037_conditions.json`.

Hyperfine 1.20.0, `-N`, CPU 4, 10 warmups, 50 runs per command, controlled
PATH and TERM=dumb. Elapsed process times include startup and destruction.
The driver checks successful exits and before/after output equality (help is
allowed to differ); the measured commands have identical outputs.

| command | before mean ± standard deviation | after mean ± standard deviation |
| --- | --- | --- |
| version | 0.530 ± 0.019 ms | 0.509 ± 0.017 ms |
| help | 0.518 ± 0.017 ms | 0.509 ± 0.020 ms |
| eval 42 | 4.022 ± 0.021 ms | 3.976 ± 0.013 ms |
| run bare file | 3.681 ± 0.016 ms | 3.651 ± 0.023 ms |
| JSON workload | 11.455 ± 0.057 ms | 11.600 ± 0.071 ms |

The JSON difference is +0.145 ms (about 1.3%); this run does not isolate its
cause. Small decreases in other rows are observations, not speedup claims.
No filesystem or path-operation throughput is measured here.

| size/accounting | before | after | change |
| --- | --- | --- | --- |
| release binary bytes | 14,288,904 | 14,349,432 | +60,528 |
| session startup reference bytes | 1,814,852 | 1,817,885 | +3,033 |
| live request bytes at :memory | 1,823,442 | 1,826,475 | +3,033 |

These are allocator request counters, not resident memory. The path value
remains deferred. This record does not close 0031's separate async CPU-loop
interruption clause or claim Windows execution.
