# Implementation plan 0347: Host functions run by path and as values

Status: Proposed

Paired ADR: ../adr/0347_host_functions_run_by_path_and_as_values.md.

1. The boundary was reported and the design accepted before
   ADR 0346 was written. Rebase the work onto the published base.
   Report anything that has changed since.
2. Declare the two source transitions in the policy file, with
   the hashes of the signed candidate.
3. Path calls as Decision 1 states, with Controls 1 and 6.
4. Values as Decisions 2 to 8 state, with Controls 2 to 9.
5. Plant the refusal back. Replay the public admission corpus
   against the published base. List every changed source as
   Control 11 states. Send the list for review before signing if
   any source changes its exit, output or effects.
6. Score the file gate and both project screens.
7. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the whole rune-suite stage, the root-tests stage
   and the final-controls stage by their stage commands, as
   Control 14 states. Sign. Source review, one complete final,
   Results.

## Results

The compiler records the resolved host identity and receiver in the
program. A value made from that descriptor can be stored, selected
at runtime and called. The complete executor sends host values to the
same checked entry used by sliced execution. Script closures use their
existing frame setup. Controls exercised mixed vectors and objects,
aliases, shadowing, callbacks, moves, clones, repeated calls and heap
release. One stored `std::io::dbg` accepted zero, one and two arguments.

The registry audit covered 2,348 metadata rows. Of 1,856 named paths
and specializations, 406 ran as values. Stock could not resolve 46
abstract declarations as values. The compile-time refusals comprised
571 fused receiver or result shapes, two argument proofs
(`String::split` and `Vec::resize`), six inherited iterator gaps, one
future operation and 824 entries the new engine cannot call. The other
492 rows have no named path. Of the 762 named entries whose method
inventory admits at least one form, 379 ran as values, 375 depended on
fusion, two required an argument proof and six had an iterator gap.

The six gaps for 0348 are `Option::Iter::clone`, `Object::Iter::len`
and `len` on the i64 and u64 specializations of `RangeIter` and
`RangeInclusiveIter`. An escaped `chars().skip(n)` or `take(n)` can
also reach `InvalidCode` on the base. Those producers and terminals
keep their refusal in this record. In the paired adapter project a
`DataFrame::width` value still routed through `OpaqueIngress` and
printed 0 with native code on and off.

The CLI boundary replay compared 332 source programs in each native
mode with forced legacy. It normalized route markers and physical
addresses in debug displays. Wrong counts and receivers through a
function value preserved the stock diagnostic and source position.
The one existing difference was direct `std::io::println(true)`,
which reports `store: Type` on execution. The indirect call reports
the stock String-versus-bool error at its call site.

The public replay used `rnx --color=never run --debug-engine
--native=MODE --budget BUDGET SCRIPT` with the 0340 inventory and its
per-case budgets and clock properties. Of 3,202 sources in each mode,
successful legacy routes fell from 70 on e621f958 to 67. L1 00338,
00339 and 00340 moved from Native refusal to successful execution.
L1 00056 moved from Native refusal to execution and kept its missing-main
error. The historical `time::from_parts(#{})` case moved from Arity
refusal to execution and kept its one-argument error. These five sources
kept their exit status, output, diagnostic and file effects. Every other
source kept its route and outcome.

The expression_calls contract admits row 099 and cmp_ordering admits
205. S7's direct `sort_by` on unit compares its type error with stock.
The iter_adapters admissions are 543–546, 556–559, 569–572, 582–585,
595–598, 608–611, 621–624, 647–650, 652, 654, 668–670, 718–719, 737,
739, 840 and 1018. Of these, 41 compare exactly, including the callback
conversion in 840. Rows 718 and 719 reach ADR 0337's script-function
copy difference. Stock reports “Cannot take the value: it has been
moved” and “Cannot read the value: it has been moved”, respectively.
Execution returns `([2], [3])` and `([2], 2)`. The tests check both
outcomes. The session transcript compares the admitted map/collect
with stock. A refused async value checks fallback before mutation.

The other admissions are function_values 5–7, 37–51, 77–82 and 85,
option_methods 155–158, object_methods 174–178, and numeric_methods
272–273, 276 and 334–335. The 81 wrong_arity rows are 75–83, 85–92,
94–101, 103–110, 112–120, 122–129, 159–166, 168–175, 177–180,
182–189, 330–331 and 334. The initial pre-sign suite caught a host
arity diagnostic with the correct text but the wrong error kind.
The shared host check reports `ArgumentCount` with the given and
expected counts. iter_adapters 653 and iter_terminals 558 reach the
next unsupported operation. Their controls pin its reason and location.

The guard produced these SHA-256 transitions against e621f958.

| Executor method | Published source | Candidate source |
| --- | --- | --- |
| `fixed_intrinsic` | `64265c3f78592d39ac5503da5aab17d8d88fa0dc8fea42bb48ea710b17c777ec` | `ea656353f127000eff91976b84199a502804e4f0e2b57bf7c112cb4de6f8196f` |
| `call_closure` | `d69ef975d0029dbb5535604fc267ff0d2084262acb5a610f593f663b5e9d4c4f` | `37ee734a2d9e06720751c802d07c3bf40c3a10b42490de55c5f31cceecdcd14f` |

The file gate passed all 32 per-record and 32 cumulative rows. In
`shared_rows` native-on the outer total rose by 138 instructions at
both sizes. The loader accounted for all 138, with ten added relative
relocations. The earlier rule had stopped on ten against a limit of
four. Each project passed all 64 comparisons against its published
counterpart. In the file screen `numeric_calls` used 12 fewer W
instructions at size five and eight fewer in growth in each native mode.

A separate fixture made 1,000 and 5,000 calls to a closure that captured
a runtime value. The first candidate added 34 instructions per call.
Inlining the lookup and borrowing the host descriptor reduced that to
seven, still above the 0.5% band. Sharing the checked host entry brought
the delta to three. Across 4,000 extra calls, W increased by 12,000 on a
base of 4,876,000 (0.246%). The file, empty project and paired project
each passed level and growth for W and T with native code on and off.
No gate threshold changed.

Formatting, all four library Clippy configurations, preflight and the
planted-refusal control passed. The whole Rune stage passed its 12
recipes and 50 contracts. The root-tests and final-controls stage
commands also passed on this source.

Source review caught two enrollment expectations before the complete
final. On 95afda37 the whole cross stage passed the census and stopped
at fresh row 101. The engine produced a typed argument-count error at
the call, while the verifier expected a compile refusal and could not
project the runtime error. The successor compares rows 101
(`fs::read()`) and 102 (`path::separator(1)`) exactly with stock.
Both engines reported 0 arguments where 1 was expected at bytes
16..26, and 1 where 0 was expected at bytes 16..34, respectively.
The verifier takes the counts from the typed error and compares the
message, site, streams and file effects. The retired arity-refusal
policy had no remaining rows, so its branch and counter were removed.

Cross passed on the successor with 244 census admission checks, 109
exact enrollment rows, one retained resource difference and the full
embedding inventory. The whole Rune suite (12 recipes and 50
contracts), root-tests and final-controls passed again on that source.

The complete final ran on 57e8f2e5 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
1289717de9dc8f3edb3d81fbd1e02f8d3920b354aa00e9447ee4056f3f6f144b.
Entry through cleanup took 4,226.9 seconds against the 5,100 second
limit. The run entered with 193,788,096,512 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,636 to 4,187 seconds. The Rune suite ended at 2,484 seconds with
1,283.0 seconds in the contracts. The two declarations changed the
policy file, so no published gate inputs matched and the gate rebuilt
and measured the base on the clock. The anchor lookup missed and the
anchor build took 96.9 seconds. The source condition passed over 40
items and accepted the two declared transitions, fixed_intrinsic and
call_closure, with no other item changed. All 32 per-record rows passed
and none were exact. Five rows rose, each by less than 0.1 per mille
against the 5 per mille band: three awfy_sieve rows and the two
native-off awfy_permute rows. The other 27 fell. The largest falls were
in the two native-on awfy_permute rows: 1,256,852 instructions at size 1
and 1,256,832 at size 5 in W, and 1,491,813 and 1,493,081 in T. The
eight installed native-on T rows were scored under version 2, each with
ten added relative relocations and 138 loader instructions. In
shared_rows the loader was the whole rise in setup. All 32 cumulative
rows passed. The fetch stayed offline and took 11.8 seconds. The build
peaked at 121,016,852,480 bytes and shared memory at 5,668,864 bytes.
The cleanup cleaned 37 targets and removed the run. The memory
controller reported no events. The run left no new entry in /tmp or
/var/tmp.
