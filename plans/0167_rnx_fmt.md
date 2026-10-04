# rnx 0167: source formatting with tabs

**Status:** plan, for Claude's review before implementation.

The user requests cargo-fmt-like formatting for .rn files, specifically tab
indentation and newlines between functions. Formatting belongs to this team;
linting belongs to the editor's linting component. This record formats source,
never evaluates it, changes its APIs or deploys an application.

## 1. Implementation choice and measure

Use the public formatter in pinned Rune 0.14.2 (`rune::fmt::prepare`, feature
`fmt`), without a compiler fork. Its indentation is hardcoded to four spaces
in fmt/mod.rs; there is no public indentation option. Adapt its result using
Rune token/AST spans, not global space replacement. Preserve string, byte,
character and template literal contents and comments. Inspect and report any
upstream limitation encountered rather than expanding into a new parser.

Earlier exploratory probes found that an incomplete macro can hang the
formatter or be completed by it. Strict ast::File parsing is a first check,
not a termination proof. Parsing and formatting both run in an isolated,
bounded child; a failed file is never rewritten.

Acceptance: a dense multi-function fixture becomes readable, using literal
TAB bytes for structural indentation and a blank line between adjacent
functions; the user's external site.rn is formatted with the same command.
Retain before/after, token/literal fidelity, idempotence and the existing HTTP
wire checks. Application source and content stay in their own repository,
with its formatting-only change reviewed separately. No application-specific
name or content is committed to rnx. No deploy occurs in this record.

## 2. Closed command surface

- `rnx fmt FILE...`: format explicitly named regular UTF-8 files in place.
  Accept paths after `--`; no discovery, recursion, dependency build or network.
- `rnx fmt --check FILE...`: no writes; report each file needing formatting.
  Exit 0 when all are formatted, 1 on formatting differences, 2 on input,
  syntax, worker or publication errors (errors take precedence).
- `rnx fmt --stdin`: read one UTF-8 source from stdin, formatted source only on
  stdout; diagnostics on stderr. No files and no --check in this mode.
- `rnx fmt --help`: document tabs, item spacing, limits and status codes.
  No arguments is a usage error, not an implicit whole-project rewrite.

Normal write/stdin success exits 0; failure exits 2. Duplicate options and
incompatible forms are refused before opening files. Reject duplicate input
files, including aliases to the same file identity, before formatting. Source
names in errors are bounded and escaped. Stock and feature-off binaries expose
fmt; assembled hosts inherit it through the ordinary command dispatch.
There is no session :fmt command, editor hook or public library API in this cut.

## 3. Style and fidelity

Structural indentation is one TAB per nesting level. Any genuine alignment
spaces after structural tabs are distinguished from indentation and reported
by fixtures. Convert only formatter-generated line prefixes outside protected
literal/comment spans; never strip indentation inside a multiline literal.
Line endings outside literals become LF; final source has one final newline.
Literal line endings and whitespace are preserved byte for byte.

Adjacent function items are separated by one blank line, including functions
inside modules/impls. Keep attributes and leading documentation/comments
attached to their item; insert spacing before that group, never between it
and the function. Do not sort imports, rename identifiers, change quoting,
rewrite strings or remove comments. Other layout follows Rune's formatter.

Before publication, parse the result again and compare original/result token
kinds and exact non-whitespace token bytes, including literal payloads; compare
comment contents/order separately. No semantic rewrites are accepted. If
upstream adds optional trailing commas, a normalization may remove only those
new comma tokens proven absent from the original token stream, retaining all
original tokens in order. Document and control this exact exception before
using it; other inserted/deleted tokens cause a named refusal. Do not excuse
an automatically inserted closing delimiter. Run the formatter pipeline again
and require byte-identical output: idempotence is a gate, not just an example.
Formatting must work without resolving imports, adapters or native functions.

## 4. Resource and file lifecycle

At most 128 explicit files, each input <= 2 MiB, cumulative input <= 16 MiB;
output per file <= 4 MiB, cumulative output <= 32 MiB. Count bytes with bounded
reads, checked arithmetic, and no proportional allocation before limits.
Open regular files on a handle using the existing nonblocking/fstat approach;
refuse directories, FIFOs and symlinks for in-place writes. Reject hard-linked
files for in-place mode, so replacing one name cannot silently split an alias.
--check may inspect hard-linked regular files. Keep all input bytes and file
identities until publication; detect edits/replacements since reading.

One worker per file, sequentially: the same executable's private formatter
mode, framed input/output through pipes, no inherited application source or
execution context. Parent drains bounded output and stderr while polling one
5-second deadline; output overflow, panic, malformed protocol or timeout
kills and reaps the worker. Bound the worker's address space (512 MiB), CPU
(2 seconds) and stack (8 MiB) before invoking any parser/formatter; limits
apply to the worker only. First cut uses Linux's process/resource primitives;
other platforms refuse fmt by name until an equivalent bound is implemented.
No new daemon or standing process. Test worker controls must not activate
through environment variables in ordinary builds.

Format and validate every file before the first target write. Stage each
result in a create-new sibling file with original permission bits, sync it,
and revalidate target identity/content before rename. Clean staged files on
failure. Each replacement is atomic; a set of file renames is not a
transaction. If publication fails midway, report exactly which files were
replaced and which were not, rather than claiming rollback. Recheck a target
immediately before its rename; document that this detects observed concurrent
edits but is not a filesystem compare-and-swap guarantee. Unchanged files are
not rewritten. Never expose worker stderr/source as an unbounded diagnostic.

## 5. Controls and evidence

- Exact tab bytes, blank function separation, nested blocks/modules/impls,
  pub/async functions, attributes/doc comments, line/trailing/block comments,
  macros with all delimiters, Unicode, shebang where accepted, multiline
  strings/templates and embedded HTML/CSS. Preserve every literal payload.
- Strict malformed-source refusal: incomplete macro (the earlier hanging
  reproducer), mismatched delimiters, unterminated strings/comments; no output
  or target edits. Named timeout and excessive output controls prove kill/reap.
- Fidelity catches token edits, lost comments and changed literal whitespace;
  injected comma normalization controls if that exception is needed.
- CLI statuses, stdin stdout purity, --, duplicate/aliased paths, invalid UTF-8,
  FIFO/directory/symlink/hardlink handling, per-file/total caps, permission
  preservation, no rewrite of already-formatted files, late-file syntax
  failure with all targets unchanged, publication failure and staged cleanup,
  concurrent-edit refusal. Verify worker failures leave no live descendants.
- Run on the committed terminal demos and representative source fixtures,
  without loading adapters; compare their outputs before/after where runnable.
  Use token/literal/comment parity on scripts whose external data is unavailable.
- External site.rn: retained formatting-only diff showing tabs and function
  gaps, repeat --check success and existing 26 HTTP fixtures plus keep-alive;
  reviewer sees this separately before any site commit is published.
- Root suites with test-support and feature-off, project suite for command
  dispatch integration, fmt/clippy/diff checks. Record binary-size change and
  six interleaved startup rounds (run/eval/prompt) against ffa2d4e; median
  slowdown >5% stops for review, with spread and retained raw measurements.

Implementation starts only after the plan review. Evidence records departures,
failed attempts and upstream limitations. Commit the generic rnx plan and impl
separately; keep benchmark artifacts and application formatting in their own
repositories. Formatter success is readability plus fidelity and bounded
failure, not a claim of full rustfmt style parity.

## 6. Plan review amendments (A1–A4)

A1: caps must admit a realistic dense valid source just under the selected
per-file maximum. Measure worker RSS, VSZ, CPU and wall time with that
control and record headroom against each limit. The provisional 2 MiB /
512 MiB AS combination is not accepted without this measurement: lower the
input cap or raise worker AS from the measured result, explaining the choice
in evidence before freezing final limits. Keep both hostile AS-exhaustion
(the unbounded unterminated-macro reproducer) and wall-timeout controls.

A2: dispatch the private worker before runtime/context setup. It builds no
tokio runtime, thread pool, Rune Context or battery/HTTP modules. Formatting
needs only the parser, Sources and formatter. A worker control checks one
thread and zero Context constructions. Parent limits and worker protocol
errors are independent of the ordinary execution path.

A3: report end-to-end rnx fmt --check latency for a typical source matching
the 0166 after.rn shape and the largest admitted file. There is no new numeric
latency gate; report worker spawn cost and the actual timings.

A4: add three unfiled entries to rnx-upstream-drafts for the user to file:
Rune 0.14.2 formatter unbounded allocation on unterminated
`println!("x",`, silent completion of `foo(1,` to `foo(1)` with recovery off,
and a public indentation option supporting tabs. Each has a minimal pinned
reproducer; the indentation request explains the span-aware workaround.
No upstream issue is sent or filed automatically.

### A1 capacity choice during implementation

Choose 1 MiB input per file with the same 512 MiB address-space, 2-second CPU
and 5-second wall limits; output remains 4 MiB. A 1,048,506-byte dense valid
source (about 9,000 functions) formats successfully with the full fidelity and
idempotence pipeline at approximately 1.19 seconds wall, 0.83 seconds user
plus 0.36 seconds system, and 229 MiB peak RSS on this machine. Record final
RSS/VSZ and repeat checks in evidence. Earlier provisional 2 MiB and initial
quadratic-edit attempts reached the CPU bound; those attempts remain disclosed.
This replaces the provisional 2 MiB input number in sections 2–4.
