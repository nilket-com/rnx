# rnx 0037: paths are strings, until a script needs them not to be

Status: implemented 2026-09-14 after review. Evidence is in
`0037_paths_are_strings_until_a_script_needs_them_not_to_be_evidence.md`. The
thirty-seventh record of rnx, and record
0031's gate 5, third part: the path helpers, and the decision records 0035
and 0036 deferred here — whether a path value is needed now to carry a
name that is not Unicode, or whether string helpers are the right thing
and the value waits for a use case. This record chooses the helpers, prices
the value so the choice is checkable, and states the property the value must
have so nothing here forecloses it.

## Context

Since record 0035 a script reads, writes, lists and removes files by
string paths, and since record 0036 it can find its home directory and its
arguments. What it cannot do is take a path apart or put one together
without writing the separator handling itself: the parent of a path, the
file's name, its extension, or a base joined to a part. Record 0031 said
path-taking functions accept strings, and that "a later path type must
interoperate without forcing a script to construct it for simple
operations". Records 0035 and 0036 then refused, by name and with the
bytes escaped, every non-Unicode name that came back from the OS — a
directory entry, an argument, a variable — and each said the place for
such a name to go was this record.

So there are two questions, and they are separable. The helpers are
needed by every script that touches more than one file. The value is
needed only by a script that has to act on a name the OS holds that is
not Unicode, and in three records of file, environment and argument work,
including the ports that drove them, no script has needed to. Both are
decided below, the second by pricing.

Measured on 2026-09-14, on Linux, what Rust's `std::path` says for the
cases a script meets. These are the semantics the helpers carry, because
the helpers are `Path::new(&str)` and nothing else:

| call | Rust's answer |
| --- | --- |
| `parent("a")` | `Some("")` |
| `parent("")` | `None` |
| `parent("/")` | `None` |
| `parent("a/b/")` | `Some("a")` — a trailing separator is ignored |
| `file_name("a/b/")` | `Some("b")` |
| `file_name("..")` | `None` |
| `file_name("a/.")` | `Some("a")` |
| `extension("a.tar.gz")` | `Some("gz")` |
| `extension(".bashrc")` | `None` — a leading dot is a name, not an extension |
| `extension("a.")` | `Some("")` |
| `file_stem("a.tar.gz")` | `Some("a.tar")` |
| `join("a", "b")` | `"a/b"` |
| `join("a/", "b")` | `"a/b"` |
| `join("a", "/b")` | `"/b"` — an absolute part replaces the base |
| `join("a", "")` | `"a/"` |
| `with_extension("a.tar.gz", "")` | `"a.tar"` — whose `extension` is `Some("tar")` |
| `with_extension("a", "x.y")` | `"a.x.y"` |
| `with_extension("a", "x/y")` | **panics in Rust**; refused here, decision 2 |
| `is_absolute("C:\\a")` | **`false` on Linux**, `true` on Windows |

The last row is the one to notice: these functions are the running
platform's, as every `std::path` function is. That is the Rust dual, and
it is stated rather than smoothed over.

## Decision

### 1. Helpers now, over strings; the value is priced and deferred

A path value — a native type holding the OS's bytes — would have to be
**assessed** against five contracts rnx already has before it could be
used: the renderer, which today prints a native type as an opaque
`<type>` marker (record 0019's test says so) and would need to show a
path; the JSON writer, which already refuses a native value it does not
know and would have to keep doing so or decide a representation; the
inspector and the completer, which know strings, objects, vectors and
Rune's own types; and equality and hashing, where the value would have to
say whether it means the spelling or the file — `a//b` and `a/b` are
different strings and the same file, and a value that compares by bytes
answers the first question while a script asking the second needs the
filesystem. Some of those assessments end in "nothing to change"; the
JSON one probably does. Others end in work. None of it has been done, and
it should not be done for a name no script has needed to carry.

So this record adds helpers over strings, in a `path::` module, with
record 0010's precedent: functions over `String`, each because scripts
need it. They are lossless by construction — a Unicode string through
`Path::new` and back is the same string — so record 0035's rule about
lossy conversion is not engaged, because nothing here can lose anything.

The value is **deferred, not refused**, and what this record commits to
is a property, not a representation: whatever carries a non-Unicode name
later must preserve the OS's native name — bytes on Unix, 16-bit units on
Windows — without loss, and must be accepted by the functions that take
paths alongside `String`, never instead of it. Whether that is a
`Bytes`-backed value, a native type, or something else, and which
functions produce it — `fs::read_dir` and `env::args` are the ones that
refuse such names today; an environment variable's *name* is not a path
and is not on the list — is that record's to decide with a use case in
front of it. This record's helpers take and return `String` and nothing
else, so a later representation cannot be surprised by them, and a
script written against them does not change.

### 2. The surface

```
path::join(base, part)        -> String
path::parent(p)               -> Option<String>
path::file_name(p)            -> Option<String>
path::file_stem(p)            -> Option<String>
path::extension(p)            -> Option<String>
path::with_extension(p, ext)  -> Result<String>
path::is_absolute(p)          -> bool
path::separator()             -> String
```

Eight functions, Rust's names, Rust's semantics, the table above as
their contract. Seven of them cannot fail and return no `Result`;
`parent`, `file_name`, `file_stem` and `extension` return `Option`
because Rust's do and because the `None` cases in the table are real
answers, not errors — a root has no parent and `..` has no file name.

`with_extension` is the one that can, and it is `Result<String>`: Rust's
`Path::with_extension` **panics** when the extension contains a
separator (measured: `"x/y"`), and a panic is not an answer rnx gives a
script. rnx checks the extension by the running platform's separator
rules — `/` on Unix, `/` or `\` on Windows — and refuses one that
contains a separator, naming it, as a catchable error. Otherwise it
follows Rust: replace the extension or add one, and given `""` remove the
last one — only the last, so `"a.tar.gz"` becomes `"a.tar"`, which still
has an extension. `separator` is the platform's, `"/"` or `"\\"`, for the
script that builds a display string and wants to say which.

Two of Rust's names are left out on purpose. `join` takes two arguments
and not a vector, because a fold over `join` is one line and a variadic
helper is a second way to spell it. `components` is deferred to decision
5: its answer on Windows involves prefixes and roots this record has not
measured and will not guess at.

### 3. The platform's rules, stated, gated on both

Every helper does what `std::path` does on the platform rnx is running
on. On Windows, both `/` and `\` separate, `join` inserts `\`, and
`is_absolute` needs a prefix **and** a root: `"C:\\a"` is absolute,
`"\\a"` (rooted, no prefix) is not, and `"C:a"` (drive-relative, no
root) is not. On Unix, `\` is an ordinary character in a name
and `"C:\\a"` is a relative path with a colon in it. A script that has to
reason about the other platform's paths — a build tool writing a Windows
path on Linux — has no helper here; that is a use case for a later
record, and it is named rather than half-served by a flag.

The table is gated on Linux, and its Windows rows are gated in record
0025's Windows suite — including a verbatim-prefixed base, a rooted path
without a prefix, and a drive-relative path — so that a difference
between the platforms is a test that reads differently on each and not a
surprise.

### 4. What the helpers do not do

They do not resolve `..`, and on Unix `join` preserves it: `join("a",
"../b")` is `"a/../b"`, because resolving `..` without asking the
filesystem is wrong whenever a component is a symlink, and `fs::absolute`
exists for the script that wants the filesystem's answer. Rust has one exception, and because the contract is
Rust's behaviour the exception is documented rather than removed: on
Windows, `join` onto a base with a verbatim prefix (`\\?\C:\...`) with a
non-empty part normalises `.` and `..` lexically, since a verbatim path
is handed to the OS without interpretation. That is stated beside `join`
and gated in the Windows suite. What the helpers do normalise is spelling,
in the ordinary way `Path` does: taking a path apart and putting it back
turns `a//b`, `a/./b` and `a/b/` into `a/b`, which preserves every
character of every name and does not preserve the original string; gate
2 says which, with each platform's separator. They do not validate: a path with a NUL in it is a string
like any other here, and the `fs::` function that receives it refuses it
there, naming it. They do not touch the disk.

### 5. What this record does not decide

- The path value: decision 1 names its shape and its trigger.
- `components`, `strip_prefix`, `starts_with` and `ends_with` over
  components, `canonicalize` without following the last link, globbing.
- A foreign-platform mode.

## Acceptance gates

1. **The table is a test.** Every row, on Linux; the platform-dependent
   rows with their Windows answers in the Windows suite, and the shared
   rows there too.
2. **Reassembly, with explicit answers.** `join(parent(p), file_name(p))`
   is `"a/b"` on Unix and `"a\\b"` on Windows for each of `"a/b"`,
   `"a//b"`, `"a/./b"` and `"a/b/"` — one lexical spelling for four,
   which says nothing about the filesystem, where `a/b/` needs `b` to be a
   directory and `a/b` may be a file — and is `"/a/b"` on Unix and
   `"/a\\b"` on Windows for `"/a/b"` (the existing leading slash is retained);
   `with_extension("a.txt", "")` is `"a"` with `extension` `None`, while
   `with_extension("a.tar.gz", "")` is `"a.tar"` with `extension`
   `Some("tar")`; `with_extension("a", "x/y")` is a catchable refusal
   naming the extension, and on Windows so is `"x\\y"`. Every name in
   every case contains a non-ASCII character, so what is preserved is
   demonstrated to be the characters.
3. **Answers do not depend on existence, and nothing touches the disk.**
   `parent`, `file_name` and `extension` give the same answer for a path
   before and after a fixture file is created at it, which shows the
   answers do not require existence; that no helper makes a filesystem
   call is established by source review of the module, which the evidence
   states, since a test cannot prove a negative about syscalls.
4. **The surface is exact.** `path::` lists eight names, each with `:help`,
   and `:help path::join` says that an absolute part replaces the base.
5. **Existing guarantees.** Both suites, the startup measurements and the
   session baseline before and after.

## Guardrails and stop conditions

1. No helper consults the filesystem, and none resolves `..` except
   where Rust does on a Windows verbatim base (decision 4), which is
   documented and gated.
2. No helper takes or returns anything but `String`, `Option<String>`,
   `Result<String>` or `bool` in this record; a non-Unicode form waits
   for the value.
3. No helper panics on any string. If a `std::path` function is found to
   panic on an input the helper accepts, the helper checks first and
   refuses, as `with_extension` does.
4. If any `std::path` behaviour in the table differs between the Rust
   version rnx pins and a later one, the row moves with a note, and the
   gate is what notices.

## Risks

- **`parent("a")` is `Some("")`, not `None`.** Rust's answer, and one
  step later `parent("")` is `None`, so a script that loops
  `while let Some(p) = parent(p)` ends on its own with one empty step;
  the README says so beside the function.
- **The Windows rows are authored, not executed**, until record 0025's
  suite runs on a Windows machine, as every record since has said.
- **A script that needs a non-Unicode name has nowhere to go yet.** True
  since record 0035, and stated there; decision 1 is the property the
  answer must have and the condition for building it.

## Forward

Record 0031's remaining batteries: time and dates, which record 0035's
`modified_ms` was shaped to feed, then the child-environment option on
`host::process` if a use case asks.
