# ADR 0337: Result callbacks take any receiver

Status: Proposed

Base: published 509fb093b102d039208a7a1b2956f74729bcabfc (ADR 0336).

## Problem

ADR 0336 admitted `Result::map` and `Result::and_then` only where the
receiver is an `Ok` or `Err` built in the call and the callback is a
closure written there. The ordinary ways to write these calls still
refuse at compile time: a receiver held in a variable or returned
from a call, a callback passed by name, and an `and_then` callback
that returns a `Result` it didn't build. Such programs run today
only through the legacy route.

The old core holds a shared borrow of the receiver while the callback
runs, takes the callback by value, and takes the `Result` that an
`and_then` callback returns. The new engine has no borrow that lasts
across a script call and doesn't consume a value by calling it. The
14 ownership observations taken for ADR 0336 show where a program
can see this. In three, the old core stops with a borrow error
because the receiver is still borrowed while the callback runs or
returns. In four, the program asks whether an alias of the callback
or of the returned `Result` is readable, and the old core says it
isn't. The other seven don't depend on it.

Five of the 14 ask that question with the global `is_readable`,
which the new engine refuses today. It is one of the 20 small-form
cases ADR 0336 left for later. Without it those programs can't
complete on the new engine, and the second difference of Decision 4
has no witness.

The second problem is the file gate. Its level allowance for the
installed native-on setup count on `shared_rows` is 11 instructions.
The loader spends 12 instructions on each load-time relative
relocation. So any change that adds one relocated pointer to the
image fails the gate: a panic site, a table entry, a specialized
routine. That cost is paid once at load and doesn't grow with work.
ADR 0336 twice shaped its code around this limit. The gate should
stop a real setup cost and let a counted loader constant through.

## Decision

1. Admit `Result::map` and `Result::and_then` for a receiver in any
   expression and a callback in any expression: a closure, a named
   function or a function value held in a variable, with or without
   captures. An `and_then` callback may return any `Result`.
2. Order and checks follow the old core. The receiver expression
   is evaluated and its value kept. The callback expression is
   evaluated next, once, whether the receiver is `Ok` or `Err`.
   Then, before the variant is examined, the kept receiver must
   still be readable and the callback must be a readable function.
   If both fail, the receiver's error is reported. The callback is
   called only for `Ok`, and its parameter count is checked only
   then. The value an `and_then` callback returns must be a
   `Result`, of either variant. Each failure is the old core's
   error, with its message and position, and a callback's own
   failure passes through unchanged. Return to review if an exact
   message needs more than Decision 11 allows.
3. The call works on the value kept in Decision 2. Assigning to the
   receiver's variable in the callback expression doesn't change
   it. Once the checks pass, the variant and the payload the call
   uses are fixed. That fixes which values are used. It copies
   nothing: the payload is the same value, and for `Err` the result
   carries it in a wrapper of its own. Dropping or replacing the
   receiver leaves the result usable, and the reverse.
4. Two differences from the old core are new and intentional. Each
   begins when the checks of Decision 2 pass, so it holds inside the
   callback as well as after the call:
   - the receiver isn't borrowed. A callback may write to or take
     the receiver, and an `and_then` callback may return the
     receiver itself, where the old core stops with a borrow error;
   - the callback and the `Result` an `and_then` callback returns
     aren't taken. An alias of either stays readable, where the old
     core reports it taken, the callback's from before its body
     starts.
   A third difference is already in the contract and reaches these
   calls. A named function and a closure that captures nothing are
   copies on the new engine, so dropping or moving one copy leaves
   the others readable. The old core then reports the others moved:
   in the callback check of Decision 2, in a call, and through
   `is_readable`. The new engine doesn't, and this record adds no
   state to make it. A closure that captures a value still moves as
   on the old core.
   Each difference is recorded in the contract with witnesses and
   the outcome on each route. A program that completes on the old
   core without seeing any of them keeps the same results, output,
   errors and effects.
5. Admission stays by identity. A call runs the `Result` path only
   on a `Result`. Where the receiver can be another type with a
   method of the same name, the call behaves as it does today for
   that type, or keeps a compile-time refusal. That holds for a call
   site that sees a `Result` on one path and another type on
   another. A receiver that comes from a script call isn't thereby
   proved a `Result`. No call fails at run time where the old core
   would have succeeded, and none fails after an effect the old
   core would not have run. A refusal names `Result::map` or
   `Result::and_then` only where the receiver is proved a `Result`;
   otherwise it names the method as written.
6. Admit the global `is_readable` by its registered identity. It
   reports whether its argument can be read now and doesn't take or
   change it. Its answers are the old core's wherever the two
   engines agree on the value's state; where they differ, the
   difference is one of Decision 4. It is never a constant. A
   value owned by a native adapter keeps its compile-time refusal
   here; that question belongs with the adapters' enrollment.
7. The gate accounts for added relative relocations separately, up
   to four per record. The ordinary rule is scored first. Only where
   the installed native-on per-record setup level fails, at size 1
   or 5, does the alternate apply. It passes when all of these hold:
   - the candidate image has one to four more ordinary relative
     relocations than the base;
   - the increase observed inside the loader's relocation processing
     is the same at both sizes and equals that count times the unit;
   - the raw setup increase is at most the larger of the existing
     allowance and four units, at both sizes;
   - the raw increase less the loader's part meets the existing
     allowance at each size.
   The evidence is bound to the runner, the installed helper and the
   engine library measured on each side, the relocation counts read
   from those same images, the loader object with the profiled
   symbol and window that carry the cost, the capture configuration,
   and the row, mode and size. Missing or inconsistent evidence
   can't earn the alternate pass. The report shows the raw increase,
   the loader's part and the remainder. Every other axis is scored
   as before.
8. The unit is measured on the published base, not assumed. If it
   isn't 12 instructions at both sizes, report before fixing it.
   The unit belongs to the loader it was measured on. With another
   loader the alternate doesn't apply until it is measured again.
9. Nothing else in the gate changes: the cumulative comparison
   against its fixed anchor, growth, the W counts, native-off and
   execution scoring. Added relocations still spend the cumulative
   headroom. The rule carries a version, and receipts from earlier
   records are scored by the rule they were taken under.
10. The rule, its unit and its controls are fixed and reviewed
    before any product code of this record is scored.
11. Scope: the two methods, `is_readable` and the gate rule. No
    change to the legacy engine. The checks of Decision 2 on the
    callback and on the value `and_then` receives may be private
    checks on a cold path, reached through the existing intrinsic
    call route. The bytecode format, argument passing and the host
    ABI don't change, and the hot dispatch path gains nothing.
    Return to review for a new opcode or anything beyond that.

## Controls

1. The 14 ownership observations, replayed on both engines with
   fallback disabled. Of the eight that complete on the old core,
   four agree byte for byte and four differ only as Decision 4's
   second point says. Of the six that fail, the callback failure and
   both type errors agree, and the three borrow errors complete with
   the outcome recorded. Each outcome is measured, never inferred
   from a refusal.
2. A parity table for the new forms:
   - a named function item, a named local that holds a capturing
     closure, and a function value in a variable;
   - a receiver whose variant is chosen at run time at one call
     site;
   - a function of the wrong parameter count: the old core's error
     for `Ok`, completion for `Err`;
   - a callback that isn't a function, for `Ok` and for `Err`, and
     the same with a receiver that was moved, which reports the
     receiver;
   - the receiver's variable reassigned in the callback expression:
     the call uses the first value and the variable keeps the
     second;
   - an alias of the receiver dropped in the callback expression:
     the expression's output appears, then the receiver's error,
     and the callback doesn't run;
   - an `and_then` callback that returns a value that isn't a
     `Result`, one that returns an existing `Result` of each
     variant, and one that returns the receiver;
   - an alias of the callback read inside its body, with the outcome
     on each route;
   - a callback that replaces the receiver, and dropping either of
     the receiver and the result while using the other;
   - a callback dropped through another name before the call: a
     named function and a closure that captures nothing complete on
     the new engine, with the old core's error recorded, and a
     capturing closure reports the moved value on both.
3. Dispatch: an `Option` receiver, a script-defined type with
   methods of these names, and a call site that sees a `Result` and
   another type behave as they do on the published base or refuse
   before any effect. The missing, overridden and foreign-installed
   cases of each method keep their refusals. Each refusal message
   names what Decision 5 says.
4. ADR 0336's six full-source cases and its parity table are
   unchanged. A form refused before counts as admitted only when its
   full source completes on the new route with fallback disabled.
   Report restored corpus entries separately from new forms.
5. The contract rows that record these forms as refused are replaced
   by measured outcomes. Both intentional differences are listed
   with witnesses. Unaffected dispositions stay.
6. `is_readable`: a parity table against the old core for a
   readable value, a moved value and a dropped alias, for each kind
   of script value the engine tells apart, with the argument
   unchanged afterward. A native adapter's value refuses at compile
   time. A copy of a function that captures nothing reads as
   readable after another copy is dropped, recorded under the third
   difference. The missing, overridden and foreign-installed cases
   keep their refusals. The deferred `is_readable` case counts as
   restored only when its full source completes with fallback
   disabled, and is reported apart from the new forms. The other 19
   small-form cases keep their boundary refusals.
7. The gate rule, on the published base with planted images, on
   the `shared_rows` row whose allowance is 11 instructions: one and
   four added relocations pass; five fail; 12 instructions of fixed
   setup outside the loader fail; one relocation together with 12
   unrelated instructions fails. Two increments that each pass fail
   the cumulative comparison when together they exceed a planted
   cumulative limit. Evidence taken from a different image is
   rejected, and so is evidence missing one size. A receipt from an
   earlier record, with no evidence, is scored by its old rule.
8. The file gate and both project screens, image to image against
   the published base, under the new rule.
9. Finals: targeted pre-sign checks in separate state, with every
   measured delta checked against its band, then one complete final
   on the signed candidate.

## Constraints

- No new third-party crate. Tabs.
- Programs admitted today keep the same results and effects.
- No refusal at run time.
