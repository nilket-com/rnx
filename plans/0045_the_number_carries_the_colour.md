# rnx 0045: the number carries the colour

Status: implemented 2026-09-14; Linux gates pass, with evidence beside
this record. Windows execution remains unverified. The forty-fifth record of rnx. It separates
input from output at a glance, then separates the digits from their frame.

## Context

Record 0040 pairs an input and its result with the same number. Records
0042 and 0043 currently paint both entire bracketed numbers bold green
through `PromptNumber` and the config key `prompt_number`. The user wants
IPython's distinction between inputs and outputs, with blue rather than
red for output because red should continue to signal an error. The user
also asked whether brackets or digits should carry that colour.

This record chooses coloured digits and a dim frame: the number is the
reference, while brackets and the arrow delimit it. Both input and output
are already styled; rustyline's `highlight_prompt` styles the input even
though the prompt object's `styled()` method returns the original text.

## Decision

1. Input digits retain `prompt_number`, bold ANSI green (`1;32`). Output
   digits get `result_number`, bold ANSI blue (`1;34`). Plain blue is the
   chosen palette slot, not a fixed RGB value; bold is retained on both
   so distinguishing direction does not sacrifice the output's weight.
   `bright-blue` remains a configurable alternative for a terminal whose
   blue is difficult to read. No claim of identical appearance across
   terminal themes is made.
2. Brackets on both lines and the prompt's `>` use `prompt_frame`, dim
   default foreground (`2`), as comments do by default. This is a request
   to dim the terminal foreground, not a guaranteed shade of grey: a
   terminal may ignore faint intensity. A configured frame foreground
   retains the dim attribute. Spaces keep their existing bytes; they may
   share the frame span. Digits never inherit dimness because spans reset
   before applying the next style. Result digits remain bold.
3. The palette grows from six keys to eight by appending `result_number`
   and `prompt_frame`. Existing indices/keys and optional-key behaviour
   stay compatible. An old `prompt_number` override now colours only input
   digits: separating the result is the deliberate presentation change.
   It does not implicitly override either new key. Document this migration.
   The two new keys accept the same sixteen ANSI names and six-digit RGB
   grammar as existing keys. Foreground overrides do not change weight.
4. Input prompt and result marker use one span-building helper. Only the
   ASCII digits inside the brackets receive the input or output number
   role; the bracket/arrow frame receives the other role. The raw forms
   remain `[N] > ` and `[N] `. Unit still prints nothing, and counting,
   renumbering, error origins and admission rules remain record 0040's.
   Error and caret styles, value syntax colours and input highlighting are
   untouched. In particular a number inside a value is still cyan.
5. Scope is the session's displayed prompts and markers only. Reader
   selection, automatic colour policy, `NO_COLOR`, `--color` and unsupported
   terminal behaviour are unchanged. Run and eval still emit no markers
   and never load the config. Stripping permitted SGR from presentation
   gives exactly the same text as before; no spacing or cursor-width change
   is part of this record.

## Gates

1. Unit assertions pin the spans/SGR for one- and multi-digit input and
   output numbers. Stripping those sequences restores the raw text;
   disabled colour returns the raw text. Each span resets before the next.
2. The regular pty suite asserts green input digits, blue output digits
   and dim frame, with the value still cyan. Update the old same-style
   expectation explicitly. The existing edit/cancellation gate asserts
   the new prompt bytes and retains balanced-style checks. Plain numbering
   and renumbering gates stay unchanged.
3. Configured input, output and frame each get distinct foregrounds in
   a real terminal fixture, with input/output bold and frame dim. An old
   input-only override leaves output blue and the frame dim/default.
   Invalid new keys/values follow the existing warning/default contract.
4. Preserve a before binary. Plain session transcripts, run and eval
   outputs remain byte-identical for representative values, errors and
   renumbering. Existing suites run with and without test-support.
5. Capture a session containing an input, a result, a multi-digit prompt,
   renumbering and an error. Compare screen text/cursor under never and
   always, preserve raw bytes, and render dark/light specimens in rnx-bench.
   A renderer must account for faint style or disclose when it does not;
   a fabricated grey is not evidence of a terminal's behaviour. Report
   which example terminal palette is shown. These are specimens for the
   user's eye, not proof of every terminal's contrast.

## Guardrails and limits

No new dependency, startup config scope, terminal-control sequence, history
format or numbering semantics. Only SGR metadata changes. Config remains
session-only. Windows receives the same ANSI requests under the existing
console policy; Windows execution and physical terminal appearance remain
unverified until observed there. This record does not promise that every
terminal palette's blue or faint foreground is equally readable.
