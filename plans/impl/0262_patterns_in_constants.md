# Implementation plan 0262: patterns in constants

Status: Proposed

Paired ADR: ../adr/0262_patterns_in_constants.md.

1. Pin 294997ae. Read the bounded evaluator's let, if, block and call
   paths and its frame, the pattern representation it receives, how the
   run-time lowering matches the same patterns, and the stack tests of
   ADR 0259.
2. Before editing, send a note: each form of the ADR at run time in both
   engines and as a legacy constant, with any disagreement; every
   pattern kind the language accepts and which are proposed; how one
   matcher serves the four forms; what is moved and what is copied when
   a part is bound, with the charge; frame size of the matcher per level
   in both profiles and the effect on the limits; anything in the ADR
   the code makes wrong. If an inventoried item would change, send that
   instead and stop.
3. Implement the matcher and its four callers.
4. Add the controls of the ADR and regenerate the pinned table.
5. Run Miri, a final in required mode, preflight and the commit checker.
   Bring the packet with the class of each new row, the totals before
   and after, the stack table, and the per-stage durations. The
   candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Source-review results

The four constant forms share an iterative pattern walker. Bindings are
committed only after the complete match and all required copies succeed.
Owned root bindings move; borrowed roots and child bindings use the existing
cumulative data charge. Pattern nodes, scanned object fields and compared
text bytes consume work. Existing limits are unchanged.

Focused core controls pass in engine-dev and engine-release (17 tests each),
including the 2 MiB stack controls and direct expression/combined-depth
boundaries. The largest observed stack-marker distance from thread entry is
1,383,768 bytes in engine-dev and 550,768 bytes in engine-release; both leave
more than 512 KiB. These are measured marker distances, not a proof covering
all possible compiler frames. Pattern traversal itself adds no recursive
native frame per nested pattern. Measured bind/walk prologue frames are
1,176/3,688 bytes in engine-dev and 248/296 bytes in engine-release, excluding
return addresses and caller arguments.

Five constant-session controls and the regenerated parity-pin control pass.
Ten new constant rows measure Equal; total Equal rises from 184 to 194.
Existing rows and other classifications are unchanged. Cross-source parameter
refusals preserve ExecutionRefusal. Unit on unmatched match and if-let without
else follows the observed run-time and legacy-constant behavior.

Core library clippy passes. An ad-hoc root clippy invocation with blanket
-D warnings reports 15 existing diagnostics outside the changed files; this
is not the repository's baseline-comparison gate. The ordinary final will
run that gate. Earlier bin-target session/pin invocations selected zero tests
and are not acceptance evidence; the passing controls above use the library.

Existing constant text binary equality is refused by binary_finish, rather
than performing an uncharged text comparison. This record does not change it.
The recursive work calibration adds one pattern-node charge per parameter
match (32,767 additional units in the existing calibration).

Miri and the ordinary final, preflight and commit checker remain pending.

### Measured stack and frames

Prologue bytes exclude return addresses and caller arguments.

| Matcher function | engine-dev | engine-release |
| --- | ---: | ---: |
| bind | 1176 | 248 |
| walk | 3688 | 296 |
| kind_matches | 328 | 168 |
| literal_matches | 424 | 136 |
| text_eq | 376 | 88 |

Observed marker distances from thread entry, in bytes:

| Control | engine-dev | engine-release |
| --- | ---: | ---: |
| pending_add | 1138856 | 438144 |
| tail | 973416 | 389504 |
| pending_add_limit | 1185192 | 457856 |
| call_limit | 652312 | 201728 |
| object_limit | 1383768 | 539328 |
| vector_limit | 1303688 | 550768 |
| failure_unwind | 1139416 | 438400 |
| pattern_nested | 216760 | 54464 |
| pattern_calls | 1143152 | 439808 |
| pattern_failure_calls | 1142912 | 438816 |

Source nesting 90 reaches the existing HIR depth refusal before evaluator
entry. Direct HIR controls therefore cover the matcher depth boundaries.
Source review accepted without production changes before signing.

The first Miri invocation timed out after 3,600 seconds in the combined
25-program semantic test, without reporting undefined behavior. The same
25 cases and assertions are now separate exact tests with durable receipts
and bounded parallel admission; all 25 pass natively after the split.
Production code is unchanged by this test/inventory-only repair.
