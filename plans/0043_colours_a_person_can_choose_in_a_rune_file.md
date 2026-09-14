# rnx 0043: colours a person can choose, in a Rune file

Status: proposed 2026-09-14; revised the same day after review. The
forty-third record of rnx. A person ran
the session on Windows and on Linux and found the colours beautiful on one
and bland on the other, and asked for two things that pull against each
other: control over the colours, and the same colours everywhere. This
record gives both without breaking the reason record 0039 chose the
terminal's palette, and it does it through a `.rn` config file, because
rnx already runs Rune and a settings file that is a Rune value is the
slickest thing rnx can offer.

## Context

Record 0039 styles with the sixteen-colour ANSI palette on purpose: those
colours are the ones a person has already tuned to their background, so
rnx looks native everywhere and never fights the terminal. Measured in
use, that has a cost the record accepted without seeing: the *same* SGR
codes render vividly under Windows Terminal's default scheme and flatly
under one person's kitty theme, so rnx looks cared-for on one machine and
plain on another, through no fault of rnx's — the terminal's palette is
doing exactly what record 0039 asked.

So "consistent across platforms" and "use the terminal's palette" cannot
both be the default; they are opposite choices. The resolution is not to
pick one for everyone but to keep the portable default and let a person
who wants sameness ask for it, in a place that also answers "give me
control over the colours" and "colour the `[n]` like IPython" at the same
time. That place is a config file, and its format is Rune.

## Decision

### 1. The default does not change

With no config and no flags, rnx styles exactly as record 0039 and 0042
leave it: the sixteen-colour palette, the terminal's own choices, native
on every screen. A person who never writes a config sees no difference,
and record 0039's gates still hold — with **one exception, stated in
every place it matters**: the prompt number `[n]` moves from cyan to
green (decision 6), the one default this record changes because a person
named it. "Default unchanged" below always means "unchanged but for the
green `[n]`". Everything else here is opt-in.

### 2. `config.rn` is a Rune script that evaluates to a settings object

rnx reads one file, `config.rn`. Its location:

- `RNX_CONFIG` if set and non-empty — an absolute path to the file
  itself, not a directory; a relative or non-Unicode `RNX_CONFIG` is a
  warning and defaults;
- the config file itself is read with rnx's regular-file rule (record
  0035): the file is opened non-blocking on Unix and checked on its handle;
  a FIFO or device is refused before reading, and reading is capped at
  64 KiB + 1 byte to detect an oversize file without reading it whole.
  The one stall this cannot prevent is a hung filesystem under the config
  path itself, which is the excluded OS-stall case above, not something a
  colour setting invents;
- else `<config-dir>/rnx/config.rn`, where the config directory is
  `XDG_CONFIG_HOME` if it is set to an **absolute** path (the XDG spec
  says a relative one is ignored), else `~/.config`; on Windows,
  `APPDATA` if set, else `LOCALAPPDATA`, else nowhere;
- if neither yields a path, or the file is **missing**, there is no
  config and no warning — the absence of a config is the ordinary case
  and silent. An **unreadable** file (it exists, permission denied) is a
  warning and defaults. An **empty** file evaluates to unit, which is
  not an object, and is treated as no settings — defaults, no warning,
  because an empty config is a person's blank canvas, not a mistake.

Empty directory overrides count as absent; non-Unicode directory overrides
warn. Relative XDG_CONFIG_HOME is ignored; a relative HOME, APPDATA or
LOCALAPPDATA warns rather than resolving against the working directory. Unix
home fallback uses HOME only, without an account-database lookup. Whitespace-only
files are silent; explicit unit and other non-object results warn. Warnings are
emitted in sorted-key order. Invalid palette entries retain defaults individually.
The error colour also colours the caret, preserving their separate attributes.

The file is a Rune script — decision 2a says exactly how it is run,
because "run the way any script is" is not safe enough — and its value is
an object:

```rune
#{
    color: "auto",              // the default --color mode
    splash: false,              // record 0042's banner, off by default
    palette: #{
        keyword: "#c678dd",
        string:  "#98c379",
        number:  "#61afef",
        comment: "bright-black",
        prompt_number: "#98c379",   // the [n], IPython green
        error:   "bright-red",
    },
}
```

Every key is optional and an absent one keeps its default. A colour is a
string: `#rrggbb` for a true colour, or one of the sixteen ANSI names
(`red`, `bright-red`, …) for a palette colour. A `#rrggbb` value sends
the **same RGB request** on every terminal that supports true colour —
which is the answer to "consistent across platforms", with the caveat
that identical RGB is not identical *appearance*, since a terminal's
contrast and profile still apply; an ANSI name is the terminal's own
colour, record 0039's default made explicit. A person who wants rnx as
alike as terminals allow on kitty and Windows Terminal writes hex; a
person who wants it native writes nothing.

A palette entry sets a role's **foreground colour only**. The attributes
record 0039 attached to roles are rnx's and stay: the error stays bold,
the comment stays dim, the prompt number and keys keep their weight, and
a config changes the hue under them, not the weight. An attribute grammar
(`bold red`, `dim`) is a later key if anyone asks; this record keeps the
surface to a colour per role so a config cannot accidentally un-bold an
error into invisibility.

That the config is a Rune value, not TOML or JSON, is the point a person
asked for across their other tools: the language configures itself, a
setting can be computed, and there is no second parser to learn or ship.

### 2a. The config computes without I/O, under an instruction budget

"Run the way any script is" and "never prevents reaching the prompt"
cannot both hold as written, and the review is right: an instruction
budget does not bound a blocking `fs::read`, and the allocation ceiling
is a process-wide accounting figure, not a recoverable per-evaluation
heap limit. A config that called `http::get` or `fs::read` of a slow
path would hang the prompt no budget could interrupt. So the config does
**not** run in rnx's ordinary context. It runs in a dedicated evaluator:

- a Rune `Context` built with `Context::with_config(false)`, **not**
  `with_default_modules()`. The default context is `with_config(true)`,
  which installs `::std::io::print`, `println` and `dbg` — writes to
  standard output and error. `with_config(false)` leaves those out, and
  none of rnx's host modules (`fs`, `http`, `env`, `time`, `process`,
  `host`) is installed either. So the config **cannot** open a file, make
  a request, sleep, or even print: it computes a value and returns it,
  and has no way to reach a stream or the disk.
- the source is the body of `pub fn main() { ... }`; helper functions may
  be declared inside it. No session bindings or declarations are injected.
- no async execution, so no runtime and no top-level `.await`; the evaluator is synchronous
  and record 0032's driver is not involved.
- one in-memory source and **no file-based module loader** on the
  compiler, so a `mod foo;` that would load `foo.rn` from disk is a
  compile error, not a quiet filesystem read; the config is one file and
  the compiler is given no way to reach a second.
- a source-size cap (64 KiB) applied to the bytes read, refused before
  compiling, so a pathological file cannot cost unbounded parse time.
- a fixed small instruction budget (100 000), separate from the session's
  two billion.
- the result read once into rnx's own settings and the whole evaluator
  then dropped — its `Vm`, `Unit`, and context — so no config state
  outlives the read.

What this bounds, precisely, and what it does not: the evaluator
**recovers from** a parse error, a compile error, a value of the wrong
shape, and a spent instruction budget — the failure classes a config's
*content* can cause — and starts with defaults. It does **not** claim to
bound wall-clock time inside a native function or process-wide memory:
an instruction budget counts VM instructions, not time spent in a native
call or bytes allocated there, and the allocation ceiling is a
process-wide figure, not a recoverable per-evaluation heap limit. With
every I/O function removed those vectors are out of a config's reach, so
in practice a config computes and returns or trips the budget; the honest
promise is recovery from the content failures above, with OS stalls and
process-wide resource exhaustion excluded rather than pretended away.

A computed setting still works: `#{ palette: #{ number: if dark() { "#61afef" } else
{ "#005faf" } } }` is fine as long as `dark()` is defined in the file and
computes without I/O. What is gone is the ability to read the environment
or a file to decide, which is the price of a config that cannot reach the
disk, and it is the right price.

### 3. A broken config warns per setting and never locks the prompt

Each setting is validated on its own. An **absent** key keeps its
default **silently** — that is what optional means. A key that is
**present** but unknown, of the wrong type, or a colour rnx cannot read
keeps its default and adds one warning naming that key. A file that does
not parse, throws before
producing a value, exhausts the config budget, exceeds the size cap, or
evaluates to something that is not an object yields no settings at all,
one warning, and every default. So the rule is one rule at two levels: a
setting is taken when it is valid and defaulted with a warning when it is
not, and a file with no valid object is the case where zero settings are
valid. A valid setting beside an invalid one in the same file keeps the
valid one and warns about the other — gate 3 holds exactly that. The listed content failures do not refuse the prompt (the OS-stall and
resource-exhaustion exclusions in decision 2a still apply), and a typo in a colour is the last thing that should stand between
them and it. An unknown key warns rather than errors so a config written
for a newer rnx degrades on an older one instead of breaking.

### 4. Precedence: a flag beats the config beats the default

`--color=never` on the command line wins over `color: "auto"` in the
config wins over the built-in `auto`; `NO_COLOR` is still honoured by
`auto` wherever `auto` ends up in force. `--no-splash` wins over
`splash: true`. A flag is this moment's intent and outranks a saved one;
the config outranks the built-in. Nothing surprising, stated so the order
is a decision and not an accident.

### 5. When the config is read, and when it is not

The config is read exactly where colour is emitted and the context is
built: `run`, `eval`, and the session. It is **not** read by `version` or
`help`, which record 0030 answers before building anything and which this
record keeps free of a file read, so the fast path stays fast. Reading
and evaluating a small config is a cost the context-building paths already
resemble; gate 6 measures it and holds `version`/`help` unchanged.

### 6. The `[n]` default becomes IPython's green

With no config, the prompt number and the matching result marker that
record 0042 left in the cyan number colour become green — `In [n]` is
green in IPython and a person said so — and everything else keeps record
0039's accents. This is the one default this record moves, the exception
to "default unchanged" named wherever that phrase appears, and a config
overrides it like any other role.

### 7. What this record does not decide

- Configuring anything but colour, the splash, and the default colour
  mode — not the prompt glyph, not key bindings, not the ceiling. Each is
  a later key with its own decision, and the object grows by record.
- A `256`-colour indexed mode; `#rrggbb` and the sixteen names are the
  two a person needs.
- Per-project config, or a config that layers over another. One file.
- Reloading the config without restarting.

## Acceptance gates

1. **No config, no change.** Every gate sets `RNX_CONFIG` or clears the
   config environment so nothing depends on the developer's home. With no
   `config.rn` and the config environment cleared, every record 0039 and
   0042 gate still passes and the specimen is byte-identical but for
   decision 6's green `[n]`; a missing file and an empty file are both
   silent; an unreadable file warns.
2. **Hex is an RGB request, names are the terminal's.** `keyword:
   "#c678dd"` emits `\x1b[38;2;198;120;221m` on both platforms — the same
   bytes, not a claim about appearance; `keyword: "bright-magenta"` emits
   the sixteen-colour SGR; both strip to the same plain text; a role's
   attribute is unchanged, so `error: "#00ff00"` is still bold; an unknown
   colour name warns and that role keeps its default.
3. **A broken config still starts, and a good setting beside a bad one
   survives.** A config with one valid colour and one malformed colour
   applies the valid one and warns about the other; a file that fails to
   parse, throws, exhausts the config budget, exceeds the size cap, or
   evaluates to a non-object produces one warning and all defaults; none
   exits non-zero from the config alone; the pure evaluator fails a
   config that calls `fs::read`, `http::get`, or `println` with an
   unknown-function compile error, warns, and defaults, proving both the
   I/O ban and that the config cannot write to a stream.
4. **Precedence.** `--color=never` beats `color: "always"`; `color:
   "always"` beats the default on a pipe; `NO_COLOR` disables `auto`
   however `auto` was chosen; `--no-splash` beats `splash: true` and
   `splash: false` suppresses the banner with no flag.
5. **Read where colour is, not where speed is.** `run` and `eval` apply
   the config's palette to their diagnostics; `version` and `help` never
   open the file — established by source review and by a `test-support`
   read counter that stays zero across `version` and `help` and is
   non-zero after a session, not by a throwing config, which cannot prove
   a file was never opened; `RNX_CONFIG` pointing at a fixture is
   honoured.
6. **Cost.** `version`, `help`, `eval 42`, the bare run and the JSON
   workload before and after; the config read's cost appears on `eval`
   and the session and not on `version`/`help`; a session with a
   six-colour config starts within a stated bound of one without.
7. **Existing guarantees.** Both suites, and record 0039's strip-to-plain
   equality holds with a truecolor palette in force.

## Guardrails and stop conditions

1. A missing config, or the config environment cleared, is exactly
   today's behaviour but for the green `[n]`. If the absence of a config
   changes anything else, stop.
2. Parse, compile, shape and budget failures warn and continue. OS stalls
   and process-wide resource exhaustion are excluded as in decision 2a.
3. `version` and `help` never open the config. If the fast path grows a
   file read, stop.
4. The default palette is unchanged but for decision 6. If a default
   colour moves without the record saying so, stop.

## Risks

- **The config runs Rune, so it runs code at startup.** It is the
  person's own file, run as they are, like a shell's rc file — but in the
  pure evaluator of decision 2a, with no I/O function reachable, a size
  cap, and its own instruction budget, so its code computes without I/O; native-call time and process-wide
  resource exhaustion remain outside the guarantee. Not a general sandbox, and not claimed to
  be one; a bound on this one evaluation.
- **True colour on a sixteen-colour terminal.** A real terminal that
  cannot do `\x1b[38;2` shows the nearest it can or ignores it; the
  person chose hex, and the sixteen names are there for them if it looks
  wrong. Stated beside the palette in the README.
- **A config for a newer rnx on an older binary.** An unknown key warns
  and is ignored, so a shared config degrades rather than breaks.

## Forward

More keys as people reach for them — the prompt, the ceiling, a startup
script — each its own decision, the object growing one record at a time.
