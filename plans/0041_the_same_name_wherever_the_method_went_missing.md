# rnx 0041: the same name, wherever the method went missing

Status: implemented 2026-09-14; Linux gates pass. Windows execution remains
unverified. Evidence is beside this record. The forty-first record of rnx, and a
consistency fix: record 0014 taught `rnx run` to say `no method `join` on
`::std::vec::Vec`` where Rune says `Missing instance function
`0xf77d…``, and it never taught the session or `eval`. This record
applies the same naming, by the same proof, in the two places it is
missing, including a call site that lives in an input from before a
`:renumber`.

## Context

Measured on 2026-09-14, on the same binary, the same mistake in three
places:

```
$ rnx run f.rn            # f.rn calls parts.join("-")
runtime error at f.rn, line 3, column 8: no method `join` on `::std::vec::Vec`

$ rnx eval '1.missing()'
runtime error at input 1, line 1, column 1: Missing instance function
`0xff0afefdd65e03a7` for `::std::i64`

[4] rnx> let old = |v| v.missing_method();
[5] rnx> :renumber
[2] rnx> old(1)
runtime error at input 4 of numbering 1, line 1, column 15: Missing
instance function `0xd568eb7c24a7c487` for `::std::i64`
```

The first names the method. The other two hand a person a hash. Record
0014's `method::named` is called from one place, the file runner, and
from nowhere else — found while reviewing record 0040's specimen, which
shows the third case in colour. "Missing method `missing`" tells a
person what to fix; a hash does not, and the session is where people
make the most of these mistakes, because that is where they type.

What record 0014 decided still holds and is not reopened here: a name
is printed only when its hash, computed against the instance type,
equals the hash in the message, so nothing is guessed; the candidates
are the identifiers in method position in the script's own source, read
from Rune's tokens; the type hashes are read off live values, not
written down; when the name cannot be proved the message is left exactly
as Rune produced it; the pattern must be the whole message; and only
this one diagnostic is reworded. This record changes where that is
applied, not what it does.

## Decision

### 1. The session and `eval` name the method the way `run` does

The session builds `Failure::Runtime` in several places, but every VM
error passes through one conversion point, `runtime_failure`, which
takes the error and resolves its origin from the retained source maps.
That is where the naming goes: the message passes through
`method::named` there, before anything is appended to it, and `eval` gets it for free because `eval` is a
session with one input. `run` keeps its own call, unchanged. The three
entry points then say the same sentence for the same mistake, which is
record 0009's standard for diagnostics restated for this one.

Order matters and is stated: the session appends "; the budget of N
instructions was exhausted at that point" to a runtime failure when that
is so, and record 0014's pattern requires the whole message to be the
diagnostic. Naming happens first, on Rune's message alone, and the
budget clause is appended to the named result, so a budget-exhausted
missing-method error reads `no method `x` on `…`; the budget of …`.

### 2. The candidates come from the input that contains the call site

In a file there is one source. In a session there are many, and the
call that failed may be in a different input from the one that ran it:
a closure defined at input 4 and called at input 9 fails at input 4,
where `v.missing_method()` was written; input 9 says only `old(1)`, and
its identifiers prove nothing. So the candidates are read from **the
source of the origin's input**, the one the retained maps resolved the
error to — record 0040 made that index stable and kept it apart from
the display number, and this record is the first to depend on that. A
`:renumber` between definition and call changes what the diagnostic
calls the input and nothing about which source it reads.

When the error has no origin — the maps cannot place it — there is no
source to prove a name against, and record 0014's decision 3 applies:
the message is left as Rune produced it. The current input's text is
not used as a fallback, because a name proved against the wrong source
is still a guess about which call failed, and the record's rule is that
nothing is guessed.

### 3. Nothing else changes

The known-type table, the candidate scan, the pattern match and the
sentence are record 0014's, untouched, so `run`'s gates hold unchanged
and a type outside the table still gets Rune's message in every entry
point. Record 0040's numbering, record 0039's colour and record 0009's
placement are unchanged: the same place, the same excerpt, the same
caret, a better sentence.

### 4. What this record does not decide

- Extending the known-type table — `Option`, `Result`, `HashMap`, host
  types — which would name more methods everywhere at once and is its
  own small record with its own gate per type.
- A "did you mean", which record 0014 refused and this record does not
  reopen.
- Naming anything but this one diagnostic.

## Acceptance gates

1. **The same sentence in three places.** `1.missing()` through `run`,
   `eval` and a piped session each produce `no method `missing` on
   `::std::i64`` with no `0x` and no `Missing instance function` in the
   output; the place, the excerpt and the caret are exactly what they
   were.
2. **A retained call site, across a renumber.** In a session: define a
   closure at input 4 calling a method that does not exist, `:renumber`,
   call it at the new input 2; the diagnostic reads `runtime error at
   input 4 of numbering 1, …: no method `missing_method` on `::std::i64``.
   The same with two renumbers between, and with the closure defined in
   numbering 1, redefined under the same name in numbering 2 with a
   different missing method, and the second one called: the name is the
   second method's, because the source read is the origin's.
3. **A chain names the one that failed.** `"abc".to_uppercase().frobnicate()`
   in a session names `frobnicate` and does not mention `to_uppercase`,
   as `run` already does.
4. **Not proved, not named.** An object's `values().frobnicate()` — a
   type outside the table — keeps Rune's message in the session as it
   does under `run`; a session input that panics with the literal text
   of a missing-method message keeps that text. The no-origin case is
   made **discriminating**, so that it proves there is no fallback to
   the current input rather than merely that nothing crashed: retain a
   closure that calls a missing method, remove its source-map entry the
   way the existing defensive test does, then call it from an input
   that itself contains a matching candidate — `old(1); 1.missing_method()`
   in the calling text — and assert the original hashed message
   survives. A fallback to the current input would have named it.
5. **The budget clause follows the name.** `--budget` belongs to `run`,
   which this record does not touch, so the session path is exercised
   directly: a unit test sets the session's budget through
   `Session::set_budget` low enough that a missing-method error
   coincides with exhaustion, and asserts the message is the named
   sentence followed by the exhaustion clause, in that order.
6. **Existing guarantees.** Both suites pass; `run`'s naming gates are
   untouched; the startup measurements and session baseline before and
   after.

## Guardrails and stop conditions

1. One naming function, called from the runner and from the session's
   shared VM-error conversion point, `runtime_failure`, and from nowhere
   else.
2. The source proved against is the origin's input, selected by its
   stable index; never the current input, never a display number.
3. If the session's failure constructor cannot reach the origin's source
   text without a second copy of it, stop; the retained sources are
   already kept for the excerpt.

## Risks

- **A session input's source is the user's text, not the generated
  wrapper.** Candidates read from the user's text see exactly the calls
  the user wrote, which is the right set; the wrapper adds no method
  calls. Stated so nobody widens the scan to the generated source.
- **The known-type table is short.** Ten types; a method missing on
  anything else still shows a hash, in every entry point alike. That is
  the deferred record in decision 4, not this one's defect.

## Forward

The known-type table, when the specimen of someone's real session shows
which types people actually mistype methods on.
