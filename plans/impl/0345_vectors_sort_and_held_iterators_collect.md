# Implementation plan 0345: Vectors sort and held iterators collect

Status: Proposed

Paired ADR: ../adr/0345_vectors_sort_and_held_iterators_collect.md.

1. Boundary: the published base. Reproduce the observations of the
   ADR's Problem on both engines in both native modes. Find why a
   held iterator runs `next` and refuses the rest, and why `sort_by`
   runs and `sort` doesn't. List the methods refused on a held
   iterator. Measure the old core's outcome for the forms of
   Controls 1 to 5. Report before going on if a part needs more
   than Decision 8 allows. That part then leaves the record as
   Decision 6 states.
2. `sort` as Decisions 1 and 2 state, with Controls 1 and 2.
3. Held iterators as Decisions 3 to 5 state, with Controls 3 to 6.
4. Plant each refusal back, one at a time. Replay the public
   admission corpus against the published base. List every changed
   source as Control 8 states. Send the list for review before
   signing if any source changes its exit, output or effects.
5. Score the file gate and both project screens.
6. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the whole rune-suite stage and the root-tests stage
   by their stage commands, as Control 11 states. Sign. Source
   review, one complete final, Results.

## Results

On published 5aeda4b2, the automatic and forced-legacy file routes
were measured on 117 programs with native code off and on. The same
468 observations were taken on the candidate image. Thirty-one
programs moved from fallback to execution. Three kept fallback and
changed its reported reason. In both modes every change preserved the
exit, output, diagnostic and source position, and the forced-legacy
observations were identical.

Default vector sort uses the stable merge shared with sort_by. Its
comparator applies the CMP protocol at each element, including the
leaves of nested sequences. The receiver can be an argument, a field
or a second name for the vector. Controls cover stable equal floats,
mixed numeric types, strings, a NaN, moved elements, an active
reader, and a failed comparison with a retained reference to each
element. A replaced sort handler or an uncertified comparison handler
on a reachable type prevents admission.

A held iterator uses the cursor already in its slot. Aliases share
progress, and collecting after next starts at the remaining elements.
Consuming the cursor to exhaustion keeps its borrow of the vector
until the last iterator reference is released. A repeated object key
keeps its last value. Controls for failed collection, early exit,
and dropping an unused cursor release the store entries and bytes.
The forwarding rule uses existing positive evidence for the receiver.
The measured fields, elements, captures and parameters lack that proof
and keep their Method refusal. An unrelated script method keeps its
own dispatch.

On the measured slice cursor, next and any already ran. Collect into
Vec, Object, Tuple or String, count, numeric sum and product, find,
all, fold and reduce run when their inputs meet the existing checks.
Skip, take, rev, map, filter, filter_map, flat_map, enumerate, chain,
peekable, nth, next_back and size_hint kept Method refusals, as did
collect into VecDeque. Position, last and len kept their located
missing-method errors. Host functions called by path or held as values
remain a separate record.

The public corpus replay covered 3,202 cases in each native mode.
There were 70 successful legacy routes on each side in each mode.
Only the refusal label changed in L1 00056, 00338, 00339 and 00340,
from Method to Native. The first lacks main and the other three
succeed. Their output, diagnostics, exit and filesystem effects agree
with the base.

Eight existing contract cases now compare admitted results with live
stock. They are cmp_ordering 184, type_values 40, object_keys 11, and
iter_terminals 465, 466, 467, 474 and 477. The embedding inventory
checks default sort and the supported consumers on materialized
iterators. The pre-sign rune-suite stage passed its twelve recipes and
fifty contract families. The file performance gate passed all 32
per-record and all 32 cumulative rows. The empty and paired-adapter
project screens passed all 128 rows against their matching published
images.

Planting back only the sort refusal broke its ordering witness and
left the held-cursor witnesses passing. Planting back only the held
receiver refusal broke the consumption and capacity witnesses and
left sort passing. The first pre-sign library and suite attempts
exposed outdated expectations for held count, default sort and the
private iterator intrinsic range. The corrected library passed 2,238
tests with 246 ignored. The root-tests stage, all four Rune Clippy
configurations, workspace formatting and preflight passed.

The complete final ran on e1eb8cd6 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
0e2fccd2646d373763fb90406c0da4e38204d5790cf37458aed06f46a7486638.
Entry through cleanup took 3,836.7 seconds against the 5,100 second
limit. The run entered with 159,019,958,272 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,559 to 3,796 seconds. The Rune suite ended at 2,106 seconds with 877.3
seconds in the contracts. The gate reused the gate inputs published with
5aeda4b2, so the base image was not rebuilt on the clock. All 32
per-record rows passed. Twenty-two were exact. Seven native-on T rows
rose by 5,482 to 70,182 instructions at size 1, at most 0.1004 percent:
fib, loops, numeric_calls, awfy_sieve, awfy_permute, vec_iter_i64 and
vec_iter_bool. Each rise is in the setup phase and within its allowance.
These are the seven rows that fell in the 0344 final. The four
awfy_sieve rows rose in execution by 14 instructions at size 1 and 70 at
size 5, at most 0.0001 percent. All 32 cumulative rows passed. The fetch
stayed offline and took 11.7 seconds. The build peaked at
120,489,672,704 bytes and shared memory at 5,668,864 bytes. The cleanup
cleaned 37 targets and removed the run. The memory controller reported
no events. The run left no new entry in /tmp or /var/tmp.
