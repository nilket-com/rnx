# rnx 0035 evidence: whole filesystem values

Implemented and measured by Codex on nano, Linux, Intel i7-14700,
2026-09-13, Rust 1.98.1, Rune 0.14.2. The plan commit is `f4eb316`.
Linux execution is verified; Windows execution is not claimed.

## Implementation and gates

`src/fs.rs` installs exactly eighteen names and supplies the descriptions
used by completion and help. The old four `host::` names are absent.
Live scripts, tests and README use the replacement names; earlier records
retain their historical wording. The stdin/file reader ceiling is one
constant, `fs::READ_LIMIT`, 8 MiB. Existing JSON and HTTP implementations
are unchanged; there are no new shipping dependencies or feature changes.

The new module gates cover:

- Text and bytes at 8 MiB and one byte beyond; invalid UTF-8 gives the
  offset and bytes-reader hint. Unix FIFO opens use O_NONBLOCK, and a
  subprocess timeout verifies refusal without a writer. Directories are
  refused. This does not establish a deadline for other open operations.
- String and Bytes writes, exclusive creation, in-place replacement,
  append/create, and invalid data refused before creating or truncating.
  Unix mode bits and a hard-link alias survive the ordinary write fixture.
- Missing paths, non-directory parents, path-bearing errors, exists versus
  permission failure, metadata and link fields, modification time within
  five seconds of the fixture clock, and a utimensat time one nanosecond
  before epoch reported as -1 ms. Unit gates cover flooring and overflow.
  Permission gates ran as uid 1003, not root.
- Sorted Unicode names, 100000 entries accepted, 100001 refused, and an
  invalid Unix filename refused with an escaped name and parent path.
- Copy and rename onto an existing target preserve both contents and
  modification times. Dangling target links are refused too. Successful
  copy preserves permissions and successful rename moves the source.
- Exclusive rename dispatch is isolated in `fs_platform.rs`: Linux
  renameat2/RENAME_NOREPLACE, macOS renamex_np/RENAME_EXCL, Windows
  MoveFileExW with flags 0. The argument/flag unit gate reaches the call
  with nonexistent paths; the dispatcher has no existence check or fallback.
  Atomicity rests on the OS primitive, not on winning a timing race.
- Copy validates its open source first. Under test-support only,
  RNX_TEST_COPY_FAIL_AFTER=4 injects a reader error: the destination is
  exactly `0123`, and the refusal names both paths and that it was left.
  The ordinary build does not read this environment variable.
- Recursive deletion removes internal and top-level links without removing
  targets, including dangling links and a link to cwd. Root, cwd and its
  ancestor are refused. The canonicalisation guard is not an atomic defense
  against concurrent replacement of the supplied path.
- File, eval and piped-session entry points; all eighteen names compile and
  have help text. Existing terminal completion gates use the now-unique
  `fs::write_` prefix, and check host listing membership without assuming
  screen-column order.

## Verification

Final suites were run sequentially with TERM=xterm-kitty, to avoid replacing
an integration test's executable with another feature build while it runs.
Piped-session helpers explicitly choose TERM=dumb; PTY gates use xterm.

- `cargo test --locked`: 263 passed, zero failed.
- `cargo test --locked --features test-support`: 301 passed, zero failed.
- `cargo build --release --locked`: passed.
- `scripts/third-party-notices.sh --check`: unchanged and passed,
  121 packages, 87 texts, 13 fetched, the same one unresolved item.
- New module/test files pass rustfmt; product diff whitespace check passes.

The complete Windows cross-check stopped while building ring: this Linux
host lacks MSVC `lib.exe`. The independent rnx-bench
`probes/fs-portability` crate instead includes the actual fs and fs_platform
source, using a minimal host-registration record. Its locked Windows
`cargo check --tests --features test-support` passes, including the Windows
exclusive-rename argument gate's types. Its lockfile and raw check output
are preserved. This is neither an application cross-build nor Windows test
execution. The portable integration gates are authored for the Windows
suite but remain unexecuted here. macOS is unexecuted as well.

## Measured cost

Preserved command script: rnx-bench `scripts/measure_fs_0035.py`.
Preserved together at rnx-bench commit `147093a`.
Raw exports: `results/fs_0035_startup.json` and
`results/fs_0035_conditions.json`. Core 4, hyperfine -N, 10 warmups,
50 runs per command, release binaries. Each row includes process startup.
The read workload evaluates 100 reads of a 4096-byte regular file, comparing
the old host name with the new fs name. Captured exit status, full stdout
and stderr agree before and after for every workload except help, whose
expanded surface is intentional and whose outputs are both preserved.

| workload | before ms, mean ± sd | after ms, mean ± sd |
| --- | --- | --- |
| version | 0.558 ± 0.021 | 0.544 ± 0.031 |
| help | 0.547 ± 0.022 | 0.534 ± 0.025 |
| eval 42 | 4.017 ± 0.019 | 3.998 ± 0.018 |
| bare run | 3.701 ± 0.023 | 3.744 ± 0.217 |
| 10k JSON | 11.785 ± 0.098 | 11.516 ± 0.082 |
| 100 reads of 4096 bytes | 4.431 ± 0.071 | 4.407 ± 0.020 |

No startup or file-read regression was detected at this resolution. The
JSON difference is not attributed to this change: the serializer is untouched
and these short process runs carry drift.

Binary sizes: 14,142,008 before, 14,262,968 after,
a growth of 120,960 bytes.

SHA-256 before: `7165be726c82f00870d968c976897e8b1cdbe14547e5b5055f0a4a34959da5dc`.
SHA-256 after: `e7ca8e29a8adea5bd62a11c0478fdf1b2c73f5f5936384d0fb97a3269fbdd7ce`.

Piped-session startup allocation references: 1,813,610 bytes before and
1,818,697 after (+5,087). Live allocations at the first memory command were
1,822,061 and 1,827,161 (+5,100). These measure allocator requests, not RSS;
the extra registrations have a small measured cost, not zero cost.

## Remaining scope

Windows execution evidence remains outstanding; no result from Linux is
presented as its substitute. The synchronous filesystem functions have no
mid-call cancellation or deadline. Record 0031's separate async CPU-loop
interruption clause remains open. No reviews are saved in git and no
upstream issue has been filed by this implementation.
