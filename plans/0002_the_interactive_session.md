# rnx 0002: the interactive session

Status: proposed 2026-09-08. The second record of rnx. The first release
record settled what a session means: what it retains, how it fails, what it
refuses. This record makes `rnx repl` a terminal session a person keeps
open, in the way an IPython session is kept open. It decides the input
model, the display of values, where diagnostics point, and what Ctrl-C does.
It does not decide completion or introspection; those are named in Forward
and scoped by what this cut leaves behind.

## Context

### What exists

The spike's `repl` reads standard input a line at a time, treats each line
as one input unless the user brackets several with `:begin` and `:end`,
prints every result through JSON serialization, and reports serialization
failure for values JSON cannot represent, such as functions and structs.
Diagnostics come from the adapter's generated unit, so a compile error names
positions in source the user never wrote. There is no editing, no history,
and no way to stop a running evaluation except the instruction budget.

### What upstream provides

Rune's compiler reports running out of input through a small set of error
kinds rather than one: an expected-token error whose actual token is end of
input, a bare unexpected end of input, an unterminated string literal, and
an unterminated block comment. The adapter wraps each input in a block, so
an input that ends inside an open bracket reaches the parser as an
unexpected closing brace at the wrapper's position rather than as end of
input. A continuation prompt therefore has to decide completeness against
the original input with those cases named, not against one error variant.

Rune's execution object can run under an instruction budget and reports
exhaustion as a halt that can be resumed, so evaluation can proceed
in bounded slices with a check between them; that is the mechanism for
interrupting a running input without a fork.  Rune's values carry their
own debug formatting. None of this is a change to the compiler or the VM.

### Why this cut and not completion first

Completion, `:vars`, and `:help` make a session discoverable. They are worth
little in a session that cannot recall the previous input, cannot enter a
function across lines without ceremony, and prints `<serialization error>`
for a struct. The daily-use floor comes first.

## Decision

### 1. Editable input with persistent history

`rnx repl` reads through a line editor: cursor movement, in-line editing,
multi-line editing of the current input, history recall with the arrow keys,
incremental history search, and paste that arrives as one input. History
persists across sessions as source text only, in a file under the user's
state directory, appended on each accepted input. Restoring history never
executes anything: a reopened session has its history available and its
execution state empty. A line editor crate under permissive terms supplies
this; the choice is an implementation detail recorded in the evidence, not a
promise of this record.

### 2. Natural multi-line entry

Completeness is decided against the original input text. The input is
incomplete, and the session shows a continuation prompt and appends the
next line, exactly when the parser's failure is one of these, positioned at
the end of the original input once wrapper offsets are removed: an
expected-token error whose actual token is end of input; unexpected end of
input; an unterminated string literal; an unterminated block comment; or an
unexpected closing token that the adapter's wrapper supplied, which is how
an open bracket, brace, or parenthesis presents. Every other failure means
the input is complete and invalid, and its diagnostic prints at once. The
list is closed; a case not on it is a diagnostic. `:begin` and
`:end` are removed. A blank line on a continuation prompt with a still-open
input keeps waiting; two blank lines in a row abandon the input, and the
abandoned text stays in history so it can be recalled and repaired.

### 3. Values display as values

A result is rendered by the session's own formatter walking Rune's value
model directly: strings quoted, numbers and booleans plain, unit omitted,
vectors and tuples with their elements, objects with their keys, struct and
enum instances with their type name and fields, and functions and closures
by name and arity where the runtime's metadata supplies them, otherwise as
an opaque function. Any value the formatter does not walk prints as an
opaque marker carrying its type name. The formatter invokes no formatting
protocol through the VM by default: it does not call a value's own debug or
display implementation, because those run script code. The whole rendering
is bounded: nesting at a depth, collections at a length, strings at a byte
length, and the total output at a byte budget, each cut shown with an
elision mark and the count of what was omitted. A shared handle already on
the current render path prints as a cycle marker instead of recursing. JSON
is an explicit operation, a host function the user calls, and never the
default display. A statement that produces unit prints nothing.

### 4. Diagnostics point at what was typed

Every diagnostic reports a position in the user's input text: the input's
number in the session, and the line and column as entered, with the
offending line shown and the span marked. The session owns a source map per
compiled unit, keyed by the identity of the input that produced it, and
keeps the map alive as long as the unit can still run. A declaration that
is recompiled into later units keeps the identity of the input in which it
was last entered, so a runtime error inside a function defined three inputs
ago reports that input's number and position, not the current one. The
adapter maps positions through the unit that ran before printing. A
diagnostic whose position falls in adapter-generated text rather than user
text is a defect in the adapter and is reported as such,
with the generated source available under a `:debug` command, never in the
default output.

### 5. Ctrl-C is predictable

At the prompt, Ctrl-C clears the input being edited, including an open
multi-line input, and shows a fresh prompt. During evaluation, Ctrl-C sets
an interrupt flag. Evaluation runs in bounded instruction slices and the
flag is checked between slices. That is the whole guarantee: the interrupt
is observed at the next slice boundary. For pure Rune execution the latency
is one slice, and the evidence states the slice in instructions and the
measured milliseconds. A host call in progress delays the boundary until it
returns; a child process wait is the one host call with its own cancellation
path, the process cancellation of the first release record, which Ctrl-C
also triggers, and every other host call runs to completion first. The
evidence lists the host calls and which of the two behaviours each has. When
the interrupt is observed, the input's candidate bindings are not published
and the prompt returns; the first release record's failure rules apply
unchanged, so mutations already made through shared handles and effects
outside the process stand. The flag is cleared before each evaluation
starts, so an interrupt that arrived at the prompt never cancels the next
input. Ctrl-D at an empty prompt ends the session;
`:quit` does the same.

### 6. What this record does not decide

Completion of any kind; `:vars`, `:help`, and introspection; syntax
highlighting; a language server connection; and any change to the session
semantics of the first release record.

## Acceptance gates

1. **The ordinary session.** From a fresh launch: define a vector and a
   struct on one line; define a three-line function using the continuation
   prompt; recall the function with the up arrow, change its body, and
   re-enter it; call it and see the struct displayed by type and fields;
   enter an input with a syntax error and see the diagnostic mark the
   column in the line typed; start an infinite loop and stop it with
   Ctrl-C; call the function again in the same session. The continuation
   prompt is exercised on each incomplete form named in decision 2: an
   open function body, an open vector literal, a `let` with no value, an
   unterminated string, and an open block comment; and it is not shown for
   the complete-but-invalid controls, a `let` with a missing expression
   before a semicolon and a call with a stray closing parenthesis, each of
   which prints its diagnostic at once. All as one
   scripted transcript, run against the binary with a pseudo-terminal, and
   its output compared to an expected transcript.
2. **History across sessions.** Close and reopen: the previous inputs are
   recallable, the function is not defined, and nothing ran on launch. A
   history file containing an input with side effects, such as a file
   write, produces no such effect on reopen.
3. **Display limits.** A vector of ten thousand elements, an object nested
   twenty deep, a string of ten megabytes, a vector whose full rendering
   would exceed the total budget, and a vector that contains itself each
   print within the limits, with the elision or cycle mark and the omitted
   count; a value of a type the formatter does not walk prints as the opaque
   marker with its type name; a struct with a script-defined debug
   implementation prints by its fields without that implementation running,
   proved by an implementation with an observable side effect; and the JSON
   host function prints the whole value when asked.
4. **Position mapping.** A compile error on line two, column seven of a
   three-line input reports line two, column seven; a runtime error in a
   function defined two inputs earlier reports the position in that earlier
   input; no default-output diagnostic references adapter-generated text,
   checked by a test over every diagnostic the adapter can produce for its
   supported input forms.
5. **Interruption.** An infinite loop interrupted by Ctrl-C returns to the
   prompt within one slice, with the slice stated in the evidence in
   instructions and in measured milliseconds; a shared-handle mutation
   before the loop survives, and a `let` in the same input is not published.
   A child process that sleeps past any reasonable wait is interrupted by
   Ctrl-C through process cancellation and the prompt returns with the
   documented result. After each interrupted input, the next input runs to
   completion, proving no leftover flag cancels it.
6. **Ctrl-C at the prompt.** An open three-line input is cleared by Ctrl-C
   and the next input is evaluated on its own.
7. **Nothing regresses.** Every gate of the first release record that this
   cut touches, the session-rule gates in particular, still passes.

## Guardrails and stop conditions

1. Completeness is decided only by the closed list in decision 2, against
   the original input's end position; a failure not on the list, or one
   positioned anywhere but the end, is a diagnostic, never a guess.
2. If interruption cannot be delivered within a bounded slice through the
   execution object's own resumable halt, stop; do not reach into the VM.
3. If a position cannot be mapped to user text, the diagnostic says so
   explicitly rather than printing the generated position.
4. History restore executes nothing, ever; a test proves it with a history
   file that would have an observable effect.
5. No change to the first release record's session semantics rides on this
   cut; a needed change gets its own record.

## Risks

- **Line-editor terminal handling differs across the three platforms.**
  Gate 1 runs on Linux in this cut; the Mac and Windows runs are the first
  release's platform ladder and are named as not yet done rather than
  assumed.
- **Slice granularity trades interrupt latency for throughput.** The
  evidence records both at the chosen slice size; the size is a constant
  with a stated reason, not a tunable.
- **Position mapping is only as good as the adapter's bookkeeping.** Gate
  4 tests every diagnostic path, not a sample, and the per-unit source maps
  are retained memory that the first release record's bound must charge.

## Forward

Completion of retained names, declarations, host functions, and commands,
then `:vars` and `:help <name>`, each as its own record with the same
transcript-style gate. Introspection is bounded by what Rune exposes at
runtime and is not promised in Python's terms.
