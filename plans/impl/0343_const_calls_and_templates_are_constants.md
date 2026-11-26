# Implementation plan 0343: Const calls and templates are constants

Status: Proposed

Paired ADR: ../adr/0343_const_calls_and_templates_are_constants.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. Find how a
   `const` item gets its value and whether the same path can serve
   a call in a function body. List the macros that take a template.
   Find which roots the check of ADR 0342 needs for the two forms.
   Measure the old core's outcome for the planted forms of Controls
   1 to 6. Report before going on if a cause is beyond Decision 8.
   That part then leaves the record as Decision 7 states.
2. Const calls as Decisions 1 to 4 state, with Controls 1 to 5.
3. Constant templates as Decision 5 states, with Controls 5 and 6.
   Control 8 covers both forms.
4. Plant the two refusals back, one at a time. Replay the public
   admission corpus against the published base. List every changed
   source as Control 9 states. Send the list for review before
   signing if anything but the three cases changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.

## Results

A body call now uses the same bounded evaluator as a constant item.
Each immediate argument is a separate constant entry. In
`add({return 2; local}, {return 3;})` the result is 5. A reference
to a runtime local in a branch that the compiler checks still gives
the original missing-variable error. The structural check follows
called constant functions in their own HIR arenas. The existing
constant lowering constructs a new value each time a site runs.

All seven formatting macros use the same template expansion.
A computed string goes through the constant check and evaluator,
then through the literal format parser. The installed runner ran
325 boundary programs in both native modes, on the automatic route
and on forced legacy. The tests included the seven macros with
literal, block and named templates, invalid arguments and format
specifications. All 27 planted invalid forms across the three new
entry paths kept their diagnostics. A reached print in a body call
kept ConstCall and ran once on fallback. A template block that
prints while being evaluated kept FrontendDiagnostic and printed
twice, as on the base.

At 450,000 iterations the item, the call in a body and the
const-function template all yielded 7 on execution. On forced legacy
each exceeded the instruction budget. At three million iterations
all three still failed. The distinct inline-block template with
the loop written directly inside it kept its fallback and budget
error at both sizes.

The additional 28 programs checked the two inherited differences.
For input 1 the wildcard arm produced 9. When the earlier guarded
body contained a print, execution printed only the wildcard's
string, including in a named template and beside a newly admitted
pure call. Forced legacy printed the wrongly paired body's line.
With an unrelated byte-string refusal the file still fell back and
showed the legacy output. The invalid assignment following an
unconditional continue remained unvisited on execution across
the same roots. The implementation adds no guard-based routing rule.

The public replay ran `rnx --color=never run --debug-engine` on
each adapted script with its recorded budget and each of
`--native=off` and `--native=on`. Among 3,202 inputs in each mode,
successful legacy-only entries fell from 76 to 73. The format
example l1-00291 and the shared source l1-00404/l1-00405 now run on
execution. Two test-only copies also changed route. l1-00046 has
the constant template and previously reported FrontendDiagnostic.
l1-00074 calls a local const fn and previously reported ConstCall.
Both still exit 1 with the identical missing-main diagnostic.
Every corpus input kept its exit, stdout, diagnostic and file
effects. All other routes stayed the same.

Restoring either old refusal alone moved its witness back to
legacy while the other witness stayed on execution in both native
modes. The whole native-profile Rune library binary passed 2,225
tests with 246 ignored. Its former named-string refusal test now
accepts the string constant, checks the unsigned length and retains
negative controls for a non-string template and dynamic precision.
All four Rune Clippy configurations, formatting, preflight and the
three public routing tests passed.

The edit to compile/const_eval.rs factors token indexing and HIR
lowering into a shared helper. The legacy caller still submits the
same expression, arena and source location to its existing VM
evaluator with an empty argument list. The new evaluator is selected
only during execution-core collection. The forced-legacy probes
kept their output, diagnostics and exit on the base and candidate.
All 15 accepted structural-exit examples from 0342 Control 4 also
ran through a body call, a direct template and a block template on
both engines in both native modes. All 180 invocations produced their
expected values. Examples include a return inside a tuple operand,
a break before an invalid assignment and a returning match guard.

Against published f562c486 the file gate passed 32 per-record and
32 cumulative rows. Native-on shared_rows setup T was 871,505
at both sizes on both images. All 64 rows for the empty project
and all 64 for the Polars/Candle project matched the base exactly.
The measurements used source snapshot b9facd26. This commit adds
the implementation Results to that snapshot.

FINAL-v1 on fe45b1b7 failed in the root session suite. The test
expected a refusal for the pure body call `f(2)`, which now yields
3. The successor checks that value and materializes a fresh vector
on each call. A runtime-local argument still exercises the constant
expression refusal. The byte-string refusal retains its own diagnostic.
The refusal audit found no other stale root or adapter expectation.

On the successor test tree the complete root test command from the
failed receipt passed 743 tests with 11 ignored, including 426 passed
and 10 ignored in the root library. All 32 async test-support tests,
all eight production CLI cases and all three Jupyter stage checks
passed. The only source edit since FINAL-v1 is in the session tests.
The corpus, Rune library and file and project measurements above
therefore apply to the same production source.

One additional tuple-pattern example shows an inherited difference.
For `const fn f(x){let (a,b)=(return 7,1);3}` called with a constant
argument, execution returns 7 in both native modes. Forced legacy
reports an unbound name in the pattern before execution.

The RESULT.json of the failed FINAL-v1 has sha256
ee53cbd4d7931c41060d5e9fe23e0d1f430d913bd4f70cadb1d6d172ed1231f4.
It passed 74 stages, failed root-tests and did not start 32, the gate
among them.

The second complete final ran on 9f96014a as FINAL-v2 and passed all 107
stages. RESULT.json has sha256
3cedc9d20f8b6bf5b2a5457ba048c71ce2909d5c1489c4c089cd800234b2c54f.
Entry through cleanup took 3,620.9 seconds against the 5,100 second
limit. The longest chain was the gate, which ran from 3,338 to 3,575
seconds. The root tests, which failed in FINAL-v1, ran from 1,764 to
1,949 seconds and passed. The Rune suite ended at 2,167 seconds with
955.2 seconds in the contracts. The gate reused the gate inputs
published with f562c486, so the base image was not rebuilt on the clock.
All 32 per-record rows passed. Twenty-five were exact. Seven native-on T
rows rose by 1,908 to 43,810 instructions at both sizes, at most 0.0614
percent: fib, loops, numeric_calls, awfy_sieve, awfy_permute,
vec_iter_i64 and vec_iter_bool. Each rise is in the setup phase and
within its allowance; execution and growth are unchanged in all seven.
All 32 cumulative rows passed. The fetch stayed offline and took 11.8
seconds. The build peaked at 120,448,159,744 bytes and shared memory at
5,668,864 bytes. The cleanup cleaned 37 targets and removed the run. The
memory controller reported no events. The run left no new entry in /tmp
or /var/tmp.
