# Implementation plan 0286: Every Python test has a route

Status: Implemented

Paired ADR: ../adr/0286_every_python_test_has_a_route.md.

1. Pin 0bacd2c7. Add the Rust control that runs the controller's disk
   rule and `ci/disk_space.py` on the same planted paths and requires
   them to agree; run `ci/test_disk_space.py` and
   `ci/test_disk_launchers.py` in preflight.
2. Add the two stages of decision 3 to the Rust catalogue, and update
   by hand, for exactly those two stages, each witness fixture they
   change, marking it a reviewed ADR 0286 successor of the 0284 witness.
3. Retire the instruments and data of decision 4; extend the tombstone
   check; update the data registry.
4. Add `test_callgrind_n1_cold.py` to `TESTS` with its discovered count.
5. Add the route column and its Rust check with planted cases.
6. Run the affected checks in a final-like environment, preflight and
   the commit checker; sign; source review; one complete Rust final.
   Results state what the code does and the final's duration.

## Results

Every Python test file in `tools/python-files.txt` names its route, and
`rnx-final python-list` holds each to the entry point it names: 65 run
in the complete final (the `tools/column` and `tools/routing-bench`
discoveries, `pin-controls`, the two new stages, and the Jupyter driver
through `jupyter_controls.py`), 6 in preflight, 47 in `cargo xtask suite
measure-checks` and 5 in the supplemental workflow, the last two outside
the final; 3 are deferred with their reasons (the CLI capture helpers'
tests and the comparison harness). Each route is held to its entry
point's active declarations, comments removed: preflight's unittest
commands, the catalogue stage's command or discovery, the supplemental
workflow's discovery call, the `commands` declaration a stage script
iterates, and the measurement suite's `TESTS` list, whose every count
must equal the cases unittest discovers in its file, found once for the
family with the interpreter and isolation that suite runs them with
(`/usr/bin/python3 -I -B -S`), so no `PYTHONPATH` module or site setup
changes the count. A test file
without a route, a route on any other file, an entry point that does not
declare the file, a commented-out call, a path kept only in a list the
wrapper never runs, a wrong or zero count, and a
deferred route without a reason each fail, with planted cases. That
count check found `test_build_recipe.py` declared with 3 cases while it
holds 4, so `cargo xtask suite measure-checks` failed on it; it now
declares 4, and the suite, which runs outside the final, passes.

`engine-source-controls` and `census-refresh-controls` run
`tools/test_engine_source.py` and `tools/test_refresh_census.py` as
stages of their own, scheduled as `pin-controls` is and required when
their tool or test changes. The catalogue, scheduling and selection
witness fixtures carry that addition as a reviewed ADR 0286 successor of
the 0284 witness. `test_callgrind_n1_cold.py` joins the measure-checks
list with its ten cases.

The disk admission keeps two implementations while its Python launchers
remain: the controller's (`disk_check`, which collects its paths with
`disk_paths` and judges them with `disk_observation`, reading real free
space) and `ci/disk_space.py`. A control runs both through their own
reading seams on the same readings and requires the same record byte for
byte, and the verdict the controller acts on to agree with its record:
for given paths (passing, failing, exactly the floor, judged at an
ancestor) and from each entry point with its own path collection, where
a full `CARGO_TARGET_DIR` fails both and an absent or empty one counts
for neither. A floor changed on either side, a flipped verdict, or either
side dropping its target directory fails it. Preflight runs
`ci/test_disk_space.py` and `ci/test_disk_launchers.py`.

`tools/redact_metadata.py` and the replay instruments
`n3_qualification.py` and `e1a_qualification.py` are retired with their
tests and their data (`qualifications/n3.json`, `n3-proof.json`,
`e1a.json`, recoverable at 0bacd2c7), and `n3-proof.json`'s data
registration with them; `tools/final/tests/retired/tools-0286.txt` lists
the nine files, and a check refuses any current path to them, inside
the Rune tree too, or import of them. The list holds 323 files.

The complete Rust final passed in 45.7 minutes, against ADR 0285's 46.6
minutes, with 1,371 memory.high events and no memory.max or OOM events.
