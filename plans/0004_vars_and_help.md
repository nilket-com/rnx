# rnx 0004: vars and help

Status: proposed 2026-09-08. The fourth record of rnx. Completion (0003)
makes the session's names reachable at the keyboard; this record makes them
legible: `:vars` lists what the session holds, and `:help <name>` describes
one thing. Both read the same session tables and host registration that
completion reads, both run no Rune code, and both are bounded in what they
print. It leaves inferred types and any documentation of the standard
library for later.

## Context

### What the session can tell about a name

The session holds each published binding's value in one object, each
retained declaration's source text, and the input each declaration was last
entered in. A value's type is readable structurally through Rune's value
model, the same way the formatter reads it, without running anything. A
host function is known only by the path it was registered under; upstream
exposes no signature or documentation for it, so anything shown beyond the
path is text rnx carries at the registration itself, next to the function,
where completion's name list already comes from.

### Why now

A session a person keeps open is worth more when they can ask what is in it
and what a name does without scrolling back or guessing. The pieces are all
present after 0003; this cut only presents them, within bounds.

## Decision

### 1. Every invocation is bounded as a whole

`:vars` and `:help` each render under one total byte budget for the whole
invocation and, for `:vars`, a maximum number of bindings. Per-value limits
are not enough: a thousand small bindings are each within their limits and
together are a screenful of scrollback. When either bound is reached the
command stops traversing, prints nothing further, and ends with a marker
naming what was cut. The budget covers everything a command prints,
including declaration source and the command listing, not only rendered
values.

### 2. `:vars` lists the session's bindings

`:vars` prints each published binding once, in name order, with its type and
a bounded rendering of its value, using the display limits of 0002 within
the invocation budget of decision 1. Declarations are not bindings and are
not listed by `:vars`; `:help` covers them. With no bindings, `:vars` says
so. The rendering runs no protocol and compiles nothing, as the formatter
already guarantees.

### 3. `:help <name>` describes one thing

`:help` with an argument resolves the argument in this order and stops at
the first that matches: a published binding, whose help is its type and the
bounded rendering of its value; a retained declaration, whose help is its
kind and its retained source text; a host function path, whose help is the
one-line description carried at its registration; and a session command,
whose help is its one-line description. A name that is both a binding and a
declaration is reported as the binding, and the help says the name is also
a declaration so the person is not misled by the precedence. A name that
matches nothing gets a message saying so. `:help` with no argument lists the
commands with their one-line descriptions and points at `:vars`.

### 4. Displayed source is safe for a terminal

Retained declaration source is text a person typed and may contain literal
control characters inside a string or a comment. It is printed through a
terminal-safe path that escapes control characters, escape included, while
preserving the line breaks and indentation that make source readable. The
same path covers any other stored text a command prints. Values already go
through the 0002 formatter, which quotes and escapes them.

### 5. Host and command descriptions have one source

Each host function's one-line description lives at the point it is
registered, in the same place its path is recorded, so a function cannot
exist without its help or drift from it; a test checks every registered
path has a description and every description belongs to a registered path.
Each command's description lives with the command's handling. Neither is a
second catalogue kept in step by hand.

### 6. Reading, never running, and available past the memory threshold

`:vars` and `:help` read the session's binding values, declaration sources,
and the registration record. They evaluate nothing, compile nothing, and
invoke no formatting protocol. A binding holding a closure with a side
effect is listed and described with no effect. Because they run nothing,
they stay available when the first release record's memory threshold has
been crossed and Rune evaluation is refused: like `:memory` and `:reset`,
they are bounded host-side inspection, and inspecting a session that is over
its bound is exactly when a person needs them.

### 7. What this record does not decide

Inferred or declared types beyond what a value carries at runtime; the
signature or argument names of a binding that is a function, which Rune
does not expose on a value; documentation of the standard library, which
upstream cannot enumerate; and search or listing of host functions by
anything but exact path.

## Acceptance gates

1. **`:vars` after work.** Bind an integer, a string, a vector, and a
   struct; `:vars` lists the four in name order, each with its type and its
   value within the display limits; a deeply nested binding is elided like a
   result. With a fresh session, `:vars` reports none. After `:reset`,
   `:vars` reports none.
2. **`:help` on each kind.** `:help` on a binding shows its type and value;
   on a declared function, its kind and source; on a struct, its kind and
   source; on `host::write_new`, the registered description; on `:memory`,
   the command description; on an unknown name, the not-found message;
   with no argument, the command list.
3. **The invocation budget holds.** A session of many bindings, each small,
   prints within the total budget and ends with the marker naming the
   bindings not shown; a single binding whose rendering alone exceeds the
   budget is cut with the marker; a declaration whose source exceeds the
   budget is cut with the marker; the command listing is subject to the same
   budget. In each case the output is within the budget plus the marker.
4. **Source is terminal-safe.** A declaration whose body contains an escape
   character in a string literal, and one containing an escape in a comment,
   are shown by `:help` with the escape rendered visibly and no control
   byte in the output, while their line breaks and indentation survive.
   Neither declaration is executed by the test.
5. **State is followed.** A binding from a failed input is not listed by
   `:vars` and not found by `:help`. An input that mutates an existing
   shared binding and then fails leaves that mutation visible in `:vars`
   and in `:help` for that binding, while the same input's new binding is
   absent, which is the first release record's failure rule shown through
   these commands. A redefined declaration shows its new source. A name
   that is both a binding and a declaration reports as the binding and says
   the declaration exists. After `:reset`, only commands and host functions
   have help.
6. **Available over the bound.** With the session past its memory
   threshold, so that evaluation is refused, `:vars` and `:help` still
   answer, within their budget.
7. **No execution.** A binding holding a side-effecting closure and a host
   function with a side effect are listed and described with the effect not
   occurring, proved by an observable one that does not happen.
8. **Nothing regresses.** The 0002 and 0003 gates pass unchanged.

## Guardrails and stop conditions

1. If describing a name would require evaluating, compiling, or invoking a
   protocol on a value, that part of the description is omitted, not
   computed.
2. No handwritten list of upstream names or signatures; a host function's
   help is the description carried at its own registration, nothing more.
3. `:vars` and `:help` render values through the 0002 formatter and its
   limits; no second renderer.
4. Nothing a command prints reaches the terminal without passing either the
   formatter or the terminal-safe text path.
5. No change to the session semantics of 0001, 0002, or 0003 rides on this
   cut.

## Risks

- **A type shown for a value is the value's runtime type, not a declared
  or inferred one.** That is all Rune exposes on a value, and the record
  says so rather than implying more.
- **Host descriptions are only as good as the text at the registration.**
  Gate 5 keeps them present and attributed; it cannot judge their prose.
- **A budget that is too small hides what a person asked for.** The marker
  always names what was cut, so a truncated answer is never mistaken for a
  complete one.

## Forward

Listing host functions by a prefix search rather than exact path; `:help`
for a declaration showing the bindings it closes over; and, if upstream
gains a way to expose a function value's arity, a signature in a binding's
help.
