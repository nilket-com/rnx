# rnx 0039: colour belongs to the presentation

Status: implemented 2026-09-14 after review and the Linux gates below.
Windows presentation code and its unit tests type-check in an isolated
probe; Windows terminal execution remains unverified. See the evidence
file beside this record. The
thirty-ninth record of rnx, and the first about how a session looks
rather than what it does. It gives the session colour — on what a person
types, on what rnx prints back, on where an error is — under one rule
that is older than this record: what rnx *presents* is escaped so
nothing in it moves the cursor, and now what rnx presents is the only
thing that carries colour. Colour is rnx's, attached to what rnx renders,
and under the default switch a pipe receives none of it.

An earlier draft was titled "colour that never reaches a pipe", which
`--color=always` contradicts by design; the title now says what the
record guarantees.

## Context

What makes IPython feel cared for is not decoration. It is that a session
is easier to read: input highlighted as you type shows what the parser
sees before you press enter, a result is recognisable as a result, an
error puts the one line that matters in front of your eyes, completion
shows what exists. rnx has the substance of most of that — a bounded
renderer that walks values by kind (record 0019), diagnostics that place
a caret by the escaped line's real columns (record 0009), a completion
menu and `:help` on any name (records 0003 and 0004) — and prints all of
it in the terminal's foreground colour, with the line editor's
highlighter and hinter derived as no-ops. Nothing rnx presents carries
an escape sequence today, and a test in `format.rs` proves that a
value whose key is `\x1b[2J` prints as that key's escaped spelling and
not as a screen clear.

That test is this record's foundation, and its scope is stated
precisely, because it is narrower than "everything": record 0009's rule
covers what rnx **presents** — a rendered value, a diagnostic, a prompt,
help — and not what a script **writes**. Measured: `println!("\x1b[2J")`
puts bytes `1b 5b 32 4a 0a` on standard output, and `host::eprint` of an
escape puts it raw on standard error, because a script's output is the
script's and rnx does not rewrite it. This record leaves that alone;
changing it would be a breaking change to script I/O that has nothing
to do with colour. Colour is escape sequences too, so the distinction
has to be structural: rnx escapes a script's text where it presents it,
and then attaches its own styling to the spans it rendered. This record
is that structure, and the gates that keep it.

Three facts found on 2026-09-14 shape the design:

- **Rune's lexer is not available for highlighting.** `rune::parse::Lexer`
  exists but its `next` is crate-private, and it returns an error on an
  unterminated string or character literal. Highlighting has to work on
  every keystroke of an unfinished line, so it needs a tokenizer that
  never fails; rnx writes a small one.
- **The line editor gates its own colour, on standard output.** rnx
  configures rustyline with its default behaviour, standard input and
  standard output, not a separate terminal device; that is an
  interaction decision this record does not reopen. rustyline applies a
  highlighter only when its colour mode allows it: `Enabled` means "if
  standard output is a terminal", `Forced` and `Disabled` mean what they
  say. Which reader runs is decided by **standard input**: a terminal
  there gets the editor, whatever standard output is — measured by
  Codex: terminal input with piped output still runs the editor, and the
  pipe receives the prompt and the redraw sequences — while a `TERM` of
  `dumb` or a non-terminal standard input reads lines directly with no
  editor at all. The input side therefore has a switch rnx must drive,
  and a reader selection rnx leaves alone; changing which reader runs
  would be a separate interaction decision.
- **Results and diagnostics go to different streams.** The session
  prints values to standard output and failures to standard error, and
  either can be redirected without the other. Colour has to be decided
  per stream.

## Decision

### 1. Three switches, one flag, one convention

Colour is decided once per process, per stream, at startup:

```
rnx --color=auto|always|never <command> ...
```

- `auto`, the default: colour on a stream if and only if that stream is
  a terminal, `TERM` is not `dumb`, and `NO_COLOR` is unset or empty.
- `always`: colour on both streams whatever they are — a person piping
  into a pager that understands colour has asked for this.
- `never`: none.

The flag is accepted **only before the command word**. After the script
path it is an ordinary argument, as every argument after the path
already is (record 0009): `rnx run f.rn --color=never` runs `f.rn` with
`--color=never` in its `env::args()`. In the runner's own flag position,
between `run` and the path, it is not a flag of the runner's and keeps
today's behaviour, measured: `rnx run --color=never f.rn` takes
`--color=never` as the path and refuses it as unreadable, exit 1, as it
does for any unknown word there. Gate 3 has both cases apart.

The same three-way answer drives the line editor's colour mode —
`Enabled`, `Forced`, `Disabled` — so a session's prompt and input are
coloured under the same rule as its output, with rustyline's own check
on standard output deciding `Enabled`. What follows from the editor's
behaviour is stated, not changed. With a terminal on standard input and
standard output redirected, the editor still runs and its prompt and
redraws go to the pipe, as they do today: under `auto` its colour is
off, because standard output is not a terminal; under `always` its
highlighting is on, into the pipe, because that is what `always` asked
for. Under `TERM` of `dumb` or a non-terminal standard input there is no
editor and so no highlighting even under `always`, while results and
diagnostics are still styled under `always`, because their styling is
rnx's. Standard output and standard error are checked separately, and
gate 4 has these cases.

`NO_COLOR` is the convention every tool people like respects, and
"nonempty" is what it specifies; `always` overrides it because a flag
typed now outranks an environment variable set once.

### 2. Text first, styling second, and the two never mix

Every presentation surface — a rendered value, a diagnostic, a prompt,
help — keeps the pipeline it has: a script's text is escaped by record
0009's rule, truncated by record 0019's bounds, and its width measured
on the escaped result. This record adds one thing
after all of that: the renderer and the diagnostic printer emit
**semantic spans** — this run of characters is a string literal, this is
a number, this is a key, this is the caret line, this is the error
message — and a styler turns spans into the terminal's escape sequences
when the stream's switch is on, and into nothing when it is off.

So styling cannot change what is visible: the bytes that carry the
text are the same in both modes, the escape sequences are added around
them, and a caret placed at column 17 of the plain line is at column 17
of the styled line, because the escapes have no width and are never
placed inside the escaped text. The proof is gate 2, and it is about
**presentation** — rendered values, diagnostics, help — read through
pipes under `always`, not about a terminal transcript: removing only
the styling sequences this record permits gives byte-for-byte the
plain output. Terminal transcripts carry the editor's own cursor
movement and redraws, which highlighting can change, so they are judged
differently, by the screen they leave (gate 6). And the old test stands,
extended: a key of `\x1b[2J` prints as its escaped spelling inside a
coloured key, and the only escape sequences in a presented value are
the ones rnx's styler put there.

### 3. The palette is the terminal's, and restrained

The sixteen-colour ANSI palette, nothing else: the terminal's own
choices for those colours are what a person has already tuned to their
background, and a hardcoded 24-bit palette looks wrong on half of all
screens. Codex's brief for this record put it well and it is adopted as
written: ordinary punctuation stays in the foreground colour, no
backgrounds, and nothing essential is dimmed.

| what | style |
| --- | --- |
| prompt `rnx> ` | bold |
| keyword (`let`, `fn`, `if`, `match`, `async`, `await`, …) | one accent colour |
| string and character literal, template | a second colour |
| number | a third colour |
| comment | dim |
| object key in a rendered value | bold |
| rendered string | the literal's colour |
| rendered number, bool, unit, `None`/`Some` | the number's colour |
| error message | red, bold |
| the caret and its line under a diagnostic | red |
| source excerpt in a diagnostic | foreground |
| `<cycle>`, `<function>`, opaque markers | foreground, not dimmed — a marker can be the whole explanation of a result |

Which three accents is decided by looking, not by argument: the
evidence carries the specimen decision 6 describes, viewed on a dark and
a light background, and the colours are whatever read well on both.
Separate themes are not in this record; the terminal's palette is the
theme.

### 4. Input highlighting never fails and never runs anything

A tokenizer of rnx's own, over the line as typed, classifying keywords,
identifiers, numbers, string and character literals including
**unterminated** ones, templates, comments including unterminated block
comments, and punctuation. It is lexical only: it does not compile, does
not consult the session's declarations, and cannot execute anything.

The input contract is different from the presentation contract and is
stated on its own: highlighting adds styles **around the original input
bytes** and changes nothing else. It does not apply the value renderer's
escaping, and it does not truncate, because the editor computes the
cursor's position from the line it was given, and a highlighter that
rewrote the text would put the cursor somewhere else. A control
character a person pastes into the line is the editor's to display as
it does today; the highlighter colours around it.

Identifier boundaries use `unicode-ident`, the same Unicode tables Rune
uses and an existing resolved dependency; adding it directly introduces no
new package or feature. The keyword list follows Rune 0.14.2.

An unfinished line is the normal case — a person is in the middle of
typing it — and it is highlighted as far as it goes, with an open string
coloured as a string to its end. The tokenizer is bounded by the line's
length and does no allocation proportional to anything else; gate 5
measures it on a pasted block.

rustyline asks the highlighter, on each edit, whether the line should
be re-highlighted. rnx answers **yes to every edit** in this record:
typing `le` then `t` turns an identifier into a keyword with no quote,
slash or boundary involved, and a deletion inside a literal changes
what is open, so any heuristic that skips edits is wrong somewhere. A
lexical pass over one line is expected to be cheap; gate 5 measures it
on a pasted block, and only if the number says otherwise does a later
change add a heuristic, with the same gates to prove it misses nothing.

### 5. What changes on a terminal, and what does not change anywhere

Changes, on a terminal with colour on: the prompt is bold, input is
highlighted as it is typed, a rendered value is coloured by kind, a
diagnostic's message is red and bold and its caret line red, and the
completion menu's candidates are styled as the identifiers they are.

Does not change anywhere: the text of anything. The transcript format —
what goes on which line, in which order, with which spelling — is
untouched, so every existing test, all of which read rnx through pipes,
passes with no edits, and that is gate 1. Numbered prompts and an `Out`
marker, which IPython has and which change the transcript, are a
separate decision for a later record, because they alter what a pipe
sees and this record preserves plain pipe output under `auto`.

### 6. The evidence is a picture as well as a number

Gate 5's numbers say the highlighting is not felt. Whether rnx now looks
cared for is not a number, so the evidence includes a **specimen**: one
session transcript, captured through the terminal harness with colour
on, containing an HTTP result object, a nested JSON value parsed back, a
string with non-ASCII characters and an escaped control character, a
multi-line input, a diagnostic with its caret, and a completion menu —
rendered as text with the escape sequences shown, and as an image of the
same session on a dark background and on a light one. The colours in
decision 3 are chosen from that specimen, and the record says what was
seen.

### 7. Windows

Modern Windows consoles render ANSI sequences once virtual terminal
processing is enabled on the output handle, one call per stream at
startup when that stream is a console. Record 0025 owns that console
and its harness; the call is added there, authored and unexecuted like
every Windows claim since, and `auto` on Windows treats a console with
that mode refused as not a terminal for colour's purposes.

### 8. What this record does not decide

- Numbered prompts, an output marker, or any change to the transcript.
- Themes beyond the terminal's palette, or configuration of colours.
- Pretty-printing with layout-driven nesting, and a pager.
- Colour in `run`'s and `eval`'s diagnostics beyond what falls out of
  the same printer — they get it under the same switch, and the record
  states that rather than designing for it.

## Acceptance gates

1. **A pipe sees nothing new.** Both suites pass with no test edited.
   Every existing test reads through a pipe, and `auto` gives a pipe no
   colour, so this gate is the whole existing suite.
2. **Presentation is separable and width-free.** Through pipes, for
   `eval` and `run` and a piped session, under `--color=always` against
   `--color=never`: removing only the permitted styling sequences —
   listed in the evidence, and nothing else removed — gives exact
   plain-text equality; the caret under a diagnostic is at the same
   column in both; a value truncated by record 0019's bounds is
   truncated at the same character in both; a key of `\x1b[2J` and a
   string containing a tab, a combining character and a double-width
   character render as their escaped spellings inside their styled
   spans, and the only escape sequences in a presented value are rnx's
   own, asserted by parsing them. A script's own `println!` of an
   escape is unchanged in both modes, asserted, because it is not
   presentation.
3. **The switches.** `auto` on a terminal colours; `auto` in a pipe does
   not; `TERM=dumb` does not; `NO_COLOR=1` does not and `NO_COLOR=` (empty)
   does; `always` colours a pipe; `never` colours nothing on a terminal;
   `rnx run f.rn --color=never` delivers `--color=never` to the script's
   `env::args()`; `rnx run --color=never f.rn` keeps today's behaviour,
   taking it as the path and refusing it as unreadable; `--color=purple`
   before the command word is refused before anything runs, naming the
   three values.
4. **Per stream, and the editor's behaviour preserved.** With a
   terminal on standard input, standard output a pipe and standard
   error a terminal: a diagnostic is coloured, a result is not, the
   editor still runs and its prompt reaches the pipe uncoloured under
   `auto` and highlighted under `always`, exactly as its redraws reach
   the pipe today; the reverse redirection gives the reverse; under
   `TERM=dumb --color=always` results are styled and there is no editor,
   so no highlighting; with standard input a pipe there is no editor
   either.
5. **Highlighting is not felt and cannot fail.** The tokenizer over a
   10 000-character pasted block completes under a stated bound in the
   evidence, single-digit milliseconds; over every prefix of that block,
   including one ending inside a string, inside a block comment, and
   after a lone backslash, it returns without error and colours the
   open literal to the end; through the terminal harness, typing `le`
   then `t` recolours the word as a keyword, deleting the `t` recolours
   it back, deleting a closing quote reopens the string to the line's
   end, and pasting the block highlights it in one redraw; the
   highlighter runs nothing, asserted by a line that would print if
   evaluated printing nothing.
6. **Terminal tests judge the screen, not the bytes.** Through the
   terminal harness on each platform, the assertion is the terminal's
   resulting screen contents and cursor position, with the raw capture
   kept for diagnosis and not compared: a multi-line input with a
   continuation prompt; a line containing a tab, a combining character
   and a double-width character, with the cursor where the editor puts
   it; a reset at the end of every styled span so the attributes after
   rnx's last byte are the terminal's defaults; a diagnostic with the
   caret under the right column of a line containing non-ASCII text;
   and `Ctrl-C` on a highlighted partial line leaving default
   attributes.
7. **Startup.** `version`, `help`, `eval 42`, the bare run and the JSON
   workload measured before and after. The terminal checks are expected
   to be an `isatty` per stream and an environment read or two; the
   measurement, not this sentence, says what they cost.
8. **The specimen.** Captured, shown on both backgrounds, and the
   record's colour choices recorded against what was seen.

## Guardrails and stop conditions

1. In what rnx presents — rendered values, diagnostics, prompts, help —
   no styling sequence ever encloses a script's unescaped bytes; styling
   wraps escaped text only. A script's own output through `println!`,
   `print!` and `host::eprint` is not presentation, is not escaped, and
   is not styled; this record does not touch it.
2. A pipe under `auto` receives no styling sequence from rnx. If any
   test has to change to pass, stop: the transcript changed.
3. The highlighter is lexical, allocation-bounded, never fails, and
   never evaluates. If it needs the compiler, stop.
4. Sixteen colours, no backgrounds, no dimming of essential text.

## Risks

- **A terminal that lies about `TERM`.** A terminal claiming not to be
  dumb but ignoring colour shows raw escapes; `--color=never` exists for
  it, and `NO_COLOR` for the person.
- **The specimen is a judgment.** Two people may disagree about the
  accents; the record records what was chosen and why, and changing a
  colour is a one-line edit with the specimen to check it against.
- **rustyline's terminal check and rnx's are two checks.** They agree
  whenever standard output is the terminal. When a session's standard
  output is redirected with a terminal on standard input, the editor
  still runs and writes its prompt and redraws into the pipe, as it does
  today; this record does not change that reader selection, only
  whether colour rides along. Stated, and gate 4 has it.

## Forward

Numbered prompts and an output marker as their own transcript decision,
carrying one request already made for it: the numbering must be
resettable on its own, without clearing the session's bindings, because
a person who restarts and pivots every few minutes resets the count far
more often than the memory — a `:renumber` beside `:reset`, or `:reset`
doing both with the lighter command doing only the count. Then
pretty-printing with layout, if the specimen shows nesting is the next
thing that is hard to read.
