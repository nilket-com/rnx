# rnx 0045 evidence: input, output and frame

2026-09-14, Codex, nano, Linux x86-64, Rust 1.98.1. Plan `5263a06`;
baseline `6fe1bf3`. Raw terminal captures, output comparisons and specimens
are in rnx-bench commit `638ffc3`, `results/number_colours_0045/`.
The runnable probe is `probes/number-colours/terminal.py`, using the existing
pinned terminal-probe dependencies. No rnx dependency changed.

## Implementation

`PromptNumber` remains bold green. `ResultNumber` is bold ANSI blue.
`PromptFrame` uses faint default foreground. Config appends `result_number`
and `prompt_frame` to the six existing keys without moving their indices.
Configured foregrounds preserve bold on both digit roles and faint on the
frame. An existing `prompt_number` setting no longer colours the output;
the README explicitly documents that presentation change.

One `presentation::numbered` helper emits frame, digit and frame spans over
the original text. Rustyline's prompt highlighter and the result-marker
printer both use it. Every span resets before the next; a frame's faint
attribute cannot leak into the digits or the result value. Counting,
admission, unit silence, renumbering and reader selection do not change.
Run/eval config scope and the error/caret/value styles remain untouched.

## Verification

- Full default suite: **340 passed, zero failures**.
- Full test-support suite, run after default: **381 passed, zero failures**.
- Unit gate pins exact span sequences for 1, 12 and 100, no-colour identity,
  stripped-text identity and unchanged non-numbered prompts.
- The regular pty gates verify green input digits, blue result digits and
  dim framing, with the result value still cyan. The older cancellation
  gate uses the revised prompt bytes and retains its balanced-reset check.
- A pty config fixture sets all three roles to distinct RGB values and
  asserts the foreground and weight of each span. The older input-only
  config fixture confirms that output still uses default blue, while its
  existing input, value, comment and error overrides keep working.
- Config validation accepts both new keys, including bright-blue and RGB,
  and warns by key for invalid values. Existing config discovery and
  session-only read-counter gates remain in the passing suites.
- The terminal probe reaches input/result 10, renumbers, renders a structured
  value and produces a named runtime error. Never/always screen contents
  and cursor positions match. Raw captures preserve the SGR evidence.
- Plain before/after stdout, stderr and exit status match for bare run,
  JSON workload run, successful and failing eval, and a session including
  renumbering and an error, with TERM=xterm and TERM=dumb separately.
  These checks concern exact output, not timing.

The final release built with `--release --locked`. The suites preceded a
formatting-only pass on the new helper; the final release terminal probe
ran after it. Before/after binary hashes are in `checks.json`.

## Specimens and limits

`specimen-dark.png` and `specimen-light.png` use Tango's normal ANSI slots
on `#202428` and white, with DejaVu Sans Mono. Inspection of both shows a
readable bold blue output number distinct from the green input number.
The number remains the accent, and the frame recedes. This is the evidence
for choosing normal blue here; a person may choose bright-blue for another
terminal palette.

Pyte 0.8.2 does not retain faint intensity. The probe extends its cell model
to retain SGR 2/22/0, with a self-check that RGB channels are not interpreted
as attributes. Specimen rendering blends faint foreground halfway toward
the background. That is an explicitly chosen illustration, **not a measured
physical-terminal rendering**. The raw capture proves the actual faint
request. Real terminals may ignore faint or substitute bright colours for
bold. Windows execution and physical Windows appearance remain unverified.

No startup performance improvement or penalty is claimed by this record.
It changes SGR spans, not the raw prompt width or one-shot output contract.
