# 0058 gate 2: CSV and Parquet through the product adapter

Gate 2 is ready for review on Linux. Gate 1 was accepted and pushed at rnx
ed6367d and bench 89bf8e5. Gates 3–6 remain open; this is not a performance,
preview, notebook or Windows acceptance claim.

## Product boundary

`adapters/polars/` is an independent workspace with a thin rnx-polars executable
and a reusable build function. The pinned Polars features remain exactly lazy,
csv and parquet at 0.55.2. The accepted prototype lockfile changes only the local
package name and its direct libc edge (already resolved), used for non-blocking
regular-file opens on Unix. Tokio is an optional direct dependency for
`test-support` context assertions. No resolved package version changed.

The four native wrappers borrow their receivers. Input arrays are inspected
through borrowed Rune values and copied into owned Polars inputs. Engine I/O
and collect run on the named, scoped thread from gate 1; the caller joins it
before constructing the Rune wrapper. Ordinary builds contain no observation
counters. Test-support adds the counters and asserts no entered Tokio runtime
inside each engine call. Unexpected panics are joined and resumed.

The API table now explicitly says Result for group_by, agg and sort, matching
the accepted prototype's fallible array parsing. Invalid arrays and schema
shapes refuse catchably; no root change is needed. Preview is deliberately not
registered in this gate and remains gate 3's work.

## Independent answers

The driver is `rnx-bench/probes/polars-contract/check.py`. Its final observations
are in `results/polars-contract-0058/{ordinary,test-support}/results.json`.
Both configurations pass: pipeline, both Parquet producers, 16 CSV cases, all
four types and nulls, reused values, empty results, symlink input and 29 named
refusal cases. Every product invocation is a real Rune file run with a 30-second
external deadline, not a direct call into the adapter's Rust API. Each process
exits, and temporary data is removed after assertions.

The fixed input rows are (a,1), (a,2), (🦀,3), (🦀,4), (null-string,null).
Filter v > 1, group by k, sum and sort must produce exactly (a,2), (🦀,7), with
string and i64 columns. Python independently reads the Rune-written Parquet;
a Python-written uncompressed Parquet is read and rewritten by the adapter and
checked again. The original frame remains unchanged. Paths, schema, keys,
aggregates, sorting names, plans and frames are reused. A missing-column query
and an existing-output refusal are followed by successful operations using the
same bindings. The README's complete script also passes with its independently
read expected output.

CSV observations match the pinned Python reader for accepted data: CRLF,
quoted commas/newlines/doubled quotes survive; short data rows pad with null;
unquoted empty fields become null; quoted empty strings remain strings; header
only gives an empty typed frame. Extra fields, invalid integers, wrong header
names/order/arity, duplicate headers, empty files and invalid UTF-8 refuse.
Schema errors are detected even with an absent input path, before opening it.
Parquet errors include malformed data and unsupported i32/u64/date/null dtypes;
a bool sum producing an unsupported dtype is also refused at collect. A mode-000
input proves permission refusal under the ordinary unprivileged fixture user.
FIFO and directory reads refuse without hanging. Symlink input succeeds.

## The two non-vacuous file tests

The regular suite calls the production engine helpers, with only a private test
callback at the header/data boundary. After Polars validates the raw header,
the fixture renames the input and writes different data at its old pathname.
The same File is rewound and the typed read yields 7 from the original inode,
not 99 from the replacement. Both files remain until this observation is made.
No second parser or path reopen is involved.

The production Parquet-writing helper is exercised with a real file-backed
writer that fails after 16 bytes. The new 16-byte file remains and a retry
refuses its existing path. Separate faults on the engine flush and the final
adapter flush each retain 489 bytes and return an error. Sizes and error strings
are printed in the test logs before fixture cleanup. The partial-write engine
error is a generic Parquet transport error: Polars does not retain that injected
writer's message. The test observes the exhausted writer allowance and actual
retained bytes instead of assuming an upstream error spelling. This does not
promise atomic output or successful-file durability.

## Checks and limits

- Formatting and clippy with warnings denied pass in both configurations.
- Ordinary Rust suite: 2 passed. Test-support: 3 passed, including joined panic
  propagation. Suites run serially; the expected panic marker is in the log.
- Both product contract runs pass and have empty child stderr.
- Notices check passes: 325 packages, 215 distinct texts, three missing texts
  explicitly inventoried (alloc-stdlib, polars-parquet-format and syntree).
  Completing the notices/platform/root regression gate is still gate 6.
- The generator compares bytes because some shipped licence texts contain CRLF;
  a text-mode comparison falsely reported an unchanged inventory as stale.
- Root source, root manifest/lock/notices, kernel, PostgreSQL adapter, server and
  project tool are unchanged. Root suites are deferred to gate 6, not claimed
  from this adapter-only change.
- No Windows execution or type-check was performed for this gate.

Ordinary release SHA-256:
`16c859d664b29227638845c93e53c993651ea50bd7da63b4c0109634edf1d27d`.
The release cache was seeded from the accepted prototype; build logs are
correctness evidence, not cold-build measurements. The resolved graph and
conditions are stored with the bench results. Rust and Python engine revisions
still differ as gate 1 recorded; no boundary-only or speed conclusion is made.
