# rnx 0042: the prompt a person lives at

Status: proposed 2026-09-14; revised the same day after review. The
forty-second record of rnx, and the
first drawn from using the session rather than testing it. Four small
things a person reached for and did not find: a short quit, a way to
clear the screen, a prompt that is not four characters wide, and a way
to stop the greeting after the hundredth launch. Each is its own decision
and none is large; they are one record because they are one afternoon's
worth of friction.

## Context

The observations, from a person driving the session on Linux and Windows
on 2026-09-14:

1. `:q` is what fingers type; rnx has only `:quit`.
2. `:reset` clears the session's state but leaves the screen full of old
   work; there is no `:clear`.
3. The prompt is `[3] rnx> `. The `rnx> ` is redundant with the window it
   is in, and IPython, python, and the shell all prompt in one or two
   characters.
4. The greeting — `rnx: a Rune session. :help lists the commands, :quit
   ends it.` — is useful once and noise thereafter.

None of these is a guess about what would be nice; each is a thing that
was wanted and missing.

## Decision

### 1. `:q` ends the session, beside `:quit`

`:q` is `:quit`. `:help` lists it as `:quit` (`:q`) on the one line, and
completion offers both. Nothing else is aliased in this record; `:q` earns
it by being the one every editor and pager trained a person to type.

### 2. `:clear` clears the screen; `:reset` does not

`:clear` clears the terminal and puts the prompt at the top — the two
escape sequences a person's `clear` command sends. It touches no state:
the bindings and the numbering are as they were, the next prompt keeps
its number, and `:clear`'s own line goes into history like any typed line,
as `:renumber`'s does.

`:clear` is a **terminal control, not colour**, and the two are gated
separately. It emits when standard output is a real terminal and rnx is
not in an unsupported-terminal mode (`dumb`, `cons25`, `emacs`), and it
does so **whatever `--color` is**: a person who runs `--color=never`
still wants their screen cleared, because clearing is not styling. The
converse holds too and matters more: `--color=always` into a pipe emits
no clear and no title sequence, because those are terminal effects and a
pipe has no terminal, while `always` still colours the text going through
it. On a pipe, an unsupported terminal, or where there is no terminal,
`:clear` emits nothing and is not an error.

`:reset` and `:clear` stay separate, and this record recommends they stay
that way rather than `:reset` clearing the screen too. The reason is that
they compose — a person who wants both types both — while a person who
wants one without the other cannot get it if they are fused: resetting
the state while keeping the scrollback is exactly what someone does when
they want to copy a value from earlier before starting clean. Fusing them
takes that away; keeping them apart costs one extra word. If use proves
that nobody ever wants them apart, fusing is a one-line change later; the
reverse is not.

### 3. The prompt is `>`, with its number

```
[3] > x + 1
```

`rnx> ` becomes `> `. The tool's name belongs in the window title
(decision 5), not in front of every line a person types; the window says
what program this is, and the prompt says where the cursor is. The number
record 0040 put in front stays, so the prompt is `[n] > `.

This changes the prompt suffix, which the session tests wait on. That is
an intended transcript change, and the record names the tests it touches:
`tests/repl.rs`, `tests/numbering.rs`, and `tests/http.rs`'s `Repl`
helper, whose expected prompts become `> ` (with the number where they
already expected one). Nothing outside those changes, because a pipe
without `TERM=dumb` sees no prompt at all.

The continuation prompt for a multi-line input stays what it is — the
editor's own — because it is the same buffer under the same input, and
record 0040 left it alone.

### 4. The number is coloured, on the prompt and on the result marker

Today the whole prompt is bold, and record 0040 gave the result marker
the same bold. Splitting the prompt into a coloured `[n]` and a bold `>`
leaves the marker's style to settle, and this record settles it: the
`[n]` is coloured — the number colour record 0039 has, cyan on the
terminal palette — and the `>` is bold, and **the result marker `[n]`
takes the same colour as the prompt `[n]`**, so an input's number and its
result's number match, which is the pairing IPython's `In`/`Out` give.
It is styling, under record 0039's switches, so a pipe and `--color=never`
get the plain `[n] > ` and the plain `[n] value`, and the strip-to-plain
equality gate holds for both. Which exact accent is record 0043's to make
configurable; this record uses the existing number colour so nothing is
left half-styled while that is decided.

### 5. A window title, and `--no-splash`

**The title.** When standard output is a real terminal and rnx is not in
unsupported-terminal mode, the session pushes the title stack and sets
the title to `rnx` on start (`\x1b[22;2t` then the set), and `run` sets
`rnx <file>` for the length of the run. Restoration depends on how each
exit leaves, and the two are different:

- The **session** ends by returning: `:quit`, `:q` and EOF break the
  loop and `repl::run` returns, and a propagated error returns too. A
  scope guard restores the title (pops the stack) on that return, because
  the session's own exit does not go through `process::exit`. Ctrl-C at
  the prompt does **not** end the session — it continues — so it does
  not restore the title and does not need to.
- `run`, `eval`, and `host::exit` end by calling `process::exit`, which
  runs no destructors, so a guard cannot fire. Each emits the pop
  (`\x1b[23;2t`) explicitly before it calls `process::exit` — on normal
  completion, on a propagated error, and on `host::exit` — rather than
  relying on unwinding.

Where the terminal has no title stack the restore is best-effort: rnx
cannot read the previous title to set it back, so it pops if it can and
otherwise leaves `rnx`, which is stated. A crash that bypasses both a
guard and the explicit paths leaves the title set; nothing portable
prevents that, and it is named rather than pretended away. Emitted only
to a terminal, escaped-safe, nothing to a pipe.

**The greeting.** `--no-splash` before the command word suppresses the
banner. It is a global flag, parsed where `--color` is, before the command
word: after the file path it is an ordinary script argument reaching
`env::args()`, and in the runner's own position (`rnx run --no-splash
f.rn`) it is taken as the path and refused as unreadable, exactly as
`--color` is there — both orders of both flags are gated. The banner
otherwise prints as it does today, including into a pipe, because a test
asserts it and a first-time piped user is a real user; `--no-splash` is
how the hundredth-time user turns it off, and record 0043's config can
set it as a default so it need not be typed.

### 6. What this record does not decide

- Any other alias, any other colon command.
- Configurable colours or a config file: record 0043.
- A prompt that shows anything but the number — a mode, a timer, a
  directory — which is a bigger question about what the prompt is for.

## Acceptance gates

1. **`:q`.** `:q` ends a session exactly as `:quit`; `:help` shows both
   and completion offers both.
2. **`:clear`.** On the pty harness, `:clear` clears the screen and the
   next prompt is at the top with its number unchanged, the bindings
   intact (`:vars` after lists what it did before), and its own line in
   history; it works under `--color=never`; through a pipe and in
   unsupported-terminal mode it emits nothing and is not an error; under
   `--color=always` into a pipe it still emits nothing; the two escape
   sequences are the only bytes it adds on a real terminal.
3. **The prompt and markers.** The prompt is `[n] > ` on a terminal and
   in unsupported-terminal mode; the named tests are updated and no other
   test changes; stripping the styling gives `[n] > ` and `[n] value`
   exactly, with the `[n]` coloured and the `>` bold and the result
   marker's `[n]` the same colour as the prompt's.
4. **The title, on each kind of exit.** On the pty harness, starting a
   session pushes and sets the title to `rnx`; it is restored on a normal
   `:quit`/`:q` and on EOF (the session returns, and the scope guard
   fires); Ctrl-C at the prompt continues the session and leaves the
   title `rnx` and the session running; `run` with a terminal sets `rnx
   <file>` and restores on completion, on a propagated error, and on
   `host::exit`, each of which reaches `process::exit`, so the restore is
   the explicit emit and not a guard; where the stack is unsupported the
   restore is best-effort and not asserted; through a pipe no title
   sequence is emitted.
5. **`--no-splash` and its placement.** `--no-splash` suppresses the
   banner in a session and a piped session; `rnx run f.rn --no-splash`
   delivers it to `env::args()`; `rnx run --no-splash f.rn` is taken as
   the path and refused, as `--color` is; both orders of `--no-splash`
   and `--color` together are gated; the banner is the only thing the
   flag changes.
6. **Existing guarantees.** Both suites pass with only the named prompt
   and banner tests changed; startup and the session baseline before and
   after.

## Guardrails and stop conditions

1. Terminal controls — `:clear` and the title — emit only to a real
   terminal outside unsupported-terminal mode, only escaped-safe
   sequences, and nothing under a pipe, independent of `--color`: colour
   styling and terminal control are separate switches and neither leaks
   into the other's channel.
2. `:reset` clears state and not the screen; `:clear` clears the screen
   and not state. If the implementation blurs them, stop.
3. The prompt change touches only the named tests. If any other test
   must change, a transcript changed that should not have.

## Risks

- **A terminal that ignores the title stack.** It sets the title and does
  not restore it; the person's title is now `rnx`. Common, mild, and the
  plain-set fallback is deliberate; a terminal that supports the stack
  restores.
- **`>` is a common prompt and less identifiable in a screenshot.** The
  window title carries the identity now, and a screenshot of a terminal
  usually shows it. Stated; the name moved, it did not vanish.
- **Someone scripting the session expected the banner.** `--no-splash`
  is opt-in and the default is unchanged, so nothing that reads the
  banner today stops seeing it.

## Forward

Record 0043: colours a person can choose, and a `.rn` file to choose them
in, which is where `--no-splash` becomes a saved default.
