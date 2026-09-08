# rnx 0014: naming the missing method

Status: proposed 2026-09-08. The fourteenth record of rnx. The second port
called two methods Rune does not have, and the diagnostic named neither. This
record decides how the name is recovered, and it corrects the premise record
0011 wrote it down under.

## Context

The second port asked for `join` on a sequence. What came back was:

```
runtime error at ..., line 73, column 2: Missing instance function
`0xf77d93259f11131a` for `::std::vec::Vec`
  	parts.join(UNIT)
```

Record 0009's work put the file, the line, the column, and the source line in
front of a person, and that made it a ten-second fix. Record 0011 asked for
the name as well, and called it "a small fix to a message that already knows
the answer".

**That premise was wrong, and grounding it is most of this record.** The
message does not know the answer. Rune's error carries the hash and the
instance type and nothing else, and `VmErrorKind` is crate-private in 0.14.1,
so rnx cannot read even those two structurally. The name is not in the error
at any level of access.

Two things are true instead, and both were measured:

- **The hash is reproducible.** `Hash::associated_function(type_hash, name)`
  is public. For the real type hash of `Vec` and the name `join` it produces
  `0xf77d93259f11131a`, which is exactly what the diagnostic reported. So a
  candidate name can be *proved* right rather than guessed.
- **The position points at the receiver, not the method.** The span for
  `parts.join(UNIT)` starts at `parts`. On a chain like
  `counts.values().sum()` the span still starts at `counts`, while the method
  that is missing is `sum`. Reading the first method name after the span
  would name the wrong one, confidently.

So the name is recoverable, but by reconstruction and proof, not by reading it
out of something that already has it.

## Decision

### 1. A name is printed only when its hash matches

Candidates come from the script's own source: every identifier that appears in
method position, which is a small set the script itself wrote. Each is hashed
against the instance type, and a name is printed only if its hash equals the
one the error carries.

The candidates are read from Rune's own tokens, not from the text. Whitespace
and a comment between the dot and the name are then ignored the way the
compiler ignores them, an identifier inside a string literal is not a
candidate at all, and an identifier beyond ASCII is one. A textual scan gets
the common case right and quietly misses `parts . /* why */ join(x)`, which is
a valid call.

This is what makes the chained case correct rather than plausible. A match is
proof of the pair, so `counts.values().sum()` names `sum` and not `values`.

Nothing is guessed. There is no "did you mean", no nearest match, and no
partial answer.

### 2. The type hashes are derived at run time, not written down

rnx makes one value of each type a script commonly calls methods on, and reads
the type hash and the displayed name off the values themselves. The map is
whatever the linked Rune says it is, so it cannot drift from the version in
use the way a table of constants would.

### 3. When the name cannot be recovered, the message is unchanged

A type rnx has no value for, a method called through something the source does
not name, an upstream message that no longer parses: each falls back to
exactly what is printed today. The hash is a poor answer and it is still the
true one, so the fallback loses nothing.

### 4. The whole message must be this diagnostic, beginning and end

The hash and the type come from Rune's rendered text, because the structured
form is crate-private. The message must be exactly that diagnostic: it starts
with the phrase and ends with the closing backtick, and anything before or
after means the fault is something else.

Recognising the pattern anywhere inside a message is not enough, and the hash
does not save it. A script that calls `parts.join(x)` and also panics with the
text of a missing-method error has a candidate whose hash matches perfectly;
the fault is still a panic. The hash proves which method a name refers to. It
proves nothing about what went wrong.

rnx pins `=0.14.1`. A test asserts the shape rnx parses is the shape Rune
produces, so a version bump that changes the wording fails that test rather
than silently falling back for ever.

### 5. Only this one diagnostic

`Missing instance function` is the one this port hit twice. No other message
is reworded, and record 0009's format is unchanged: the same place, the same
source line, the same caret, with a better sentence.

## Acceptance gates

1. **A single call is named.** `parts.join("-")` reports a missing `join` on
   the sequence type, and the hash no longer appears.
2. **A chain names the right method.** `counts.values().sum()` reports `sum`,
   not `values`, and the test fails if the first name in the line is taken.
3. **A type outside the map falls back**, printing exactly today's message,
   hash and all.
4. **A name the source does not contain falls back** the same way.
5. **A fault that is not a missing method is left alone.** A script that
   panics with the text of one, while also containing a call whose hash
   matches, still reports a panic. The same script without the panic reports
   the missing method, so the case discriminates rather than merely finding
   nothing.
6. **Whitespace and comments between the dot and the name are ignored**, a
   name inside a string literal is not a candidate, and an identifier beyond
   ASCII is one.
7. **The map is derived.** A test reads the map out of values and asserts that
   a known pair reproduces the hash the diagnostic reports, so a Rune that
   hashes differently fails here rather than silently naming nothing.
8. **The upstream shape is pinned.** A test asserts Rune still renders
   `Missing instance function \`0x...\` for \`...\``, and fails if it does not.
9. **The place is unchanged.** The file, line, column, source line, and caret
   are exactly what record 0009 gave; only the sentence changes.
10. **Nothing regresses.** The gates of records 0002 through 0013 pass.

## Guardrails and stop conditions

1. A name is printed only on a hash match. If that ever becomes impossible,
   the answer is the current message, not a guess.
2. No new dependency and no fork of Rune. If naming the method needed either,
   this record would stop and say so.
3. The map is derived from values. A hardcoded hash is a stop condition, not a
   shortcut.

## Risks

- **The parse breaks on a Rune upgrade.** Gate 6 turns that into a failing
  test. The fallback means a missed parse costs the sentence, not the
  diagnostic.
- **A hash match proves a name, not a fault.** Two things have to be true
  before a name is printed: the message is this diagnostic and nothing else,
  and the candidate's hash matches. Decision 4 is the first of those, and it
  is not optional; the hash cannot stand in for it.

## Forward

Record 0011's remaining two suggestions: `join` and `contains`, which want a
third port to confirm them, and a grouped float, which is a formatting
question rather than a helper. The third port itself is the larger next thing.
