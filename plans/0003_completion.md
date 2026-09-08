# rnx 0003: completion

Status: proposed 2026-09-08. The third record of rnx. The interactive
session (0002) can be kept open; this record makes it discoverable at the
keyboard: Tab completes what the session already knows. It decides what is
completed, where the names come from, how the input is edited, and how
completion follows the session's state. It leaves member completion and
inferred types for a later record.

## Context

### What the session knows

The session holds the names of published bindings, the retained
declarations, and the commands it accepts. Those are read from its own
tables. Nothing about them requires running Rune.

### What upstream provides, and does not

Rune 0.14.1 exposes no public way to enumerate what a `Context` has
installed: the context's iteration methods are crate-private, a `Module`'s
item lists are crate-private, and the documentation walker that could list
items is behind a feature and internal. The compiler's visitor hook
reports metadata for items being compiled, not for the installed context.
So the standard library's paths, `std::string::String::new` and its kin,
cannot be listed from upstream without a fork or a handwritten copy, and
this record refuses both.

What rnx can list without a second catalogue is what it installs itself:
the host module is built in one place, and the names registered there are
the names, recorded as they are registered.

### What the editor provides

The line editor asks a completer for candidates given the whole buffer and
the cursor position, and replaces a range the completer names. That is
enough to replace only the token at the cursor in a multi-line input.

## Decision

### 1. What Tab completes

Four kinds of name, deduplicated by replacement text and, when ambiguous,
ordered: published bindings; retained declarations, functions, structs, and
enums; the host module's function paths, `host::` and each of its functions;
and the session's commands, `:quit`, `:reset`, `:memory`, `:debug`, and any
added later. A qualified prefix such as `host::wr` completes within that
module. Rune's standard library is not completed in this release, and the
record says so rather than shipping a copy of it; a `std::` prefix produces
no candidates and no error. A binding and a declaration with the same name
are one candidate, listed under the earlier kind.

### 2. Names come from the live session, never from running it

Candidates are read from the session's binding and declaration tables and
from the host module's registration record. Completion executes no Rune
code and compiles nothing. A name the session has not published is not a
candidate.

### 3. The whole token containing the cursor is replaced

The token is the identifier or path that contains the cursor, found by
tokenising the input with Rune's own parser, or a leading `:` and letters
for a command at the start of an input. The part of the token before the
cursor is the prefix that selects candidates; the whole token is what a
candidate replaces, so completing `alp` in `alp|ha + beta` yields
`alpha + beta`, never `alphaha`. Everything outside the token is preserved
byte for byte. With one candidate, it replaces the token. With several,
they are listed and the token becomes their common prefix. With none,
nothing changes.

Completion is offered only where a name can stand. It is suppressed when
the cursor is inside a string, template, or block comment, which the
parser reports as unterminated at the cursor, or after a `//` that follows
the last token on the cursor's line; when the token is preceded by `.`,
since member completion is not decided here; and when the text at the
cursor is not an identifier or path token at all, so a run of letters that
Rune would not lex as a name gets no candidates. Byte offsets are computed
on the actual input, so text with multibyte characters before the cursor
completes at the right place.

### 4. Completion follows the session's state

A binding or declaration appears once the input that published it
succeeded, and not before; a failed input publishes nothing, so its names
never appear. A declaration's new form replaces its old form; a
redefined function is one candidate. `:reset` empties bindings and
declarations, leaving host paths and commands. History restore, which
executes nothing, contributes nothing.

### 5. What this record does not decide

Member completion after `.`, which needs a value's type; inferred types;
completion of the standard library; documentation or signatures alongside
candidates; `:vars` and `:help`, which are their own record.

## Acceptance gates

1. **The ordinary use.** In a terminal session: bind `alpha` and `alphabet`,
   type `alp` and Tab, and see both listed with the token extended to
   `alpha`; type `b` more and Tab, and see `alphabet` completed. Define
   `fn total(xs)`; `to` Tab completes `total`. `host::wr` Tab completes
   `host::write_new`; `host::` Tab lists every host function and only
   those. `:me` Tab completes `:memory`. `std::` Tab changes nothing.
2. **Mid-token and mid-line editing.** With `alpha + beta` on the line
   and the cursor after `alp`, Tab replaces the whole token and leaves
   `alpha + beta` with the cursor after `alpha`; with `alpx + beta` and the
   cursor after `alp`, Tab replaces `alpx` with `alpha`; with the cursor
   inside a later line of a multi-line input, the earlier lines are
   byte-identical afterwards.
3. **Where completion is not offered.** Inside `"alp`, inside a template,
   inside `/* alp`, after `// alp` on the line, and after `obj.alp`, Tab
   changes nothing; `"héllo" + alp` with the cursor at the end completes
   `alpha` at the correct byte range; `host::wr` completes within the path
   and `hos` completes `host::`; a bare `::alp` gets no candidates.
4. **After failure and reset.** An input that binds `gone` and then panics
   leaves `go` Tab with no candidate; the next input that binds it makes it
   one. A redefined `total` is listed once. After `:reset`, `alp` Tab has no
   candidate while `host::` and `:me` still complete. A binding and a
   declaration named `same` appear once.
5. **No execution.** A binding whose value is a closure with a side effect,
   and a host function with a side effect, are candidates; completing them
   produces no effect, proved by an observable one that does not occur.
6. **One source for host names.** The list of host function candidates is
   produced by the same registration that installs the module; a test
   installs the module and checks that every registered name is a candidate
   and every candidate is registered.
7. **Nothing regresses.** The 0002 gates pass unchanged.

## Guardrails and stop conditions

1. If a candidate would require evaluating, compiling, or inspecting a
   value, it is not a candidate in this release.
2. No handwritten list of upstream names, ever; if upstream cannot
   enumerate something, the record names the limit. Token boundaries come
   from Rune's parser, not from a character class of the implementation's
   own.
3. Only the token containing the cursor changes; a test proves the bytes
   outside that token are identical before and after.
4. No change to the session semantics of 0001 or 0002 rides on this cut.

## Risks

- **A completion that looks like knowledge of the standard library.** The
  empty result for `std::` is deliberate and documented, so a person does
  not conclude the library is absent.
- **Terminal differences in how candidate lists render.** Gate 1 runs on
  Linux through the same harness as 0002; the platform ladder is still the
  first release's.

## Forward

`:vars` and `:help <name>` as the next record, drawing on the same tables;
member completion once a value's type can be read without running code,
which is a question to take upstream rather than to answer here.
