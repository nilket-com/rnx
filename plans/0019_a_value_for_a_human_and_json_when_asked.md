# rnx 0019: a value for a human, and JSON when it is asked for

Status: proposed 2026-09-09. The nineteenth record of rnx. Record 0018 deferred
the renderer and left three questions in its place. They are three separate
contracts, and this record keeps them apart: what a **returned value** looks
like, what **JSON serialization** does with a value it cannot represent, and
what an **error** looks like. The first is decided by measurement, because the
reason 0018 gave for deferring it turned out not to be true.

## Context

`rnx run` prints a returned value as JSON. `rnx eval` and the prompt print it
with the bounded renderer the session uses. Record 0018 measured nineteen
shapes across the two and found nine identical, two differing in spacing, four
spelled differently, and four that `run` could not print at all.

Record 0018 deferred unifying them on this reason: "Changing what `run` prints
would change the output of every ported script, and all four ports are checked
byte for byte against their originals."

**That reason is false, and it was mine.** Measured, all four ports:

| Port | Prints its report with | Returns |
| --- | --- | --- |
| `classify_fold_perf.rn` | `println!` | `Ok(())` |
| `summarize_runtime_filter_perft.rn` | `println!` | `Ok(())`, or `host::exit(1)` |
| `check_blake3_confinement.rn` | `println!` | `Ok(())`, or `host::exit(status)` |
| `verify_nilket_graft.rn` | `println!` | `Ok(())` |

`Ok(())` unwraps to nothing and prints nothing, so **not one line of any
port's output comes from the returned-value renderer**. Nothing else in ket
reads a returned value either: the only two places that name `rnx run` are the
two READMEs that document these ports. The renderer could have been changed in
0018 at no cost to any port, and the record should have measured that before
claiming otherwise. This one measures first, including three things the probe
found that nobody had asked about.

**A cycle through JSON kills the process.** Measured, on the shipped binary:

```
$ rnx eval 'let v = [1]; v.push(v); host::json_stringify(v)'
thread 'main' has overflowed its stack
fatal runtime error: stack overflow, aborting        # status 134
```

`run`'s JSON path does the same for a returned cyclic value. So JSON's
traversal is not merely unbounded in theory; it aborts today, with no
diagnostic and a signal for a status. The readable renderer has carried a
cycle guard since record 0006 and prints `[1, <cycle>]`.

**The readable renderer's own ceiling is the renderer's, and it is
measurable.** With the limits lifted, a debug build renders a value nested
2048 deep and aborts by 3072, while Rune builds and drops the same value
nested 8192 deep without complaint. The recursion that fails is ours.

**A declaration's identity is available and is thrown away.** A file may hold
distinct structs of the same name in different scopes; Rune's runtime type
information carries `a::P` and `b::P`, and `format::type_name` keeps only the
last segment. A value of either renders `P {0: 2}` today — a name that does
not identify the type and positions that are not its fields.

**A deep value aborts in Rune's own cleanup, not in ours.** The abort at
100,000 is in the **drop**, not the construction: building the value and
leaving through `host::exit(0)` exits 0, while letting it fall out of scope
aborts. Rune builds and drops 49,152 deep and aborts by 65,536, in a debug
build. This bounds what rnx can promise, and gate 3 says so rather than
promising past it — see the checkpoint under that gate.

**Rune 0.14.1 supplies no identity that separates two compiled units.** Its
`Rtti` carries the field-name map that would settle decision 5 outright, but
`fields` is `pub(crate)` with no accessor; the public surface is `item()`,
`type_hash()` — which is the hash *of the item path*, so two units holding
`a::P` hash alike — and `type_info()`. What a value can do is answer
`s.get("x")` for a name and report how many fields it has, which is what
decision 5 is built on.

**A `Result` is not serializable and an option collapses.** Measured:
`host::json_stringify(Ok(1))` refuses with `cannot serialize external
references`, and `host::json_stringify(Some(Some(1)))` gives `1`, so JSON does
not round-trip an option. Both matter to decision 3, which must mirror the
serializer rather than guess at it.

Three more facts the probe established, each of which a decision below rests
on:

- Scripts already have an explicit JSON path: `host::json_parse` and
  `host::json_stringify`, which returns a `Result` and so fails honestly. But
  `host::json_stringify([1, #{a: || 1}])` says only `cannot serialize external
  references` — the type of the offending value and nothing about **where** in
  the value it was.
- A returned error that is a string prints **raw**. `rnx eval 'Err("\u{1b}[31m
  RED\u{1b}[0m")'` sends real ANSI to the terminal, because 0018 decision 2
  made a string error print bare and bare was implemented as unescaped. The
  value renderer escapes control characters inside a string; this path does
  not go through it. A diagnostic echoes its source line raw for the same
  reason, and places the caret by counting **characters**, so a line
  containing a tab or a wide character gets a caret in the wrong column.
- The prompt's limits elide. A returned vector of 4000 strings renders as 64
  of them, `…(+3936 more)`, and exits 0. That is a clearly marked preview and
  it is honest; it is simply not what the caller of a script wants, which is
  decision 1's product judgement rather than a defect.

## Decision

### 1. One renderer for a returned value, and it is the readable one

`run`, `eval`, and the prompt show a returned value with the same renderer:
the readable one the session already uses. `run` stops printing JSON. This
settles every difference record 0018 wrote down — the spacing, `None` against
`null`, `Some(1)` against `1`, a tuple, a character — by having one answer
instead of two, and it retires 0018's four "no rendering in `run` at all"
shapes, because the readable renderer has a form for each.

The **renderer** is shared. The **volume limits** are not, and the split is by
what the output is for, not by which entry point it is:

- `run` and `eval` are shell entry points. Their output is the product: a
  caller reads it, pipes it, compares it. They render **complete** — no
  length, string-byte, or total-byte cap — or they fail. This is a product
  decision, not a defect being fixed: a preview marked `…(+3936 more)` is
  honest about itself, and a script's returned value still should not arrive
  with 3,936 items missing.
- The prompt previews, with the limits it has today, because a person is
  reading it and record 0005's ceiling is what keeps a huge value from
  filling a terminal. Unchanged.

### 2. What "complete" means, decided before anything is printed

**Depth is bounded at 256, and the bound is a number because it was
measured.** The renderer recurses and dies between 2048 and 3072 in a debug
build; 256 leaves an eightfold margin under the depth that works, and is twice
serde_json's own default parse limit, so a value that reaches it is
pathological rather than merely large. A value deeper than 256 is **reported**,
not elided.

**The bound is a rule about every value, not about containers.** It is checked
on entry to each value as well as at each descent, because a step of recursion
need not add a container: a chain of `Some` adds none, and a bound that only
counted containers let 257 of them through and aborted on 8,192. The renderer
and the JSON walk check at the same threshold, so the two refuse the same
chains — asserted against each other rather than each against itself.

**A byte string is data, and shows its contents.** `b"abc"` and `b"xyz"` are
two different values; rendering both as `b"3 bytes"` reported their length and
called it a value. Each byte that reads as itself does, and every other is
`\xNN`, so the text is unambiguous whether the bytes are UTF-8 or not. This is
what separates data from the opaque markers above: a function has no contents
to show, and bytes have nothing else.

**A cycle and an opaque value are complete representations, not failures.**
`<cycle>`, `<function>`, and `<::std::ops::Range>` are what those values look
like; a value containing them is rendered whole and exits 0. Without saying so
explicitly, "rendered complete" and those markers contradict each other.

**A rendering that fails prints nothing of the value, and that is a claim
about rendering, not about the stream.** The text is built whole, in memory,
and written only once it is complete; on a rendering failure standard output
receives nothing of the value, the reason goes to standard error, and the
status is nonzero, as record 0018 decision 5 does it. This is why the order
matters: a script has usually printed its own report already and those bytes
cannot be recalled, so rnx must not append half a value to them.

rnx does **not** promise atomic standard output. A write that fails part-way —
a closed pipe, a full disk, a short write — can leave bytes behind, and no
buffering prevents that. What is promised is that a failed write is reported
with a nonzero status and never as success, which is what record 0013 decided
for a failed flush.


### 3. JSON is explicit, bounded, has one user-facing implementation, and says where it failed

JSON stays available and becomes something a script asks for, through
`host::json_stringify`. `run` no longer has its own copy, so one place decides
what JSON can represent and one vocabulary reports what it cannot.

**Bounding it is rnx's job, not serde's, and the walk mirrors the serializer
arm for arm.** The serializer is Rune's `Serialize` for `Value`, which
recurses without a cycle guard and aborts the process, as the Context
measured. So a value is walked by rnx first and refused before serde is ever
handed it. The walk is not a guess at what serde does; it is read off
`rune-0.14.1/src/runtime/value/serde.rs`:

| The serializer sees | It does | The walk must |
| --- | --- | --- |
| `Inline`: unit, bool, char, unsigned, signed, float | serializes | accept, no descent |
| `Inline`: empty, type, ordering, hash | refuses | refuse, with a path |
| `Dynamic`: any struct, tuple struct, or empty struct — every `RttiKind` | refuses, always | refuse, with a path; **never descend** |
| `Any`: `Option<Value>` | serializes the inside | descend into the inside |
| `Any`: `String` | serializes | accept, no descent |
| `Any`: `Bytes` | serializes as an array of numbers | accept, no descent |
| `Any`: `Vec`, `OwnedTuple` | serializes as a sequence | descend into every element |
| `Any`: `Object` | serializes as a map | descend into every value |
| `Any`: anything else, **a `Result` included** | refuses as "external references" | refuse, with a path |

Four shapes descend and the rest are leaves, which is what makes the walk the
oracle for the whole failure set and what lets a refusal carry a path.

**A cycle is a repeat on the active path, not a value seen twice.** The walk
tracks the path it is currently inside, pushing on descent and popping on
return, exactly as the renderer has since record 0006. `let a = [1]; [a, a]`
is one allocation referenced twice, is not a cycle, and serializes to
`[[1],[1]]`; `let v = [1]; v.push(v)` is a cycle and is refused. Both are
pinned, because a "seen" set would reject the first and a missing guard aborts
on the second.

**A value the walk cannot inspect is refused, not passed through.** The
serializer takes a borrow of every container it descends and turns a failed
borrow into an error; the walk takes the same borrows first and refuses on the
same failure, and refuses an unrecognised type hash rather than assuming serde
will cope. The rule is that nothing reaches serde that the walk has not
already accepted.

**One bound, one rule, no panic, no script code.** The walk uses the same
depth bound of 256 and the same cycle rule as the renderer, defined once and
shared by both, because two numbers that agree today are two numbers that will
not. A refusal neither panics nor invokes any Rune formatting protocol, which
is the rule `format.rs` has followed since it was written: inspecting a value
runs no script code.


**A refusal names where it failed, unambiguously.** Not `cannot serialize
external references` but the path to the value: keys as `.name` when the key
is a plain identifier, otherwise quoted and escaped as `["with.dot"]`, indices
as `[3]`, control characters in a key escaped the way decision 4 escapes them
everywhere else. Two values whose naive paths would read alike must produce
different paths — a key containing a dot, a bracket, or a quote is the case
that decides the syntax.

**One user-facing serializer, and that claim is scoped.** `host::json_stringify`
is the only JSON that rnx offers a script. It is not a claim about JSON inside
the crate: `host.rs` builds a process result with `serde_json::json!`, `run`
decodes its arguments through serde_json, and tests serialize freely. Those
are internal construction of values rnx itself shapes, they cannot receive an
arbitrary script value, and they are out of this decision's scope.

**No `--json` flag on `run`.** A script that wants JSON on standard output
prints `host::json_stringify(value)?`, which is explicit at the point of need
and already works. A flag would be a second way to say the same thing, and
nothing measured here asks for one.

### 4. One error display, and nothing rnx prints can move a cursor

An error carrying a value renders through one function shared by both entry
points. **Every byte rnx writes on its own behalf is escaped**: a rendered
value, a returned error including a bare string, a source excerpt, a
diagnostic message, and **the file path in it**, because a path is text from
outside too. A string error still prints without quotes and without a
wrapper, which is what 0018 decision 2 was about; escaping is what "bare" has
to mean, since a returned error is text a script chose and neither it nor a
file should be able to repaint the terminal of whoever ran it. Escaping is
visible, never dropped: a control character becomes an escape sequence in the
output, so no evidence is destroyed.

**An error stays bounded, in one function, and the budget is named.** A
returned value is the product and renders complete; an error is a report of a
failure, and a diagnostic carrying four thousand items reads worse than one
that says how many it left out. Record 0009 decided this and it is unchanged;
what is new is saying so, so that decision 1's completeness is not extended to
the error channel by anyone reading either decision alone.

One function decides it for both entry points, because two dispatches drifted
once already: a string error skipped the budget a rendered error obeyed, so
twenty thousand characters printed twenty thousand bytes through one shape and
four thousand through the other. The budget is the preview's string budget, so
a string error and a string inside a rendered error cannot diverge again, and
escaping **stops** at it — the work is bounded by what is printed, not by the
size of what was returned, which a length check alone would not catch.

**Columns are deterministic, and the two kinds are named separately.**

- The **reported column** stays what it is today: a position in the source,
  counted in characters, because that is what a person cross-referencing the
  file or an editor jumping to it needs. It is not a display column.
- The **caret** is computed from the escaped prefix actually printed — the
  same bytes the reader sees — measured in display columns.
- A tab needs no expansion policy, because escaping turns it into `\t`, which
  occupies two columns like any other escape.
- A wide character occupies the columns it occupies. This adds one
  dependency, `unicode-width`, rather than a hand-maintained table in this
  crate: East Asian Wide and Fullwidth count two, combining and zero-width
  count none. Owning a Unicode table is worse than owning a dependency that
  is only a Unicode table.

### 5. Field names are verified against the value, never assumed from a name

Rune 0.14.1 offers no way to ask a value for its field names and no identity
that separates two compiled units: `Rtti.fields` is `pub(crate)`, and
`type_hash()` is the hash of the item path, so a struct `a::P` in one unit and
another `a::P` in a different unit are indistinguishable through the public
surface. A full item path is therefore a **candidate key, not an identity**,
and this record does not pretend otherwise.

What a value will answer is `s.get("x")` for a name and how many fields it
holds. So names are **verified**:

- rnx keeps the field lists it parsed from declarations it compiled, as a set
  of **candidates per item path** rather than one entry, since two units may
  contribute the same path.
- A candidate is used only if **every** name in it resolves on this value and
  the count matches the value's own field count. Then the names are right by
  construction: they name this shape, whichever unit built it.
- If no candidate verifies, the fields render as **unknown** in a form that
  cannot be mistaken for a field name. Not positions, which is what
  `Problem {0: 7}` does today and is the defect.

This is record 0014's rule again: a match proves a **name**, never a fault.
It makes three cases correct rather than accidentally right — two structs of
the same name in different scopes of one file, each verifying its own
candidate; a value retained from an earlier session unit, which verifies
against the candidate its own declaration contributed and keeps its names
across later inputs; and a value whose declaration is genuinely gone, which
says unknown. `run` contributes candidates from the file it just compiled, and
the session's own site that renders without them (`session.rs:809`) is a third
place that should.

If a later Rune exposes the field map or a per-unit identity, this decision
becomes one line and the candidates go. That is a forward item, not a reason
to reach into private API now.

### 6. What this record does not decide

How the prompt bounds its own output, which is record 0005's and 0007's and is
unchanged. Whether `host::json_parse` should be strict about anything it
currently accepts. Any change to what a script prints itself, which is the
script's business and never rnx's.

## Acceptance gates

1. **One renderer.** The nineteen shapes of record 0018's gate produce
   identical standard output from `rnx eval EXPR` and `rnx run` on a script
   whose `main` is `EXPR`, and identical standard error, and the same exit
   code. The difference table in 0018 decision 4 becomes one row: identical.
2. **Complete output from both shell entry points.** A returned vector of 4000
   items prints 4000 items from `rnx run` and from `rnx eval`, with no elision
   marker anywhere in either. The prompt still previews the same value, which
   is asserted in the same test so the split is visible in one place.
3. **The depth bound is a boundary, and cleanup is part of it.** A value
   nested 256 deep renders from both shell entry points and exits 0. One
   nested 257 deep is reported: nothing of the value on standard output, a
   reason on standard error naming the depth, and a nonzero status. At 32,768
   deep — inside Rune's own measured capacity, with margin — the refusal is
   **catchable**: a script catches it, does further work afterwards, and the
   process exits normally with the value dropped, asserted on the process's
   status rather than on its output.

   **Checkpoint, not a weakened test.** The 100,000-deep case in this gate's
   first draft cannot be met by rnx and is not a gate here. Measured: Rune
   builds a value 100,000 deep and aborts **while dropping it**, and its own
   build-and-drop ceiling is between 49,152 and 65,536 in a debug build. The
   value exists before rnx is asked to print it, so refusing to print does not
   avoid the drop, and no rnx-side check can. A script can abort the process
   this way without rnx rendering anything at all. That is an upstream defect
   to record against Rune, in the shape record 0006 uses, and the honest
   guarantee here is the one above: for any value Rune can itself drop, rnx
   refuses cleanly and the process exits with a status.

4. **The bound covers a path with no container on it.** A chain of 256 options
   serializes and renders; 257 is refused by both, with a status and not a
   signal; 8,192 is refused, caught by the script, and the process still exits
   normally. A mixed chain of alternating containers and options is refused
   too. The renderer and the walk are asserted to agree at each depth, so one
   constant cannot become two.
5. **A byte string shows its contents.** Two three-byte strings that differ
   render differently; bytes that are not UTF-8 and the quote and backslash
   render as escapes; nine thousand bytes render whole at a shell entry point
   and preview at a prompt with the number left out.
6. **An error costs the same whatever its shape.** A twenty-thousand character
   string error and the same text inside an object are both bounded by the
   named budget, read identically from both entry points, and a string of
   nothing but escapes is bounded by the **work** the budget allows rather
   than by its character count. A string error still prints without quotes.
7. **A cycle and an opaque value are successes.** A self-referential value
   renders with `<cycle>` and exits 0; a closure and a range render with their
   markers and exit 0; from both shell entry points.
8. **A failed rendering leaves standard output alone.** A script that prints
   two lines of its own and then returns a value that cannot be rendered
   produces exactly those two lines on standard output, the reason on standard
   error, and a nonzero status. A write that fails part-way is a
   different thing and is asserted separately: the status is nonzero, and no
   claim is made that standard output was left untouched.
9. **JSON is bounded, mirrors the serializer, and no longer aborts.**
   `host::json_stringify` of a cyclic value and of a value nested 257 deep
   each return an error the script can catch, and the process exits with a
   status rather than a signal — asserted as a status, and failing on the
   current binary with a signal, which is what makes it a control. At 32,768
   deep the error is catchable and the script continues, on the same terms as
   gate 3. Repeated shared data is **not** a cycle: `let a = [1]; [a, a]`
   serializes to `[[1],[1]]` and exits 0, so a "seen" set cannot pass this
   gate. Every arm of decision 3's table is exercised: each refusing shape is
   refused, each descending shape descends, and a `Result` is refused, which
   pins the walk to the serializer rather than to an assumption about it.
10. **A refusal names the path.** `host::json_stringify` of a closure eight
   levels inside an object names the keys and indices that lead to it. Keys
   containing a dot, a bracket, a quote, and a control character produce paths
   that differ from the paths of keys that merely look like them, asserted as
   a pair that would collide under a naive syntax.

11. **One user-facing serializer.** No JSON writer other than
   `host::json_stringify` can be reached from a script value: `run`'s copy is
   gone, and a test asserts a returned value takes the renderer's path. The
   crate's internal uses named in decision 3 are exempt by name.
12. **Nothing rnx prints can move a cursor.** For a returned value containing a
   control character, a returned string error containing one, a source excerpt
   containing one, a file path containing one, and a diagnostic message
   containing one, the bytes rnx writes contain no ESC and no bare carriage
   return. One test covers all five surfaces so a new surface cannot be added
   without meeting the rule. A string error still prints without quotes.
13. **The caret lands on the token.** For a source line containing a tab and
    one containing a wide character, the caret's column equals the display
    width of the escaped prefix printed above it, asserted by computing that
    width rather than by eye. The reported numeric column is asserted to be
    the source position in the same test, so the two kinds cannot be conflated.
14. **Fields are verified or declared unknown, for a struct and for a
    struct variant.** A returned error carrying a
    struct with named fields names them from `run` as from `eval`, replacing
    0018's case that asserts `Problem {0: 7}`. A file declaring two structs
    named `P` with different fields in different scopes renders each with its
    own fields. A value **retained from an earlier session input** still
    renders with its own field names after later inputs have been evaluated,
    which is the case a per-path table without verification gets wrong. A
    value whose declaration cannot be matched renders a form that contains no
    field name and is asserted not to look like one. An enum's struct variant is named
    from its declaration too, which is the same mechanism and was the same
    defect: `C {0: 4}` was a position dressed as a field name.

15. **The ports are unaffected, and that is measured, not argued.** All four
    ports match their originals byte for byte, and a test asserts that a
    script returning `Ok(())` prints nothing, which is why they are unaffected.
16. **Nothing regresses.** The gates of records 0002 through 0018 pass, except
    the two 0018 gates this record replaces rather than deletes: gate 6's
    difference table becomes gate 1 above, and gate 7's "cannot show" shapes
    become renderable.

## Guardrails and stop conditions

1. Three contracts, three places: one renderer for a value, one user-facing
   serializer for JSON, one display for an error. A second implementation of
   any of them is the defect this record exists to remove.
2. The bound is defined once. The renderer's walk and JSON's pre-walk are two
   walks, and they must share one depth constant and one cycle rule; two
   numbers that agree today are two numbers that will not.
3. No change to what a script prints itself. Every port's output comes from
   its own `println!`, and this record must not touch a byte of it.
4. If any consumer of `run`'s JSON output is found after all — inside ket or
   outside it — the default flip stops and is recorded, rather than being
   softened into a flag nobody asked for.
5. Escaping is not eliding. A control character is rendered visibly, never
   dropped, because a dropped byte is evidence destroyed.

## Risks

- **A value that was JSON is now readable text.** Anyone piping `rnx run` into
  `jq` breaks. Measured: nobody does, inside ket. Guardrail 4 is what happens
  if that turns out to be wrong outside it, and `host::json_stringify` is the
  one-line repair for any script that needs the old shape.
- **Complete output can be asked for a huge value.** A script returning a
  gigabyte prints a gigabyte, where the prompt would have previewed it. That
  is the contract every other command-line tool offers, and gate 5 is what
  keeps a failure to produce it from arriving as a partial line.
- **A deep value can still abort the process, and not through rnx.** Rune
  aborts dropping a value nested past roughly 50,000, whether or not anything
  renders it. Gate 3 states the boundary rnx can hold and the Forward raises
  the rest upstream; what this record must not do is imply a guarantee that
  stops at rnx's own walk.
- **A fifth dependency.** `unicode-width` is small and is only a Unicode
  table, which is the argument for it: the alternative is a table in this
  crate that nobody will update. If it is refused, the caret rule stands and
  the width function becomes ours, with the table's provenance recorded.
- **Escaping changes text a script chose.** A script that deliberately emitted
  colour through a returned error loses it. It can still print colour itself;
  what it loses is the ability to do so through rnx's error channel, which is
  the point.

## Forward

Two things to raise upstream with Rune, in the shape record 0006 uses: that
dropping a deeply nested value aborts the process — a script can do it with no
help from rnx — and that `Rtti`'s field map has no public accessor, which is
the only reason decision 5 verifies candidates instead of asking the value.

Then a second script that reads bytes, and a child that stays open — for which
record 0017's four thousand invocations are a shape, not yet a requirement:
what to measure first is the verifier's actual bottleneck, and what a
persistent child would have to guarantee about standard input delivery,
backpressure, cancellation, and cleanup before any interface is committed to.

