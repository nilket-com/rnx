# rnx 0007 evidence: the binding prelude

Implementation of `plans/0007_the_binding_prelude.md`, 2026-09-08, on Linux
x86_64 against upstream Rune 0.14.1 with no patch. Status of the record
stays proposed until reviewed. Figures are from a debug build on this
machine, measured the same way as the record's table: whole sessions, wall
clock, process start to exit.

## What landed

- `src/session.rs`: `mentioned` lexes an input with Rune's own lexer and
  returns the published names it may reference: identifier tokens, and the
  leading identifier of each `{...}` group in a string literal after that
  literal is decoded. The prelude restores only those, and `restored` builds
  the object holding them before anything runs. The unit returns a delta of
  the names it restored and the names it declared, and `merge` writes that
  delta into the session's state.
- `decode` mirrors the escapes Rune's lexer accepts and returns nothing for
  any escape it does not recognise, in which case the input restores every
  published name. `format_captures` then takes each `{...}` group's name
  as everything up to the first `}` or `:`, and the caller looks that
  up among the published names. Matching against a known set rather
  than parsing an identifier is what lets a name like `café` be found
  without this code holding any view on which characters an identifier
  may contain. `{{` is a literal brace and captures nothing, while
  `{{{name}}}` is a literal brace beside a real capture, and both are
  tested. The formatter reads the decoded string, so the scan must read
  it too: `format!("\\u{7b}x}")` is `{x}` and captures `x`, which a raw
  scan misses. Rune's own decoder is crate-private, so this one is
  written against the escape table in its lexer and refuses to guess
  beyond it.
- The session's state moved from a Rune object to a `BTreeMap` on this side.
  That is what makes publication atomic: every fallible step, building
  the restored object and reading the delta, happens before anything is
  committed, and the commit itself is a map insert that cannot fail. The
  previous version allocated and inserted into live state, so a failure
  partway could leave some rebindings published while returning an error.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 the prelude and the publication follow the input | pass: the same input generates byte-identical source in a session with a hundred names and in one with two, with exactly two restoring lines | `the_generated_source_follows_the_input_not_the_session` |
| 2 the session stops slowing down | pass, with a measured regression on the cheap case; see below | measured |
| 3 a pinned unit stops carrying the session | pass by the same measurement as gate 1: a unit's generated source is the size of its input, so a closure pins that rather than the session | `the_generated_source_follows_the_input_not_the_session` |
| 4 state semantics are preserved, case by case | pass: assignment to a restored name, shadowing, destructuring, a closure capturing a restored binding, two names aliasing one handle mutated through one and read through the other, a binding untouched for twenty inputs, and an input that mutates a shared handle and then fails, publishing no rebinding while the mutation stands | `assignment_shadowing_and_destructuring_publish_correctly`, `aliases_keep_one_shared_handle_across_a_narrowed_prelude`, `a_closure_captures_a_restored_binding_and_keeps_seeing_it`, `a_failed_input_publishes_no_rebinding_and_keeps_its_mutation` |
| 5 diagnostics are unchanged | pass: the 0002 origin tests are green, including an error in a function defined earlier and one through a retained closure | `src/session.rs` tests |
| 6 format captures are restored | pass: `format!("{captured}")`, a format spec, `println!`, and a template interpolation all read a binding ten inputs old; a capture hidden behind `\u{7b}` or `\x7b` is found because the literal is decoded first; an escape the decoder does not recognise restores every published name; a capture naming a non-ASCII binding is found, written literally or as an escape, alone or before a spec; a published name that is a prefix of another does not shadow it; `{{` captures nothing and `{{{name}}}` still captures | `a_format_capture_restores_the_binding_it_names`, `a_template_interpolation_restores_the_binding_it_names`, `escape_tests`, `unicode_capture_tests` |
| 6b publication is atomic | pass by construction and stated: the fallible steps precede the commit, and the commit is a map insert that cannot fail | `merge`, `restored` |
| 7 nothing regresses | pass: 61 unit tests, 13 pseudo-terminal tests, 2 reproducer tests, formatting clean, spike self-checks pass | whole suite |

### Gate 2, measured, both directions

Baseline re-measured on the parent commit under the same conditions rather
than quoted from the record, because the record's figures were taken at a
different time. Medians of five runs for the eight-hundred-input sessions.

| Workload, 800 inputs | Before | After |
| --- | --- | --- |
| Distinct names | 1,587 ms | 337 ms |
| One rebound name | 188 ms | 266 ms |

| Generated source, last input | Before | After |
| --- | --- | --- |
| Distinct names, 800 | 29,334 B | 95 B |
| One rebound name, 800 | 115 B | 115 B |

| Tracked live growth, 800 inputs | Before | After |
| --- | --- | --- |
| Distinct names | 177,854 B | 148,615 B |

The distinct-name session is 4.7 times faster and its generated source no
longer grows at all.

**The cheap workload got slower, by about 78 ms over 800 inputs.** That is
roughly 98 microseconds an input, and it is not the scan. Two experiments
separated them on the earlier shape of this cut: with the scan removed and
the delta kept, the rebound workload still ran slow; with the delta replaced
by the old wholesale assignment, it returned to the baseline. The cost
belongs to the two mechanisms the record requires, building the restored
object an input receives and publishing its delta, and it does not vary with
the size of the session.

The trade, stated plainly: the new mechanism costs about 98 microseconds an
input whatever the session holds, while the old prelude cost about 1.75
milliseconds an input at eight hundred names and kept growing. The cut pays
for itself somewhere around fifty published names and is imperceptible
either way at the keyboard, where an input takes a fraction of a
millisecond. Why those mechanisms cost what they do is located but not
explained; reducing it is forward work rather than a guess made now.

## Limits stated

- The timings are whole sessions and do not separate compilation from
  execution, publication, or the loop around them. They measure what a
  person waits for.
- A unit's own storage size is still not measurable, for the reason the
  record gives.
- The scan over-approximates by design. A published name inside any string
  literal in `{...}` costs one restoring line whether or not it is a format
  argument, which the scan test pins deliberately. It holds no view on what
  an identifier may contain: a group's name is matched against the published
  names, so any name a session can hold can be found.

## Forward

The cost of the restored object and the delta merge, located but not
explained. The retained input text,
which still grows with the session. And the shared runtime context, still
waiting on the upstream fix that record 0006 reproduced.
