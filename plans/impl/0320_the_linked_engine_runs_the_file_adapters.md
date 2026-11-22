# Implementation plan 0320: The linked engine runs the file adapters

Status: Implemented

Paired ADR: ../adr/0320_the_linked_engine_runs_the_file_adapters.md.

1. Boundary: Codex's 4/6 proposal, reconciled against published d7c04262.
   Record any deviation before editing.
2. Freeze the checked-file installer inventory, and map each shape to C
   fields and ownership. Do this before any production edit.
3. Add the versioned registration surface, the single adapter-side
   owner, and the descriptors with pre-effect rejection.
4. Add the isolation, parity and cap/cleanup controls. Freeze the launch
   configurations, then capture and attribute both images.
5. fmt, Clippy and preflight. Sign. Then run `final.sh --required` on the
   signed commit. Source review, one complete final, Results.

## Results

The checked stock runner and the linked file runner agreed on all 35
inventory cases. Thirty entered checked execution and five declined
before effects. The complete model script first declined with
`OpaqueReceiver`. A 2,147,484,175-byte CSV returned width 2 through
both runners. Invalid schema and missing-model errors preserved the
stock diagnostic. A matching fixture verified all 64 forward scores.

Both provider builds used engine image
`1dd23d985c5e237b9d1c8b79041d97910a70ff28eaac486e49333e22ed5f1244`.
Changing the provider changed the model's reported output width from 1
to 2. The extended provider's twentieth unary method copied all input
bits. The ordinary provider refused that call. The finite-store controls
covered aliases, a foreign program handle, a wrong returned kind, peak
credit overflow, store admission failure and release after failure.

The first signed required final on 50365e6e stopped at stage selection.
It treated the provider directory as a separate Cargo package. The
provider example now lives in its owning Candle demo package.

The equality control compares all 5,386 named function registrations
in the three stock adapter namespaces with the table submitted to OPEN.
The linker fixture generates the inactive declarations from the stock
installer. An extra stock registration and a missing provider row both
failed the same comparison. The twentieth unary appeared only in the
extended provider. Compile refusal controls matched the stock source ID,
revision and byte range, including the absent location on DataFrame::empty.
They abort on any provider invocation. The demo reached the stock
OpaqueReceiver refusal at the same source location.

The early screen, first integrated screen and rebuilt sibling screen
each passed all 32 fixed-route comparisons. The rebuilt stock-to-image
diagnostic exceeded the ordinary band in eight native-on comparisons:
fib W and T, loops W, numeric_calls W, awfy_sieve W, awfy_permute T,
vec_iter_i64 W and vec_iter_bool W. Six had exceeded it on the first
integrated build. The cutover record owns that comparison.

The two new diagnostic failures were fib W and T. On c8dd44b5 the size-5
stock-to-image instruction deltas were:

| Metric | Setup | Execution | Total |
| --- | ---: | ---: | ---: |
| W | +582 | +389,949 | +390,531 |
| T | -29,017 | +389,910 | +360,893 |

The scalar fixture never opens the adapter registry. Separately, in 12
paired measurements, opening a checked file context with 5,386 names
instead of 38 cost a median additional 6.68 ms.

The recursive specialization of `Executor::invoke_args` accounts for
389,779 extra execution instructions at size 5. Its own count rose
from 25,689,887 on stock to 26,079,666 in the image. Decoding the
retained first and rebuilt image profiles shows the executor pointer
spilled to the stack. One field comparison acquired a reload on each
of its 70,869 visits. Across the loop the rebuilt image executed
372,043 additional stack loads and 17,729 additional stack stores.

REQUIRED-v2 on c8dd44b5 failed when the service's SSH agent refused
to sign fixture commits. REQUIRED-v3 on the same commit passed those
controls and failed the ordinary stock gate. Stock fib execution
increased by 389,913 instructions on W and T, against allowances of
370,237 and 370,267. The recursive loop's exclusive counts matched
the diagnostic on both sides. Setup, execution growth and the
cumulative comparisons passed.

Removing only the unused 13-line `Module::inactive_file_symbol`
definition reversed the stock spill. Reverting the inventory enumeration
or the CLI changes separately did not. Stock now excludes the image's
inactive declarations, owned association names, terminal tuple/list
outputs and checked path constructor. The stock installer inspection is
available only to the inventory control. Stock and image still share
the renderer and the registration helpers both actually use.

At size 5 all four diagnostic stock builds pushed 35,430 script call
frames. The published loop executed 25,689,887 instructions; c8dd44b5
executed 26,079,666. Both the one-hunk ablation and the feature-gated
stock build recovered the published count exactly (725.09 instructions
per pushed frame, down from 736.09). Size 1 also recovered exactly.
Both rebuilt provider variants passed the 32 sibling comparisons. The
image still compiles the registration and still shows the fib spill
against stock. Stabilizing its dispatcher remains work before cutover.

All eight compile refusals recorded zero provider payload allocations
and zero bytes charged to native host entries. The admitted CSV width
control allocated one payload and charged 2,256 bytes. These observations
used the frozen plain provider archive with a debug build of the same
engine source. The debugger recorded each admitted slot's actual charge
through cleanup. The release launcher also matched the stock refusal
diagnostics with the unexpected-invocation trap armed.

A required final on d6a3c04a passed all 82 selected stages in 42.3
minutes. The complete final on the same commit passed all 104 stages,
with 38.5 minutes of accepted run and 39.2 minutes end to end. Both
fixed-route bands held, there were no memory.high, memory.max or OOM
events, and the host was in the performance profile.
