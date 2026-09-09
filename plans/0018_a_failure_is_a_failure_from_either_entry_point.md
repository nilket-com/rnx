# rnx 0018: a failure is a failure from either entry point

Status: proposed 2026-09-09. The eighteenth record of rnx. `rnx run` treats a
returned error as a failure. `rnx eval` prints it and exits 0. This record
makes them agree, and says what it deliberately leaves different.

## Context

Record 0009 decided what `rnx run` does with what a script returns: a value is
shown and the status is 0, an error is reported on standard error and the
status is 1. Record 0013 gave both entry points an exit status a script can
choose, on the rule that `eval` runs one thing and exits, which is what a
status is for.

`eval` never got the first half. Measured:

```
$ rnx eval 'host::read("no-such-file")'
Err("cannot read no-such-file: No such file or directory (os error 2)")
$ echo $?
0
```

The read failed, the error went to standard output, and a shell checking the
status saw success. Through `run` the same expression exits 1 with `error: `
and the message on standard error.

This is the shape of defect the last several reviews have found: an outcome
that is not success reported as success. It is worse here than in a script,
because `rnx eval` is the form a shell reaches for.

Comparing the two entry points shape by shape found the same defect a second
time, in `run` rather than `eval`, which decision 5 fixes.

## Decision

### 1. `eval` reads a returned value the way `run` does

A value may arrive on its own or wrapped in a result. `Ok(v)` is `v` and the
status is 0. `Err(e)` is a failure: `error: ` and `e` on standard error, and
the status is 1. Anything that is not a result is itself, and the status is 0.
A value that is nothing at all prints nothing, where `eval` used to print
`()`.

Both entry points ask one function what a returned value means and one
function whether it is nothing, so neither answer can drift.

### 2. An error that is a string prints as it was written

`run` already does this, so `Err("plain words")` gives `error: plain words`
rather than `error: "plain words"`. `eval` now does the same, because the two
are the same sentence to whoever reads it.

### 3. A child that failed is still a success

`host::process` returning `Ok` with a nonzero `code` means the call worked and
the child did not. That is not a failure of the expression, and it keeps
exiting 0. The distinction is the reason decision 1 is about the shape of the
returned value rather than about anything inside it.

### 4. What is left different, and why

`run` renders a value with JSON and `eval` renders it with the bounded
renderer the session uses, and this record does not unify them. Changing what
`run` prints would change the output of every ported script, and all four
ports are checked byte for byte against their originals. That is a rendering
question with its own evidence to gather, and it is not this one.

The first draft of this record claimed the two now differ only in spacing on
standard output and agree on standard error. Both claims were too strong, and
review found them so. They should have been measured before they were written.
What the nineteen shapes of gate 6 actually do:

| Standard output | Shapes |
| --- | --- |
| Identical | nine: four returned errors (nothing on standard output from either), `Ok(5)`, `1 + 1`, `"hi"`, nothing at all, and a child's exit code |
| Spacing only | two: `[1, 2, 3]` against `[1,2,3]`, `{"a": 1}` against `{"a":1}` |
| Spelled differently | four: `None` against `null`, `Some(1)` against `1`, `(1, 2)` against `[1,2]`, `'x'` against `"x"` |
| No rendering in `run` at all | four: a struct, an enum variant, a closure, a range — decision 5 |

JSON has no form for an option, a tuple, or a character, so the third row is
not spacing and no amount of formatting reconciles it. The gate names each
pair, because "they differ" is not a contract and these are.

Standard error agrees for every returned error the gate covers, and one case
outside it differs: an error carrying a **struct** prints `Problem {code: 7}`
from `eval` and `Problem {0: 7}` from `run`. Both entry points render an error
through the same bounded renderer, so this is not two renderers disagreeing —
it is one renderer that takes field names from the session's table of retained
declarations, which `run` has no equivalent of, and which falls back to
positions without it. Giving `run` field names means parsing its file's
declarations, which is the renderer question above. The difference is asserted
so it cannot drift unnoticed.

So this record claims agreement on the **status** for every shape both entry
points can show, agreement on **standard error** for the errors the gate
covers, and a **named, tested difference** everywhere else.

### 5. A value `run` cannot show is not a success

Comparing the shapes for decision 4 found that a script returning a struct, an
enum variant, a closure, a function, or a range printed

```
<serialization error: cannot serialize struct main::$0::P>
```

on **standard output** and exited **0**. A shell saw success, and a reader saw
something that reads like a value where no value was shown. That is the defect
in the Context, in the other entry point, and this record is named for both.

A rendering that fails is now reported like any other failure: `error: the
value the script returned cannot be shown: ` and the reason, on standard
error, and the status is 1. One function turns a returned value into the text
`run` shows and it is allowed to fail, rather than returning a message dressed
as a value.

This does not decide **how** those shapes ought to render, which is decision
4's deferred question. It decides only that failing to render one is not
success. `eval` shows all five, because the bounded renderer has a form for
each, so this is the one place a status deliberately differs between the two.

### 6. What this record does not decide

The renderer, as above. Anything about the session, which prints values at a
prompt because that is what a prompt is for and which has no status to give.

## Acceptance gates

1. **A returned error fails.** `rnx eval` on an expression that evaluates to
   an error exits 1, puts `error: ` and the message on standard error, and
   puts nothing on standard output.
2. **A string error prints bare**, with no quotes and no wrapper.
3. **A returned `Ok` is unwrapped.** `Ok(5)` prints `5`, and a successful
   `host::read` prints the file rather than a result around it.
4. **A value that is not a result is unchanged**, and still exits 0. A value
   that is nothing at all prints nothing, from either entry point.
5. **A child that failed is still a success.** `host::process` returning `Ok`
   with a nonzero code exits 0, and the code is in the value.
6. **The two entry points agree, and where they do not the difference is
   written down.** For nineteen expressions — not every shape a returned value
   can have, but every shape this cut measured — `rnx eval EXPR` and `rnx run`
   on a script whose `main` is `EXPR` are compared. The fifteen both can show
   agree on the exit code and on standard error. Standard output is asserted
   per case against the table in decision 4: identical for nine, equal after
   spaces are removed for two, and equal to a written-out pair of texts for
   four. A returned error carrying a struct has its own case, asserting the
   two texts that differ and that both still exit 1.
7. **A value `run` cannot show is not a success.** For each of the five shapes
   with no JSON form, `run` exits 1, prints nothing on standard output, and
   reports the reason on standard error. `<serialization error:` appears
   nowhere. A value that does have a JSON form still prints it and exits 0.
8. **Nothing else about `eval` changes.** Its compile and runtime diagnostics
   are what records 0002 and 0009 gave it, and `host::exit` from `eval` still
   chooses the status.
9. **Nothing regresses.** The gates of records 0002 through 0017 pass, and all
   four ports still match their originals.

## Guardrails and stop conditions

1. One function decides what a returned value means. A second place that
   decides it is the defect this record exists to remove.
2. No change to how `run` renders a value it can render. A rendering that
   fails is a diagnostic and not a rendering, which is why decision 5 is not a
   breach of this. If the renderers should be unified, that is its own record
   with its own evidence.
3. If treating a returned error as a failure breaks a use of `rnx eval` that
   matters, that is recorded rather than softened.

## Risks

- **Someone used `rnx eval` to look at a result value.** They now see the
  inside of it, or a failure. The prompt is where looking at values belongs,
  and gate 6 is what makes the new behaviour predictable rather than
  surprising.
- **A script that returned a struct, an enum variant, a closure, or a range
  exited 0 and now exits 1.** Its output was `<serialization error: ...>`, so
  nothing that read the value can have been working; a caller that only
  checked the status will now see the failure it was already having. No port
  returns such a value, and gate 9 is what says so.

## Forward

The renderer, which decision 4 defers and decision 5 sharpens: it now owns
three named questions — how `run` should render the four shapes it has no form
for, whether the two should agree on an option, a tuple, and a character, and
whether an error carrying a struct should be able to name its fields from a
file the way it does from a session. Then a second script that reads bytes,
and a child that stays open, which record 0017 measured.
