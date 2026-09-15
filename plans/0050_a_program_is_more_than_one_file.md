# rnx 0050: a program is more than one file

Status: implemented 2026-09-15. Planned in `e6c56cd`, after Claude's
proposal and Codex's review. Implementation clarifies the private Rune error
and source-text APIs below; the evidence file records the gates and costs.
The fiftieth record is step one of the agreed extensibility sequence: local
file modules for `rnx run`. It chooses no package search path, registry or
dependency syntax; those are step five.

## Context

At `bb2e646`, `rnx run` reads the file and compiles it as an anonymous
in-memory source (`runner.rs` `Source::memory`). Rune resolves `mod name;`
at compile time through a `SourceLoader`, and its file loader refuses a
source that has no path. So a two-file program fails today with:

    error at main.rn, line 1, column 1: Cannot load modules using a source without an associated URL

Inline modules (`mod m { pub fn f() { 5 } }`) already compile and run in a
file. Eval, sessions and the worker refuse every module, inline or file,
with an existing message that points to files: "a session accepts fn,
struct, and enum declarations; modules, imports, macro declarations, and
impl blocks work in files". This record makes that sentence true for file
modules and changes nothing about where sessions stand.

The runner's header states its founding assumption: "a position in the
compiled unit is a position in the file and no mapping is needed beyond
the file's own text". Compile diagnostics are printed against the entry
text, and `fault_offset` maps a runtime fault to an offset that `located`
then looks up in the entry text. With a second source in the unit, an
offset alone names the wrong file. Rune already carries what is needed:
each fatal compile diagnostic has a `source_id()`, each `DebugInst` has a
`source_id`, and `Sources::get(id).path()` returns the file. Replacing the
single-source assumption is the substantive change of this record; enabling
the loader is the smaller part.

There is no size bound on the entry file today. `std::fs::read_to_string`
reads whatever is there. The session's 32 KiB `INPUT_CAP` is per input and
is the wrong unit for a program. A bound is introduced here so that
splitting a program across files cannot mean an unbounded compilation.

### Observed resolution in Rune 0.14.2

Measured before drafting with a standalone probe (rune 0.14.2, `std`
feature, `FileSourceLoader`, `Source::from_path` for the entry). The
loader (`compile/source_loader.rs` 43-68) pops the file name off the
root path it is given, pushes every component of the module's full item
path, then tries `<that>/mod.rn` before `<that>.rn`. The indexer passes the
entry source's path as that root. Consequences, all observed:

| entry `one/main.rn` declares | resolves to | note |
| --- | --- | --- |
| `mod a;` | `one/a.rn` | |
| `pub mod b;` inside `a.rn` | `one/a/b.rn` | item path `a::b`, not `b.rn` beside `a.rn` |
| `mod inline { pub mod deep; }` | `one/inline/deep.rn` | an inline module contributes a path component |
| `mod c;` with both `c.rn` and `c/mod.rn` present | `one/c/mod.rn` | `mod.rn` is tried first and wins |

The program returned 21111, the sum that only that set of files produces.
`b.rn` placed beside `a.rn` instead of under `a/` fails: "File not found,
expected a module file like `six/a/b.rn`". A module declared twice fails at
the second declaration: "Module `a` has already been loaded". A missing
module fails at the `mod` line in the declaring file and names the expected
path. A compile error inside an imported file carries that file's source
id. A runtime error two levels down (`x.missing()` in `a/b.rn`) reports
`source_id 2`, span 35..46, path `a/b.rn`. Paths are recorded exactly as
the entry path was given: relative entry, relative module paths; absolute
entry, absolute module paths. Module lookup is anchored to the entry path
as given; a relative entry path resolves against the working directory as
any relative path does, and there is no additional working-directory
search for modules. Item paths cannot contain `..`, so a module
cannot name a file outside the entry file's directory tree by declaration;
a symlink inside the tree is followed because the loader checks `is_file()`.

## Decision

### 1. Only `run` loads files, and the entry file is the root

`rnx run PATH` inserts the entry as a source with its path, as given on the
command line, and builds with rnx's bounded loader implementing Rune's
pinned resolution rule. Module resolution is the
rule observed above, stated in the README as Rune's rule: a module's file is
the entry file's directory joined with the module's full item path, `mod.rn`
preferred over `.rn`. Modules are found from the entry path and nowhere
else; the working directory matters only in resolving a relative entry
path, as it does today. Nested
modules follow the same rule from the entry, so `pub mod b;` inside `a.rn`
lives at `a/b.rn`; the README shows that layout once, with the inline case,
because it is the one people will get wrong.

Eval, sessions and the worker keep refusing modules exactly as today; their
message already says files are the place. The config evaluator keeps its
`NoopSourceLoader` and its memory source; settings must not read files.
`--debug-source` prints every loaded source, each under a header naming its
path, in load order.

### 2. Every diagnostic names its own file

Compile: the first fatal compile error is reported with the path and text
of the source its `source_id()` names, not the entry. Runtime: `fault_offset`
becomes a fault location, `(source id, offset)`, taken from the `DebugInst`
of the faulting instruction; `located` prints that source's path, line,
column and caret. The existing rule that a fault inside a called function
reports the callee's expression is unchanged and now extends across files.
A fault whose source id is not in the sources, or a unit that is not ours,
still reports as unlocated rather than guessing.

Two other readers of "the file's text" must follow the fault to its file.
Record 0014's method naming (`method::named`, `runner.rs` around line 297)
recovers a missing-method name from candidates found in the entry text; it
must search the faulting source's text instead, and when the faulting
source is unavailable it leaves the hash alone rather than falling back to
the entry, as 0041 requires. Returned-value rendering (`declared::in_file`,
around line 320) collects field-name candidates from the entry only; it
must collect candidates from every loaded source, still verifying each
against the value as today, so a struct or a named enum variant declared
in a module renders with its names.

Rune also keeps the source-text accessor private. The loader retains the
bounded read beside Rune's source; diagnostics resolve the actual source
id through `Sources` and use its path to find that snapshot. Load order is
not assumed to be source-id order: a duplicate declaration can load without
insertion. If repeated reads of one path disagree, attribution is refused
rather than guessed. No diagnostic rereads a file from disk.

Paths print as recorded, so a program run as `rnx run app/main.rn` reports
`app/a/b.rn`, and one run by absolute path reports absolute paths. This is
the entry file's existing behaviour extended to its modules; no
canonicalisation is added. Rune's own "expected a module file like" wording
is kept in the missing-module case, since it names the path a person must
create.

### 3. One allowance for the whole compilation

A run may read at most 8 MiB of UTF-8 source in total, the entry file and
every module it loads. This is a new bound, not an existing guarantee. It is
enforced while reading: the loader knows the remaining allowance before it
opens a file and refuses a file that would cross it without reading the
rest, naming that file and the allowance. The entry file is read under the
same allowance. Non-UTF-8 content in any file is refused naming that file,
as the entry is today. The number is chosen so that a program of ordinary
size never meets it and a mistake, such as a module pointed at a generated
dump, does. It is a constant with a test-only injection point so the gates
can exercise the boundary with a small allowance rather than 8 MiB fixtures.

Rune's `FileSourceLoader::load` selects the candidate and immediately calls
`Source::from_path`, which reads the whole file; there is no public hook
between choosing and reading, so it cannot be wrapped. rnx therefore ships
its own small `SourceLoader` that reproduces the pinned 0.14.2 candidate
rule exactly: pop the file name off the root, push every component of the
item path, refuse a non-string component, try `mod.rn` then `.rn`, accept
the first that `is_file()`, and report the missing-module message using the extensionless base.
Implementation source review found `ErrorKind::ModNotFound` crate-private
as well: the public `compile::Error::msg` constructor carries the exact
text of Rune's formatter, which appends `.rn`; passing the candidate would
print `name.rn.rn`. The comparison gate checks the message and location,
not equality of the private error variant. A metadata
size check before opening is only an optional early refusal: a file can
grow after the check, and some files report a size smaller than their
contents. The bound is the read itself, limited to the remaining allowance
plus one detection byte; if that extra byte arrives the file is refused as
crossing the allowance and the bytes read are discarded. The detection byte
is the only overshoot and is stated as such. A file within the allowance is
charged for what was actually read, then validated as UTF-8. Exhaustion is sticky: once the
allowance is crossed every later load is refused without opening anything.
The loader counts the files it opens, under test only, so the gates can
prove a refused load opened nothing. Fidelity to Rune's loader is a gate,
not an assumption: the same fixtures resolve identically through both.

### 4. What does not change

The instruction budget, `--budget`, the argument contract after the path,
stdout/stderr ownership, the exit-status rules and the presentation of
errors are untouched. One entry, one `main`. No `use` of a package, no
search path, no manifest, no `--module-path` flag: every module is found
under the entry file or not at all. No caching of compiled units.

## Acceptance gates

1. **Resolution, observed and stated.** The mixed fixture above, checked
   into `tests/` with its layout, resolves to the same paths through rnx's
   loader and through Rune's `FileSourceLoader` (a unit test drives both on
   every fixture, including the misplaced, duplicate and missing cases),
   runs through `rnx run` and returns 21111,
   from the entry's directory and from another working directory, by
   relative and by absolute entry path. The sibling misplacement, the
   duplicate declaration and the missing module each produce the expected
   refusal at the expected file and line. The README's layout statement is
   checked against this fixture, not written from the loader's source.
2. **Diagnostics name the file.** A compile error inside a nested file
   module reports that file's path, line, column and caret line. A runtime
   error inside a function two files deep reports its file and the callee's
   expression, with the caret under the faulting expression. The entry
   file's existing diagnostic tests pass unchanged, byte for byte where they
   assert on output. A missing method on a value inside a module reports
   the recovered name from that module's text; the same fault with the
   faulting source withheld reports the hash, never a name from the entry.
   A returned struct and a returned named enum variant declared in a module
   render with their names. `--debug-source` shows each source under its
   own header.
3. **The allowance holds across files.** With the injected allowance, a
   program whose files sum to exactly the allowance runs; one byte over,
   spread so that no single file is over, is refused naming the file that
   crossed it. The instrumented open counter proves no file after it was
   opened; an absent later file cannot prove that, because only the first
   fatal diagnostic is printed. A second declaration after exhaustion is
   refused without an open, proving stickiness. A reader whose reported
   size understates its contents (a test-only source, or a file appended
   after the metadata check) is still refused by the read bound, with at
   most the one detection byte read beyond the allowance. Under the real constant, a single
   module over 8 MiB is refused without being held in memory. A non-UTF-8
   module is refused naming the module.
4. **Nothing else loads files.** Eval, a session before and after reset,
   and the worker refuse `mod name;` and inline modules with today's
   message. The config evaluator still refuses to read a file. A settings
   fixture that declares a module still reaches the prompt.
5. **Regression and cost.** Both root suites, formatting, clippy at the
   inherited baseline, and the notices check. A single-file program's
   startup and run timing are measured against the accepted binary with
   the existing matched procedure; the entry now carries a path and goes
   through a loader, and this record does not assert that costs nothing
   until it is measured. Windows type-check for the new code; Rune's path
   joining is the platform's, and execution stays unverified as before.

## Guardrails and stop conditions

- The entry file is the only root. If any part of the implementation needs
  a search path or an environment variable to find a module, stop; that is
  step five's decision.
- No diagnostic may print an offset against the wrong file. If a fault
  cannot be attributed to a source, it is unlocated, as today.
- The allowance is enforced before a file is held, not after. Reading a
  file and then measuring it is a regression of the bound this record adds.
- Sessions, eval, the worker and the config evaluator gain no file loading.
  If sharing the loader would give one of them file access, do not share it.
- The rnx loader reproduces Rune's candidate rule and nothing more. Stop
  if fidelity to `FileSourceLoader` on the fixtures cannot be shown, if a
  Rune upgrade changes the rule (the pin is 0.14.2), or if extending
  `located` requires changing the shape of the entry file's existing
  diagnostics.

## Risks and forward

People will place `b.rn` beside `a.rn` because that is where it looks like
it belongs; Rune's error names the expected path, and the README shows the
layout, which is the remedy. Symlinks inside the tree are followed and this
record does not police them. Notebook cells still cannot declare modules;
loading files into a session is a separate decision. Step two, a supported
library interface for assembling rnx with extensions, follows this record
and does not depend on its path rule beyond "the entry file is the root".
