# 0058 gate 3: bounded native preview

Gate 3 is ready for review on Linux. Gates 1 and 2 are accepted and pushed;
gates 4–6 remain open. This step adds preview to the adapter only. Root source,
all manifests/lockfiles, notices, kernel, PostgreSQL adapter, server and project
tool are unchanged.

## Surface and structural bounds

`frame.preview() -> Result<String>` borrows the frame and returns text without
printing, collecting or performing I/O. The first line is the full dimensions,
answering the reviewer's size question without adding an accessor. Displayed
column names carry their dtypes. Strings are quoted, so null and "null" differ.
There is no registered formatter for a bare frame.

The implementation is in `adapters/polars/src/preview.rs`:

- Loops explicitly stop at ten rows and eight columns. Dimensions come from
  existing frame metadata. It never formats a whole frame or generic AnyValue.
- Names and string cells iterate `char_indices().take(80)`. The ending byte
  offset compared with the known string length detects remaining input without
  fetching an eighty-first scalar. Omissions are marked outside the quotes.
- Controls, quotes and backslashes are escaped while processing those bounded
  scalars. Combining scalars and emoji remain valid UTF-8; this is a scalar,
  not grapheme-cluster limit. Selected directional and Unicode line controls
  are escaped as well. No ANSI is generated.
- Scalar numeric formatting handles only the supported concrete types. f64's
  compact Debug spelling round-trips finite values, including negative zero and
  the smallest subnormal, without a hundreds-of-digits fixed decimal expansion.
- Every append admits or refuses a complete bounded token. It reserves the final
  byte-limit marker before admitting data. Thus the returned UTF-8 string never
  exceeds 8192 bytes and never contains half an escape or scalar. A byte refusal
  stops inspection immediately; it does not first render the remaining rows.

Names and cells can be clipped while dimensions remain complete. Row/column
omissions have their own explicit counts. Byte exhaustion names the omitted
remainder, even when it occurs partway through a displayed row. No padding or
terminal-width calculation is involved.

## Evidence

Bench driver: `probes/polars-preview/check.py`.
Results: `results/polars-preview-0058/{ordinary,test-support}/`.
Both configurations pass through actual Rune file execution. Text specimens
are the exact returned preview strings, printed explicitly by the fixture.

The driver covers 0/9/10/11/100 rows, 1/7/8/9/100 columns,
79/80/81/100000-scalar names/cells, ESC, tabs, quotes, backslashes, embedded
newlines, combining characters and emoji. It verifies the full dimensions and
expected count of displayed values, scalar omission markers, and a byte-limited
frame whose entire escaped tokens remain intact. It also covers i64 extremes,
nullable booleans, null versus string null, finite float bit round-trips,
infinities and NaN. Each preview is called twice on the same bound frame.

Separate eval checks prove preview has no stream side effect and native values
remain opaque. A real piped session returns the opaque frame and then explicitly
prints its preview with the expected dimensions and content. Notebook execution
is still gate 4; no notebook acceptance is claimed from these checks.

Four new Rust tests independently cover row/column boundaries, scalar boundaries
including a million-scalar input, escaping, null/numeric spelling and the exact
8192-byte output boundary. Code structure, not a timing threshold, bounds work.

Checks at this change:

- Formatting and clippy with warnings denied pass in both configurations.
- Ordinary Rust suite: 6 passed; test-support: 7 passed, serially.
- Both preview drivers pass; the unchanged gate 2 contract driver also passes
  (pipeline, both Parquet producers, 16 CSV cases and 29 refusals).
- Notices check passes unchanged, with the same three missing texts explicit.
- Root suites, Windows and performance measurements remain their later gates.

One fixture correction: the first driver included the file script's returned
`0` after the printed preview. It now returns unit, isolating the preview bytes.
There was no product bound failure.

Ordinary release SHA-256:
`1eb302d4b916d7629ca83de0d0020cd5a1fb69e5670d554698fdc185a15c6d83`.
Builds use the existing cache; no build-time or speed claim. The README now
prints the tiny example's dimensions and values through preview, and documents
its limits and continued native opacity.
