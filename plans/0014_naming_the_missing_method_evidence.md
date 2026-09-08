# rnx 0014 evidence: naming the missing method

Implementation of `plans/0014_naming_the_missing_method.md`, 2026-09-08, on
Linux x86_64 against upstream Rune 0.14.1 with no patch. Status of the record
stays proposed until reviewed.

## What landed

- `src/method.rs`: the type map, the candidate scan, and the rewrite, with
  four unit tests.
- `src/runner.rs`: one line, where the message is taken from the error.
- `src/main.rs`: the module.
- `tests/run_diagnostics.rs`: three gates.

## Before and after

The second port's own case:

```
runtime error at m1.rn, line 3, column 15: Missing instance function
`0xf77d93259f11131a` for `::std::vec::Vec`
  	let joined = parts.join("-");
                ^
```

becomes

```
runtime error at m1.rn, line 3, column 15: no method `join` on
`::std::vec::Vec`
  	let joined = parts.join("-");
                ^
```

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 a single call is named | pass: `join` and `contains`, the two the second port hit, are named, and no `0x` reaches the reader | `a_missing_method_is_named_rather_than_hashed` |
| 2 a chain names the right method | pass: `"abc".to_uppercase().frobnicate()` names `frobnicate`; the test also asserts `to_uppercase` is not named, which is what a source heuristic would say | `a_chain_names_the_method_that_failed_not_the_first_one` |
| 3 a type outside the map falls back | pass: `::std::object::Values` keeps Rune's message, hash and all | `a_type_outside_the_map_keeps_the_message_rune_produced` |
| 4 a name the source does not contain falls back | pass | `method::tests::a_message_that_does_not_parse_is_left_alone` |
| 5 a fault that is not a missing method is left alone | pass: a script that panics with the text of one, and also calls `parts.join` so a candidate's hash really does match, reports a panic; the same script without the panic reports the missing method | `a_panic_that_quotes_the_diagnostic_is_still_a_panic` |
| 6 whitespace, comments, string literals, and identifiers beyond ASCII | pass: `parts . /* ordinary comment */ join(x)` and a line comment before the dot both name `join`; a name inside a string literal is not a candidate; `café.método(x)` names `método` | `a_comment_between_the_dot_and_the_name_does_not_hide_the_method`, `method::tests::candidates_are_the_names_used_in_method_position` |
| 7 the map is derived | pass: the displayed names and hashes come from the linked Rune's own types, and the test asserts the pair `Vec` and `join` reproduces `0xf77d93259f11131a`, the hash the second port's diagnostic actually printed | `method::tests::the_map_comes_from_the_linked_rune_and_reproduces_a_reported_hash` |
| 8 the upstream shape is pinned | pass: the fallback case asserts Rune still renders ``Missing instance function `0x`` and `` ` for ` ``, so a version that changes the wording fails here rather than falling back silently for ever | `a_type_outside_the_map_keeps_the_message_rune_produced` |
| 9 the place is unchanged | pass: file, line 3, column 2, the source line, and the caret, asserted alongside the new sentence | `a_missing_method_is_named_rather_than_hashed` |
| 10 nothing regresses | pass: 78 unit tests, 16 pseudo-terminal gates, 17 runner diagnostics, 12 exit-status gates, 8 standard-input gates, 3 run-output, 2 upstream reproducer, formatting clean | whole suite |

## What the grounding overturned

Record 0011 asked for this and called it "a small fix to a message that
already knows the answer". The premise was wrong in two ways, and finding that
out was most of the work.

- **The message does not know the name.** Rune's error carries a hash and the
  instance type. `VmErrorKind` is `pub(crate)` in 0.14.1, so rnx cannot read
  even those two structurally, let alone a name that is not there.
- **The position points at the receiver.** For `parts.join(UNIT)` the span
  starts at `parts`. Reading the first method name after it looks like an
  answer and is one, until a chain: `"abc".to_uppercase().frobnicate()` would
  be reported as a missing `to_uppercase`, which exists.

What made the cut possible instead was that
`Hash::associated_function(type_hash, name)` is public and reproduces exactly
what the error reports. So a name can be proved rather than guessed, and gate
2 is the case that separates the two.

## Controls

| Removed | Result |
| --- | --- |
| the hash match, naming the first candidate instead | 3 gates fail, including the chain gate end to end |
| the anchored match, recognising the pattern anywhere again | the panic gate fails, end to end |
| the token scan, requiring a name straight after the dot | the comment gate fails, end to end, and the candidate unit test with it |

### A control that did not fire, and why the code is still right

Replacing the unknown-type fallback with a wrong type hash changed nothing:
the suite stayed green. The edit was confirmed to have landed before the
suite was run, and the reason it does not matter is the design rather than a
weak gate.

Correctness rests on the hash match, not on the type lookup. A wrong type hash
produces no match, and no match prints nothing, so a wrong type cannot produce
a wrong name. The early return for an unmapped type saves a candidate scan
that would have failed anyway; it is not what keeps the answer honest.

That is worth stating because it is the property the record is really claiming.
The map can be incomplete, or wrong, and the worst outcome is the message Rune
already prints.

## Two defects this cut shipped, and a gate that hid one of them

Both were found in review, after the suite, the formatting, and every gate
above passed.

**An unrelated fault was rewritten as a missing method.** The message was
matched with `find`, which accepts the pattern anywhere in an error. A script
that panics with the text of a missing-method diagnostic was reported as a
missing method. The hash match does not protect against this and was never
going to: it proves which method a name refers to, and says nothing about
what went wrong. The message must now be that diagnostic and nothing else,
beginning and end.

**The candidate scan missed valid syntax.** It required an identifier
immediately after the dot, so `parts . /* ordinary comment */ join("-")` kept
the hash. Candidates now come from Rune's own tokens, which also stops an
identifier inside a string literal from being a candidate and handles an
identifier beyond ASCII.

**The gate for the first defect was too weak, and the control caught that.**
The reproduction carries `a.join(b)` in a comment. Once candidates came from
tokens, a comment held no candidates, so the token fix alone made that script
report a panic. The gate passed for a reason that had nothing to do with the
fix it was meant to hold, and its control did not fire.

The gate now puts a real `parts.join(x)` call in the code beside the panic, so
a candidate's hash genuinely matches and only the anchored message stops the
rewrite. It also asserts that the same script without the panic does report
the missing method, so the case discriminates rather than merely finding
nothing. With that, the control fires.

## Limits stated

- Only `Missing instance function` is rewritten. Every other message is
  untouched.
- The map holds the nine types a script commonly calls a method on. An
  iterator type, a struct a script defines, and anything else falls back.
- The hash and the type are read out of Rune's rendered text, because the
  structured form is crate-private. rnx pins `=0.14.1` and gate 6 fails if
  that text changes.
- Candidates come from Rune's tokens, so a name reachable only through
  something the source never spells as a method call is not a candidate, and
  the message falls back.
- Two conditions have to hold before a name is printed: the message is this
  diagnostic and nothing else, and a candidate's hash matches. Neither
  substitutes for the other.
