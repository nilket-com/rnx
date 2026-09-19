# rnx 0068: interactive frames show a bounded preview

Status: accepted for the isolated gate 1 prototype after draft review. Gate 1
must settle the native presentation registration seam before any product public
API or generated wrapper changes. 0067 is closed on Linux. Its clang/mold
follow-up is rnx `1f497de`, bench `3d2a2f9`.
Gate 1 continues after the accepted source-compatibility stop in the exploratory
generic builder return type; see [the registration evidence](0068_registration_stop_evidence.md).
Presentation is now a separate capability, independent of the lifecycle hook,
as specified in decision 2. The replacement prototype and schema vectors remain
to be demonstrated; gate 1 has not passed.

## Problem and user journey

On slim, the user successfully installed rnx, consented to `:dep polars`, built
with their existing clang/mold configuration and reached the replacement
session. Reading sales.csv worked. Discovering how to see the frame did not:

- `sales` displayed `<::polars::DataFrame>`.
- `format!("{sales}")` failed because DISPLAY_FMT was missing.
- `sales.preview()?` displayed an escaped Rune string.
- Only `println!("{}", sales.preview()?)` displayed the readable table.

The intended result is that typing `sales` at the prompt, or returning it as a
notebook cell's final value, shows the existing bounded table. Explicit
`format!("{sales}")` and `println!("{sales}")` should also use that preview.
The explicit `preview()` method remains available and returns an ordinary
string; ordinary string rendering must not change to special-case its contents.

## Current boundaries, checked in source

`src/format.rs` walks values directly, invokes no VM formatting protocols, and
has a test proving a registered DEBUG_FMT is not called by rendering. The REPL
uses `render_styled`; the worker uses `format::render` for `text_plain`.
`src/inspect.rs` also promises no protocol execution. Therefore adding
DISPLAY_FMT alone cannot make a bare frame render, and dispatching arbitrary
formatting protocols would change an existing safety and side-effect contract.

`adapters/polars/src/preview.rs` already limits work structurally: ten rows,
eight columns, eighty scalars per name/cell, and 8192 output bytes with omission
markers. It escapes controls and directional characters. It inspects an already
materialized frame, without collecting a lazy query or formatting a whole frame.

The extension builder currently receives a Rune Module and returns help entries.
It has no native presentation registry. The 0051 public entry and 0057 generated
wrappers are real compatibility boundaries, not incidental implementation details.

## Decisions

### 1. DataFrame first, with no implicit query execution

Provide automatic presentation only for a top-level, successfully evaluated
Polars DataFrame in the REPL and notebook worker. A semicolon or unit result
continues to suppress the result. A frame inside a vector, tuple, Result or
other container retains existing rendering in this first record. `:vars`, error
formatting, JSON output, run/eval output and other inspection paths retain their
current policies.

LazyFrame, LazyGroupBy and Expr remain opaque. Do not collect, explain, optimize
or recursively format plans merely because the user returned a value. A later
record may specify bounded summaries for those types. No HTML display or rich
notebook MIME bundle is added here; the notebook result is text/plain.

### 2. Explicit native presentation, separate from VM formatting

Introduce an opt-in native presentation registration owned by the assembled
execution context. Match the concrete native type identity, not its printed
name. No process-global registry, no module-name special case for Polars, no
script-defined formatter, and no new root dependency on Polars.

A presenter borrows its value and receives an output budget. It must use a
bounded reader and writer, not build an unbounded string and truncate it. The
Polars presenter shares the existing preview implementation. Limit accounting
must reserve any omission marker and respect UTF-8 boundaries. The automatic
path never emits raw terminal controls from column names, data, or an error.

Native callbacks are trusted Rust, like existing extension builders. The API
cannot make an arbitrary native callback preemptible or guarantee its work;
document that obligation and prove it for the shipped presenter. Output-byte
checks alone are not a work bound. Preserve the generic renderer's no-protocol
contract rather than silently redefining it.

Keep `Extensions::with` and `with_lifecycle` byte-for-byte unchanged. Add a
separate opt-in entry, provisionally `Extensions::present(name, registrar_fn)`,
alongside the existing builder call. The adapter exports a second function,
provisionally `rnx_polars::present`. Registration is lazy: adding the capability
to Extensions does not run it. Invoke it during serving-context construction,
after its named module builder succeeds, and retain the registry with that
context. It must be equally usable with plain and lifecycle builders. Reject
missing module associations and duplicate type registrations with named errors.
A registration failure follows context construction's existing failure policy;
it must not leave a usable partially registered context.

For declarations, add optional `presentation = true`, independent of `hook`.
Absence means false. The generated wrapper keeps the existing `.with(...)` or
`.with_lifecycle(...)` call and, when requested, emits a separate
`.present(name, native_alias::present)` call. A boolean capability uses the
adapter's conventional exported `present` function; it does not overload the
builder name or introduce presentation/lifecycle_presentation hook variants.
The catalogue enables this for newly authored Polars declarations. It does not
rewrite existing declarations or reinterpret old locks. An existing project
opts in by editing its declaration and relocking/building; unchanged manifests
retain their existing generated wrapper.

The readers deny unknown fields: old tools refuse a declaration carrying
`presentation`, rather than ignoring it. Gate 1 must fix the declaration-format
choice with vectors for both path and Git declarations: omitted, false and true,
invalid types, unknown fields, and independent plain/lifecycle combinations.
Prefer omission of false when serializing so legacy declarations and envelopes
retain their bytes. Demonstrate this rather than assuming it. Decide whether a
format bump is necessary from the real readers, including retained lock readers;
record any required migration before porting the product. An old-reader refusal
must identify the unsupported field or format and the required newer tool.

Enabling presentation changes the generated wrapper bytes and therefore the
assembly key: an opted-in Polars project relocks and builds a new assembly once.
It cannot attach to the old plain artifact. Keep every other assembly identity
input and verification policy intact. Do not infer a global identity-version
bump merely from an optional new declaration; prove unchanged-input compatibility
and changed-input separation with the actual generator and identity code.

Gate 1 must demonstrate this seam through a real generated application and a
runner-only embedding consumer. The failure-only and panic-only builders from
the stop must compile without annotations. A runner-only consumer does not opt
into registration unless it calls the new API; prove no new dependency or startup
initialization cost. No global state or type-name guessing. Any further public
API, generator-version or migration change beyond this capability needs a
concrete reviewed decision before the product port.

### 3. Explicit formatting uses the same preview

Register DISPLAY_FMT for DataFrame so explicit Rune formatting can request the
same bounded preview. Use Rune's formatter error contract and preserve existing
native panic handling; do not claim a formatter error is a catchable String
without demonstrating it. Automatic presentation must not invoke this protocol
through the VM. DEBUG_FMT is not required for this first surface.

The plain table content should match explicit preview. Terminal result numbers,
colour and notebook framing are presentation outside that content. A normal
preview string still prints quoted and escaped as a Rune string.

### 4. Rendering failure does not erase a successful evaluation

A borrow or preview error on the automatic path yields the existing opaque type
label plus a bounded, terminal-safe indication that preview was unavailable.
It does not turn successful evaluation into a compile/runtime error, repeat the
cell, or advance the input number a second time. The frame and prior bindings
remain usable. Do not blanket-catch native panics or conceal a lifecycle failure
as a display error.

Registry ownership follows the context. Reset drops bindings but preserves the
registered native capability, as it preserves Polars today. A kernel restart
rebuilds it once; shutdown and :dep handover release the old context and its
registrations. Settings evaluation receives no extensions or presenters.

### 5. Scope and costs

The product goal is the user's direct interaction, not faster test execution.
Measure a returned frame's presentation separately from collecting it. Bounded
presentation must not scan the full row count or long cell contents; use the
existing preview's structural limits as the oracle.

Recursive presentation of frames inside containers, including a top-level
`Result<DataFrame>` (which still renders as `Ok(<::polars::DataFrame>)`), is later
work. No automatic value-formatting changes for other types. No mapped notebook
sources, dependency transition changes, engine-thread changes, catalogue changes
or build-verification optimizations. Launcher/runtime skew, Windows and crate
publication remain separate work.

The slim transcript shows a `--git` install of 1f497de classified unverified;
that classification question is tracked separately from this record.

## Gates

### Gate 1 — registration and ownership prototype

Use isolated source with a recoverable patch. Demonstrate the presentation seam
with a tiny native type and then DataFrame through the generated application.
Prove type identity matching, context ownership, unchanged existing builders
(including the exact error-only and panic-only closures that exposed the stop),
runner-only dependency isolation, and no VM formatting protocol calls on the
automatic path. Include a lifecycle builder with a presenter to prove the two
capabilities compose independently. Run the declaration and retained-envelope
vectors from decision 2 through real readers, then compare generated wrapper
bytes and assembly identities for omitted, false and true presentation. A side-effecting DEBUG_FMT/DISPLAY_FMT counter remains zero for
automatic results and increments only for explicit formatting. Show no callback
for unrelated types, strings, nested frames, :vars or settings. Prove real Polars registration and explicit DISPLAY_FMT, escaping and limits,
registry survival across reset, retirement through handover and context teardown,
and the worker path. Settle the declaration format and any remaining API or
wrapper/identity change here before implementation, with a reviewable decision.

### Gate 2 — bounded DataFrame presentation and explicit formatting

Reuse the preview oracle: empty, narrow, wide and tall frames, nulls, long UTF-8
cells, controls, directional characters, float extremes and the byte limit.
Measure inspected cells/scalars and output bytes, not only final string length.
Borrow failures and preview failures preserve bindings and produce the stated
fallback. Repeated rendering is deterministic and leaves the frame unchanged.
A lazy plan with a missing column remains unexecuted when displayed. Explicit
format!/println! succeed for a DataFrame and preserve the preview limits.

### Gate 3 — real interactive and notebook journey

Pin terminal type and width. Build a project with Polars, open its session, read
sales.csv, type `sales`, filter into another binding, return it, recover from a
query error, and return the original frame again. Also type a frame wider than eight columns and taller than ten rows bare
to see the omission markers at the prompt. Prove numbering, suppression with
`let x = sales;`, reset, history and quit. Repeat in a real notebook with a cell
returning a bare frame, an error, the retained original frame, restart, and a
new frame. Validate the saved notebook and reap all workers/kernels. No global
kernelspec or user files are changed by fixtures.

### Gate 4 — costs, docs and regression

Compare the published baseline and candidate on the same machine/toolchain with
interleaved samples. Stock version and eval retain the existing 5% startup gate;
report absolute times and spread as well. For Polars, report small-frame render,
large-frame bounded render, explicit preview, and spawn-to-first-prompt costs.
Separate collect from rendering. No requirement to beat explicit preview, but
unexplained work proportional to full frame size is a stop.

Run the three root configurations serially, adapter tests and strict clippy in
both configurations, root/adapter notices, management regression where the seam
affects generated applications, and runner-only/server feature audits. Preserve
the existing tests that ordinary rendering invokes no protocol. Update the
Polars walkthrough to use bare frames, while keeping explicit preview as a
portable string-producing method. Record qualifications and review before push.
