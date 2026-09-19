# 0068 gate 1: the presentation seam

Status: the isolated prototype passes on Linux, ready for review before the
product port (gate 2). This revision answers the first review's three findings
(budget bypass in escaping and error reporting, the Polars byte cap bypassed on
the automatic path, and missing ownership evidence) and the re-review's
remaining ask (an actual presenter-bearing handover and a restarted worker);
the section "Review findings" says what changed. Gates 2–4 remain open. Production is unchanged: this
commit adds only this evidence and the plan status. The exact candidate source
is `rnx-bench/probes/frame-seam/candidate.patch` against rnx `b5664ff`
(SHA-256 `cbf59f67949a4e7446086baebdc3a32d97cc5500e35eaf7b67be60672a95f70a`,
14 files, 859 insertions, 46 deletions); `prepare.py` archives the baseline
with `git archive` and applies the patch in its own repository, never in the
product checkout. Results are in `rnx-bench/results/frame-seam-0068/`.

## The seam

`Extensions::present(name, registrar)` is a separate opt-in entry beside the
unchanged `with` and `with_lifecycle`. It is lazy: installation runs the
registrar after every module builder, only if a builder of that name was
installed, under the same panic-catching wrapper, and returns the registry to
the caller together with the help entries. `Presenters::register::<T>` stores
one presenter per Rune native type hash (`T::HASH`); a second registration for
the same type is refused naming the extension that holds it. The session owns
the registry with its context: `:reset` keeps it, dropping the session drops it.

The REPL and the notebook worker ask the session to present a top-level value
before rendering. A presenter borrows the value through `Value::borrow_ref::<T>`
and writes through `rnx::Output`, a budgeted writer that reserves its omission
marker (a shorter marker, or none, under tiny limits; a limit is never raised)
and refuses whole tokens once the budget is reached. The REPL and worker then
pass the outcome through `present::finish`, which escapes the text through the
existing terminal-safe path *under the same byte budget*, marker included, and
bounds an error to the opaque label plus `(preview unavailable: …)` with the
error text itself escaped and cut at the budget. Nothing the adapter writes
reaches the terminal unescaped or unbounded. A value whose type has no presenter, a unit, a container holding a
frame, a `Result`, and every non-native value take the existing renderer, which
still invokes no VM protocol (its existing test is unchanged and passes). A
presenter error is reported beside the opaque label with the evaluation kept.

The Polars adapter exports `present`, registering `DataFrame` only, and
registers `DISPLAY_FMT` for `DataFrame` so `format!("{frame}")` prints the same
preview. `preview::render` is refactored into a sink-based `render_into` that
both the explicit `preview()` string and the presenter use; the structural limits
(10 rows, 8 columns, 80 scalars per cell) are unchanged and the 8192-byte cap
with its omission marker is enforced inside `render_into` by a `Capped` sink
wrapper, so the automatic path gets exactly the bytes `preview()` returns (a
test renders a 10×8 frame of eighty-crab cells both ways: 7,985 identical bytes
ending in the marker). LazyFrame,
LazyGroupBy and Expr have no presenter and no display protocol.

Declarations gain `presentation: bool` with `#[serde(default,
skip_serializing_if = Not::not)]` in both the format-1 and format-2 readers; the
generator emits `.present(name, native_alias::present)` after the native's
builder call when it is true; the catalogue sets it for newly authored Polars
declarations and leaves an existing declaration's choice alone when comparing.

## What the probe proves

Eight drivers, all passing; every command log and document is retained.

**Vectors** (`vectors.json`). For format 1 and format 2, plain and lifecycle:
`presentation = false` canonicalizes to the byte-identical document of the
omitted field; `true` changes the canonical bytes; a string or integer value,
an unknown sibling field and a duplicate key refuse. The baseline tool
(`b5664ff`) refuses a declaration carrying the field before publishing a lock,
naming it: "unknown field `presentation`, expected one of `path`, `package`,
`builder`, `hook`" (format 1) and the equivalent list for format 2. No format
bump is needed: omitted and false keep every existing byte, and old readers
refuse the new field by name.

**Identity** (`identity.json`). The same path project locked by the baseline
tool and the candidate tool produces the same assembly key
`08bf3340265c7ad0…` and the same generated `main`; an explicit
`presentation = false` locks to that key too. `presentation = true` locks to
`fa2a9d9795fc3e87…`, and the only identity field that differs between the two
documents is `main` (the keys change with every candidate revision because the
path declaration hashes the candidate adapter's source), which now ends
`.with("polars", native_0::build).present("polars", native_0::present)`. The
probe's wrapper command shows the same two forms for a Git declaration.

**Consumers** (`consumers.json`). Against the candidate with default features
off, the failure-only builder, the panic-only builder and a failure-only
lifecycle builder compile without annotations. A runner-only consumer builds,
answers `eval 42` identically, has the same 200-package dependency graph as the
same consumer against the baseline, and links no registration symbol
(`Presenters::register`, `Extensions::present`); the registry-lookup symbol is
recorded (present, once) rather than asserted, because inlining decides whether
it survives as a symbol. That lookup is a `BTreeMap` probe on an empty map per top-level
result; its cost is gate 4's to measure, and no initialization runs for it.

**Ownership** (`ownership.json`). A tiny extension whose registrar and
presenters hold drop sentinels reports to an event file. In a real session:
registering happens once at start; `:reset` keeps the registry (the next frame
presents, no re-registration); `:quit` drops it exactly once. A presenter that
writes 10,000 ESC bytes and one that fails with 100,000 ESC bytes each produce
a bounded, escaped prompt result (16,618 and 16,664 bytes of terminal output
around a 16,384-byte budget; before the fix these were 60 KB and 600 KB). The
notebook worker, driven over its control pipes with the required
acknowledgements, presents a frame (`text_plain` 18 bytes), reports the same
two bounded fallbacks (16,384 and 16,383 bytes, `render_bounded` true, no
failure), leaves unrelated values and `let` results untouched, keeps the
registry across `reset` and drops it exactly once at `shutdown`. A settings
file naming the extension's function fails to compile with "Missing item
frame::Frame::new": settings evaluation sees neither the extension nor its
presenters, as `config::evaluate` receives none.

**Handover** (`handover.json`). A generated
application declares a tiny path native `frame` with `presentation = true`;
its registrar and presenter hold a drop sentinel and every event names the
process id and executable path. Opened through the management command, the
session registers once, presents a frame, and keeps its registry through two
preparation failures (`:dep nope`, refused by the catalogue; `:dep polars`
declined at the consent prompt): no drop, no second registration, and a frame
still presents. A consented `:dep polars` then shows the full retirement order
in the event file: preparation runs the replacement executable once as a
startup probe in a child process, which registers and drops its own registry
(the child's pid, the new executable; it is gone afterwards); the session
drops its registry after cleanup (its pid, the old executable); and the
exec'd replacement registers (the same pid, the new executable) — the old
registry is retired before the replacement's is constructed, in one process.
The first application built in 39.7 s and the transition took 116.3 s (the
replacement assembly rebuilt Polars from source even though the journey's
cache held a Polars artifact; whether Cargo output is shared across assemblies
is a 0061 question, not this record's). In the replacement a `frame` presents, the catalogue authored
`presentation = true` for the added Polars declaration, and a bare
`polars::read_csv(...)?` prints `DataFrame: 2 rows × 2 columns` with its rows.
`:quit` drops the replacement's registry exactly once. Then the replacement
executable is started twice as a notebook worker, in two processes with
different pids: each registers once, presents (a top-level frame; a frame
inside a vector stays opaque; a frame after `reset`), and drops exactly once
at `shutdown`; no event from one process appears in the other. A Jupyter
restart is a shutdown with `restart` set followed by a fresh kernel process
that spawns its own worker, which is the sequence shown here at the worker
level; the real notebook journey is gate 3.

**Journey** (`journey.json`). A real generated application with
`presentation = true` locks and builds (114.2 s cold Polars build) and its
session shows: a bare `sales` prints `[3] DataFrame: 5 rows × 4 columns` with
the header and rows; `format!("{sales}")` and `println!("{sales}")` print the
same table; `let again = sales;` prints nothing; `[sales]` prints
`[<::polars::DataFrame>]`; `sales.lazy().collect()` without `?` prints
`Ok(<::polars::DataFrame>)`; a lazy plan and an expression stay opaque;
`sales.preview()?` is still a quoted string; a filter result presents as a
3-row frame; a missing-column error is a value and `sales` presents again
afterwards; a 15×12 frame prints `[5 rows and 4 columns omitted by display
limits]`; `:reset` keeps the presenter and a fresh frame presents at `[1]`.

**Checks** (`checks.json`, `checks/`). In the candidate tree: root, Polars and
tool formatting pass; root strict clippy has the same 16 diagnostics as the
baseline tree (byte-compared lists), tool and Polars strict clippy pass with
test-support; suites pass serially: root default 388 (376 + 12 new presenter
tests), root test-support 431, Polars 7, tool 54 and 56.

The new presenter tests prove, with a native type carrying instrumented
`DEBUG_FMT` and `DISPLAY_FMT`: the automatic path runs neither protocol and
runs the presenter once; the generic renderer stays opaque for the same value;
explicit `format!` runs `DISPLAY_FMT` once; a value inside a vector is not
presented; `:reset` keeps the registry; a presenter for a name without a
builder, a second presenter for one type, and a panicking registrar are
installation errors. The ownership tests add: reset with the registry keeps
its sentinel alive, closing and dropping the session drop it once, a lifecycle
builder composes with a presenter, and settings evaluation runs with no
extension. The budget tests prove `Output` never exceeds a limit of 0..64
bytes with or without a marker, and `finish` bounds escaped controls, errors
and multi-byte UTF-8 to the budget.

## Review findings

F1: the terminal-safe escaping ran after the budget, so ESC-heavy output grew
six-fold (60 KB), an error was reported whole (600 KB), and `Output::new`
silently raised a limit below the marker's length. Now `finish` applies the
budget after escaping and to the error path, and `marker_for` shrinks or drops
the marker instead of raising the limit. F2: the Polars presenter wrote through
`render_into` without the 8192-byte cap that `preview()` applied to its string;
the cap now lives inside `render_into`. F3: teardown, handover, worker and
settings behaviour were asserted, not shown; the ownership tests and the
ownership driver show them, and the re-review's remaining ask — an actual
presenter-bearing handover with its retirement order, and a restarted worker
owning a fresh registry — is the handover driver above. The startup-probe
child's registration is a fact of the 0063 transition (the replacement is run
once before commitment), not a leak: its registry lives and dies in that
child. The stale header claim in `present.rs` that `Output` prevents an
unbounded intermediate string is removed; bounding work stays the trusted
callback's duty, as the struct documentation says.

## Qualifications and next gate

No product code changed in this commit; gate 2 ports the patch and adds the
product-level matrix (bounded rendering against the preview oracle, borrow and
presenter failures, escaping of controls in column names, the worker path).
The generated wrapper for an opted-in project changes bytes, so an existing
Polars project keeps its old assembly until its owner adds
`presentation = true` and relocks; the catalogue enables it only for newly
authored declarations. The old-tool refusal names the field but cannot name
the newer tool. The runner-only lookup symbol is present by design and its
cost is measured in gate 4, not asserted here. No timing, notebook or Windows
claim is made.
