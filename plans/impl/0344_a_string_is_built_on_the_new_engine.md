# Implementation plan 0344: A string is built on the new engine

Status: Proposed

Paired ADR: ../adr/0344_a_string_is_built_on_the_new_engine.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. List every
   constructor and in-place method of a string with its route
   today. Find how `push` is admitted and whether the same path
   serves the others. Measure the old core's outcome for the forms
   of Controls 1 to 6. Report before going on if a method needs
   more than Decision 7 allows.
2. Constructors and methods as Decisions 1 to 3 state, with
   Controls 1 to 3, 5 and 7.
3. Capacity and the store's growth as Decision 4 states, with
   Controls 4 and 6.
4. Plant each admitted name out, one at a time. Replay the public
   admission corpus against the published base. List every changed
   source as Control 9 states. Send the list for review before
   signing if anything but the expected cases changes.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight, the whole Rune library test binary and
   the targeted checks in separate state. Sign. Source review, one
   complete final, Results.

## Results

The installed file runner admits both String constructors and all six
builder methods. A literal evaluated twice gets fresh storage. A write
through a local alias, field, vector element, capture, argument or
returned string updates the original. A clone, conversion, sum,
formatted string or template has its own storage. Changing the string
used to insert an object key preserves the key.

A string reserved at 42 bytes still reports 42 after three bytes of
text, and 3 after shrinking. Starting from zero, pushing a character
then eight more bytes yields capacities of 8 and 16. Clearing
preserves capacity. The store checks the allocator's growth before
reserving and returns the released charge on shrink. Its S5 growth
witness now runs a million pushes, observes 33,554,432 bytes and
releases the last store entry and byte.

Self-append by either spelling, including through an alias, succeeds
on execution. Legacy refuses the aliased read. A live chars iterator
still blocks writes with the original diagnostic at the call and
allows a capacity read. A two-byte formatted string and template
start at capacity 2 on execution and 8 on legacy. An enrolled host
returning two bytes from a 64-byte buffer imports them with capacity
2. Program-directed growth uses the same allocator policy for all
of these origins.

For an unproved receiver the compiler uses the existing method lookup
only when the String operation is its sole reachable provider.
Missing-method outcomes are preserved. Unproved clear and capacity
keep the Vec collision. In the default context unproved reserve
also stays refused because VecDeque registers it. A known String
can use all three directly.

The boundary replay used 96 programs on automatic and forced legacy
routes with native code off and on. Eight independent provider
omissions each broke only the omitted name's witness. The public
CLI corpus ran 3,202 cases in each native mode on each revision.
The successful legacy-only count fell from 73 to 70 in each mode.
L1 00133, 02887 and script 00133 now run on execution. L1 00177,
02906 and script 00177 still run on legacy, with the refusal moving
from Native at the constructor to Method at into_bytes. Exit status,
stdout, diagnostics and filesystem effects matched for every row.

The first file screen failed the native shared_rows setup check by
adding 60 instructions. Five extra relative relocations in the
launcher accounted for all 60. Four new source locations came from
argument indexing and two from HIR reads, against one removed
location. Passing the fixed arguments directly and using checked
HIR reads removed that cost. The repaired file gate passed all 32
per-record and 32 cumulative rows under the existing bands.
The empty project and the Polars/Candle project each passed all 64
comparisons against their corresponding published-base image. The
largest relative increase on either project was below 0.012%.
The measurements used snapshot 3c0f9af6 on signed plan 0e882212.
The subsequent Rust edit adds a host-string test.

The whole Rune library passed with 2,231 tests and 246 ignored.
All four Rune Clippy feature configurations, workspace formatting,
preflight, and the 15 installed file-routing tests passed.
The final's root-tests command also passed in its own worktree with
project-sources enabled.

FINAL-v1 on 9baf34f1 failed in the embedding inventory because it
expected Method refusals for both receiver spellings of all six
admitted String methods (receipt SHA-256
628671e60c2b9c43398d80ae18cb6eac4cb8e2e80d40417f46706b0bc2c324ec).
The repaired inventory checks both admissions for each method, both
qualified constructors and the three ambiguous receiver refusals.
All six Rune library configurations passed before signing the successor.

FINAL-v2 on 28502f7f passed the library configurations and failed in
five contract families whose ten rows still expected Method refusals
(receipt SHA-256
7b653c804a36fcfaa4f0f21efc2e23b772dc29a89df9ab060fd3f7412420878f).
All ten ran on the new engine and matched live stock on outcomes,
reuse, events, logical traces and diagnostics. Their contract rules
now require exact agreement with stock.
Before signing, the complete rune-suite stage passed all 12 library
and documentation recipes and all 50 contract families in 1,119.1
seconds. The earlier root-tests pass covers the same production code.

The third complete final ran on 4c3930bf as FINAL-v3 and passed all 107
stages. RESULT.json has sha256
2a0b001e32c779a82cbaf99dbe9df2d309c8af8b009b396cf1c3cfd485778f34.
Entry through cleanup took 3,690.8 seconds against the 5,100 second
limit. The longest chain was the gate, which ran from 3,408 to 3,646
seconds. The Rune suite, which failed in FINAL-v1 and FINAL-v2, ended at
2,311 seconds with 1,087.4 seconds in the contracts and passed. The gate
reused the gate inputs published with 2ab8ccd7, so the base image was
not rebuilt on the clock. All 32 per-record rows passed. Eleven were
exact. Seven native-on T rows fell by 219 to 25,782 instructions at size
1, at most 0.0396 percent: fib, loops, numeric_calls, awfy_sieve,
awfy_permute, vec_iter_i64 and vec_iter_bool. Each fall is in the setup
phase. These are the seven rows that rose in the 0343 final. Sixteen
rows rose in execution, each within its allowance and at most 0.0109
percent. The rise at size 5 is five times the rise at size 1 in every
one: shared_rows by 3 and 15 instructions in all four rows, awfy_sieve
by 17 and 85 with native code off and by 7 and 35 with it on,
awfy_permute by 4 and 20 in all four rows, and vec_iter_i64 and
vec_iter_bool by 15 and 75 with native code off only. All 32 cumulative
rows passed. The fetch stayed offline and took 11.6 seconds. The build
peaked at 120,501,760,000 bytes and shared memory at 5,668,864 bytes.
The cleanup cleaned 37 targets and removed the run. The memory
controller reported no events. The run left no new entry in /tmp or
/var/tmp.
