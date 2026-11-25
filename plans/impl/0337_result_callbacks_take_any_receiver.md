# Implementation plan 0337: Result callbacks take any receiver

Status: Proposed

Paired ADR: ../adr/0337_result_callbacks_take_any_receiver.md.

1. Boundary: the published base. Replay the 14 ownership
   observations and the shapes of Control 2 on both engines with
   fallback disabled. Record what refuses, where, and what the old
   core does. Record the image's relative relocation count.
2. Fix the gate rule first. Measure the unit with zero, one, four
   and five planted relocations at both sizes, and 12 instructions
   of fixed setup outside the loader. Name the linker output and the
   profiled function that carry the cost. ADR 0336's finding of 12
   instructions is a lead, not this measurement. Report the unit and
   the outcomes of Control 7 for review. Score no product code of
   this record before that review.
3. Admit the forms of Decision 1 with the order, checks and values
   of Decisions 2 and 3. Report before going on if a common receiver,
   such as a parameter or the result of a script call, can't be
   admitted under Decision 5 within what Decision 11 allows.
4. Add the parity table, the ownership outcomes, the dispatch
   controls and the refusal messages. Update the contract rows and
   the affected inventory witnesses.
5. Admit `is_readable` as Decision 6 states. Replay ADR 0336's six
   cases and the deferred `is_readable` case from full source in
   both native modes, and confirm the other 19 keep their boundary
   refusals.
6. Score the file gate and both project screens before signing. For
   a comparison that uses the alternate, report the raw increase, the
   loader's part and the remainder.
7. fmt, Clippy, preflight and the targeted checks in separate state.
   Sign. Source review, one complete final, Results.

## Results

The no-fallback replay compared exit status, stdout and stderr for
98 inputs with native code on and off. Of the 196 comparisons, 118
matched byte for byte, 36 exhibited the ownership differences in
Decision 4, and 42 retained a compile-time refusal on the new engine.
The latter cover two dispatch controls and 19 deferred corpus entries
in each mode. Corpus input 00152 (is_readable) and all six entries
00406 through 00411 completed on both engines.

The original 14 ownership inputs gave these outcomes in both modes.
An error in the table includes its source location.

| Input | Legacy | Execution |
|---|---|---|
| Err skips the callback body | Receiver and callback expression print in order, then Err(1) | Same |
| Err wraps a shared vector payload | Both wrappers print Err([1, 2]) after mutation | Same |
| Drop the result of map on Err | Original receiver remains readable | Same |
| Drop the receiver after and_then | Ok(2) | Same |
| Read the capturing callback alias after map | Ok(3), false | Ok(3), true |
| Read the callback alias after map on Err | Err(1), false | Err(1), true |
| Read the noncapturing alias after map on Err | Err(1), false | Err(1), true |
| Read the returned Result alias after and_then | Ok(7), false | Ok(7), true |
| Drop the receiver during map | Borrow error | Ok(1) |
| Drop the receiver during and_then | Borrow error | Ok(1) |
| Return the receiver from and_then | Borrow error | Ok(1) |
| Panic in the callback | Callback panic | Same |
| Return an integer from and_then | Expected Result, found i64 | Same |
| Supply an integer callback to Err | Expected Function, found i64 | Same |

| Additional dispatch or conversion input | Observed outcome |
|---|---|
| Named function, capturing closure in a local, copied function variable | Both engines return the expected Ok value |
| Same parameter site with Ok and Err, including a script-returned receiver | Both print Ok(2), then Err(9) |
| Zero-argument callback | Both report wrong arity on Ok and return Err(1) on Err |
| Reassign the receiver in the argument | Both return Ok(1) and leave the variable holding Err(2) |
| Drop the receiver in the argument | Both print the argument effect and report the moved receiver before calling the callback |
| Both operands invalid | Both report the receiver's read error at the method call |
| Return an existing Err | Both return Err(7) |
| Read a capturing callback's alias inside its body | Legacy returns Ok(false), execution Ok(true) |
| Drop a capturing closure through an alias before the call | Both report the moved function, including on Err |
| Drop a named function or noncapturing closure through an alias | Legacy reports the move, execution can still call the copy or return Err without calling it |
| One script-defined receiver type | Both call the script method |
| Option or a site shared by a script type and Result | Execution refuses map at the call, before output |

Each readability input queried the value and its alias, dropped the
value, then queried the value once and the alias twice. For 26 of
the 28 value forms the complete output matched. Named functions and
noncapturing closures were the two exceptions. Execution returned
true on all five queries. Legacy returned true twice and false three
times. Both engines produced the latter sequence for a capturing
closure. Repeated queries preserved the observed state.

The measured loader cost was 12 Ir per added relative relocation.
With the installed file route, shared_rows native-on setup grew from
871,577 to 871,601 instructions at both sizes. The installed image
grew from 31,573 to 31,575 relative relocations. The ordinary
per-record allowance was 11. The loader accounted for all 24 added
instructions, leaving zero at both sizes. This was the only use of
the alternate rule. All 32 per-record and 32 cumulative comparisons
passed. All 128 comparisons for the empty and adapter-pair projects
passed. AWFY Sieve grew by 10 instructions at size 5 and by eight
on the growth comparison in each mode and metric on both projects.
The other project comparisons were unchanged. Formatting and Rune
Clippy passed in the file screen.

A separate two-session probe used an enrolled Dense value. The legacy
session returned (true, true, false, false, false) across drop. The
execution session refused the first query before evaluation, at input
1, column 40. The opaque argument rule retains this boundary for the
adapter enrollment record. The refusal is covered by a session test.

The iterator adapter, wrong arity and numeric method contracts retain
their refusal classes and primary source locations. The 24 affected
rows now name the unproved map method. Five iterator adapter rows also
diagnose a later operation reached during deferred receiver analysis.
The registry checks exercise each Result method on a local and an
inline receiver, and check the registered global is_readable.

The first complete final on 1d7dead1 failed in the Rune library suite.
The intrinsic partition test still expected 67 entries after Readable,
CheckCallback and CheckReturn brought the certified table to 70.
That suite reported 2,188 passes, one failure and 245 ignored tests.
The same test assumed that PRINT_RAW and VARIANT_SUCCESS occupied the
first two slots. The successor checks all five certificates by ID and
arity and updates the expected count. The failed receipt's
SHA-256 is 2ffe0c5b3db29aee67e4d573504749de6c8a91e68d7f1143ac1dd0b0c716b07a.

Before signing the successor, the full Rune library suite with the
final's execution and native-code features passed 2,189 tests with
245 ignored. The final controller package passed 323 tests with two
ignored. Both suites used separate build state.

The complete final on e96542c1 passed all 107 stages. Its
RESULT.json SHA-256 is
079a7e001fd791233a3ab2b3338058136bcc48af3cdf842b5ab9d3f420101616.
Preparation restored the checkpoint after the failed final and
rebuilt it at the 509fb093 baseline in 3,942.2 seconds, outside the
clock. The timed final took 3,046.8 seconds of the 3,600, and the
total wait was 6,989.0 seconds. The Rune suite started at 80
seconds, released the debug target directory at 281 and ended at
1,722. It passed 2,189 library tests with 245 ignored. Its contract
batch took 736.8 seconds, and every contract passed. The fixed-route
gate started at 2,359 and ran for 641. The early disk check saw
254,282,862,592 bytes free against 237,278,648,994 needed. The check
before the clock saw 142,403,416,064 against 124,670,161,905. All 32
per-record comparisons passed. Twenty-three were exact. Five
native-on T rows moved by the same amount at both sizes, from -6,342
to -42 instructions, and AWFY Sieve native-on T moved by -6,424 and
-6,416. The other three AWFY Sieve rows grew by two instructions at
size 1 and 10 at size 5. All were inside their bands. The alternate
rule applied once, to shared_rows native-on setup T. Setup grew from
871,577 to 871,601 instructions at both sizes for two added relative
relocations. The loader accounted for all 24, leaving zero. That
row's whole-run counts were exact. All 32 cumulative comparisons
passed. There were no memory.high, memory.max or OOM events, and the
host was in the performance profile. The developer cache snapshots
matched before and after the run, and the final added no temporary
entries.
