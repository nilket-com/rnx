# 0069 gate 3: lifetime, removal and the catalogue declarations

Status: passes on Linux, ready for review; second revision after review. The product is `e69d06b`: gate 2
plus the catalogue writing `shared_build = true` for Git-authored `polars`
and `postgres` declarations, with the READMEs. Gate 4 remains. The probe is
`rnx-bench/probes/shared-build-lifetime/`; results in
`rnx-bench/results/shared-build-lifetime-0069/`. PostgreSQL queries run
against the 0055 fixture cluster.

## The fixtures

A bare Git origin of the product tree at three revisions: the tree; plus two
retained-output natives in 0061's shape — an *embedding* reader whose build
script writes `OUT_DIR/retained.txt` and whose extension reads it through
`env!` (the path is in the executable), and a *runtime-configuration* reader
whose build script writes the value of a declared environment input
(`rerun-if-env-changed=PROBE_VALUE`) and whose extension learns the file's
path at runtime from `PROBE_RETAINED_PATH` (the path is not in the
executable; the review's counterexample); plus a third revision changing the
embedding reader's build script. Every executable is observed through the
notebook worker after every step: a bare frame, a real
`postgres::query(url, "SELECT 40 + 2 AS answer, current_database() AS db",
[], #{}).await` when the adapter is present, and both readers' values; the
configured reader is always asked for the path it was first given, so a
removed file shows as a missing file at that path. After every step the
four supported executables must answer exactly as at the first observation,
both private readers must keep their values, and the runtime-only project
must evaluate `40 + 2` to `42` through `project eval` (the review's first
finding: the earlier driver dropped the path and checked only the end).

## The sequence (`lifetime.json`)

Built and declared shared: runtime-only (40.5 s, 142 entries), Polars
(108.1 s, 229), PostgreSQL (35.6 s, 39), Polars + PostgreSQL (39.4 s, 13),
all in one directory. The runtime-configuration reader declared shared:
published (4.7 s; the scan cannot see it). The embedding reader undeclared:
private (41.0 s). The embedding reader as a *path* native under a *path*
runtime: private, a format-2 identity with no build kind (37.8 s). No shared
executable holds the directory's path (checked byte-wise before anything
else).

After the first builds: Polars presents; PostgreSQL and the combined
executable answer `{"affected": 1, "columns": ["answer", "db"], "rows":
[{"answer": 42, "db": "postgres"}]}`; the configured reader `Ok("first")`;
both private readers `Ok("cache-owned retained value")`. Then:

1. **A later shared build** (another Polars wrapper, 5.0 s, 1 entry): every
   executable unchanged.
2. **A same-revision build-script re-run**: a new wrapper carrying the
   configured reader with `PROBE_VALUE=second` builds in 5.3 s, 2 entries;
   the reader's build script re-runs into the same `OUT_DIR`. The *older*
   configured executable now reads `Ok("second")` — the documented
   consequence of a false declaration, observed, not a product defect. The
   shared runtime, Polars, PostgreSQL and combined executables are unchanged;
   both private readers keep their values.
3. **A build-script change at a new revision** (private, 40.8 s): the new
   executable reads `value at the third revision`; the old private reader
   keeps its own; everything shared unchanged.
4. **Removing an entry** (the second Polars wrapper's): the shared directory
   stands; everything else unchanged.
5. **Removing the shared directory**: `rnx cache list` shows it with six
   referencing entries; `remove build-<key>` without `--quiescent` refuses
   with the 0066 wording; with it, 4,046 leaves are deleted. Afterwards the
   runtime, Polars, PostgreSQL and combined executables answer exactly as at
   the first observation (the PostgreSQL query included); both private
   readers keep their values; the falsely declared configured reader fails
   with `Err("<its original path>: No such file or directory")` — the second
   documented consequence. The runtime-only `eval` answers `42` after every
   step.

The evidence the plan asked for before the catalogue may declare: a
runtime-only executable, a Polars executable, a PostgreSQL executable with a
real query, and the combined executable, each published from a shared build,
each holding no reference to the directory, each unchanged through a later
build, a re-run, a revision change, an entry removal and the directory's
removal. Both declarations are therefore written. The record's claim is what
gate 3 tested on this toolchain and graph; the maintainers vouch for the
adapters' graphs going forward, and a change to a graph that added such a
reader would need the declaration dropped.

The tool README's guarantee is stated conditionally (the review's second
finding): where the declaration's promise is kept, executables from shared
builds hold no reference and removal breaks none; the tool refuses the
executable that holds the directory's path and cannot detect a reader that
learns it another way, so the guarantee rests on the declaration.

## The catalogue (`catalogue.json`)

`rnx project add polars postgres` on a Git-runtime manifest writes
`shared_build = true` on both tables (and `presentation = true` on Polars);
the project locks `shared`. An existing Git declaration without the field is
left byte-for-byte alone by a later `add` and locks `private`. On a path
runtime `add` writes no such field. The tool suite's
`git_authoring_writes_the_declaration_and_keeps_existing_choices` covers the
same three cases.

## Checks (`checks.json`)

Tool formatting, strict clippy in both configurations, tool suites 72 and
73, tool notices, root default suite 390.

## Qualifications and next gate

The interrupted-removal, resume and builder-racing cases are gate 2's
evidence and are not repeated. Costs here are single runs; gate 4 samples
against the pre-record baseline, runs the three root configurations, adapter
suites, notices and audits, and asks the user to repeat `:dep polars` then
`:dep postgres` on slim. No Windows claim.
