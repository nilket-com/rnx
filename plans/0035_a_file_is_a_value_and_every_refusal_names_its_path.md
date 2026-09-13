# rnx 0035: a file is a value, and every refusal names its path

Status: implemented 2026-09-13 after review. Linux gates and measured costs
are in the accompanying evidence; Windows gates are authored and the new
module type-checks, but Windows execution remains outstanding. The
thirty-fifth record of rnx, and record
0031's gate 5, first part: files and directories. It gives a script the
filesystem the way `host::process` and `http::get` already gave it a child
and a response — whole values, stated bounds, refusals that say which path
and why — and it moves the four file functions rnx already has into the
module where the rest of them belong.

## Context

rnx has touched files since record 0003: `host::read` (a whole file as
UTF-8, 8 MiB, refused past that), `host::write_new` (a file that must not
exist yet), `host::mkdir` (one directory), and `host::absolute` (an existing
path, canonical). Each names its path when it fails — a test called
`every_error_that_touches_a_path_names_it` has held that since the first
port could not tell which of several files was missing. That is the
contract this record extends, not one it invents.

What a script cannot do today is most of what people open Python for:
read bytes, replace or append to a file, list a directory, ask whether a
path exists or what it is, copy, rename, or remove anything. Record 0031
listed these as the filesystem breadth to add by use case, and set three
constraints: keep Rust's names where they fit, take paths as strings, and
decide what happens to a name that is not Unicode rather than converting
it lossily by default.

Upstream `rune-modules` 0.14.2 has an `fs` module. It is one function,
`read_to_string`, through tokio, with no bound and with `io::Error`'s
message and nothing else — no path. Record 0031 already said to adapt it
to rnx's contract; there is nothing in it to adapt, so this record writes
rnx's own.

## Decision

### 1. One module, `fs`, and the four existing functions move into it

Record 0031 asked for domain namespaces. The four file functions were
registered under `host::` before there was a module for them to live in.
They move: `fs::read`, `fs::write_new`, `fs::mkdir`, `fs::absolute`, the
same functions with the same contracts. The `host::` names go. This is a
**breaking change, made on purpose**: rnx is at 0.0.0 with `publish =
false`, which says nothing about scripts on someone's disk, only that the
project has made no promise to them yet. It is made now because the cost
only grows, and it is documented rather than eased: the README's file
section names each old name and its replacement, and a script that calls
a `host::` file function gets Rune's ordinary missing-function error,
which names the function. An alias would be cheap; it would also be a
second name for the same contract for as long as anyone remembered it,
and this record prefers one. Two things change in `read` with the move,
both deliberate and both listed here as part of the migration: its
refusal of a file that is not UTF-8 gains the byte and the words "use
fs::read_bytes to read it", the form `host::process` and `http::get`
already use; and it reads regular files only, refusing a FIFO, a device
or a directory that `host::read` would have opened and, for a FIFO,
blocked on (decision 7). `host::stdin` stays where it is: it is not a
file.

### 2. The surface, in Rust's words, with string paths

Reading, whole, under the bound record 0003 set:

| function | returns | note |
| --- | --- | --- |
| `fs::read(path)` | `String` | as today: UTF-8 or refused naming the byte, pointing to `read_bytes` |
| `fs::read_bytes(path)` | `Bytes` | the same 8 MiB bound |

Writing, whole, with the overwrite decision in the name:

| function | note |
| --- | --- |
| `fs::write_new(path, data)` | as today: refuses an existing file, naming it, **atomically** — `create_new`, which the OS makes exclusive |
| `fs::write(path, data)` | creates or replaces, whole |
| `fs::append(path, data)` | creates or extends |

`data` is a `String` or `Bytes`, as an HTTP request body is; anything else
is refused before the file is touched. `write` replaces in place — open,
truncate, write — which is `std::fs::write` and what Rust programmers
expect. It keeps the file's permissions, ownership and hard links, and it
means a crash mid-write leaves a shorter file, not the old one. The
alternative, write-to-temp-then-rename, gives the old file back after a
crash but gives the new one fresh permissions and a new inode, which is
the surprise a script cannot see coming. Rust chose in-place; rnx does
too, stated, and an atomic variant is a later function with its own name.

Asking:

| function | returns |
| --- | --- |
| `fs::exists(path)` | `Result<bool>`: a missing path or a dangling symlink is `Ok(false)`; a permission refusal or any other error is `Err`, naming the path, because "false" would be a lie about a file that is there |
| `fs::metadata(path)` | an object, decision 4 |
| `fs::read_dir(path)` | a `Vec` of entry names, decision 5 |
| `fs::absolute(path)` | as today: an existing path, canonical, or refused naming it |
| `fs::cwd()` | the working directory as a `String` |
| `fs::temp_dir()` | where temporary files go, as a `String` |

Changing:

| function | note |
| --- | --- |
| `fs::mkdir(path)` | as today: one directory, the parent must exist |
| `fs::mkdir_all(path)` | every missing directory on the way |
| `fs::copy(from, to)` | a regular file's bytes and permissions; refuses an existing `to`; not transactional, see below |
| `fs::rename(from, to)` | refuses an existing `to`, on every platform |
| `fs::remove_file(path)` | a file or a symlink, never what a symlink points at |
| `fs::remove_dir(path)` | an empty directory |
| `fs::remove_dir_all(path)` | a directory and everything in it, decision 6 |

Two of those depart from Rust on purpose, and for one reason: **nothing
replaces a file unless its name says so.** `std::fs::rename` and
`std::fs::copy` both replace an existing target, on Unix and on Windows
alike (an earlier draft of this record said Windows differed; it does
not). rnx's refuse, so a script that means to replace removes first, or
calls `write`, and says so in its own text.

Both refusals are **atomic**, not a check followed by an act, because a
check-then-act window is exactly what guardrail 2 forbids and what
`write_new` has never had. `copy` opens its destination with `create_new`
and copies through that handle, so the exclusivity is the OS's. `rename`
uses the primitive each platform has for a rename that must not replace:
`renameat2` with `RENAME_NOREPLACE` on Linux, `renamex_np` with
`RENAME_EXCL` on macOS, and `MoveFileExW` without `MOVEFILE_REPLACE_EXISTING`
on Windows — all three reachable through the `libc` and `windows-sys`
crates rnx already depends on. Where a filesystem refuses the flag (a
Linux filesystem older than the flag, or a network one), the call is
refused naming the path and the reason; it does not fall back to a
check-then-act rename. Guardrail 4 makes a platform where this cannot be
done a stop, not a tolerance.

Exclusive creation makes `copy` refuse to replace; it does not make it
transactional, and the record says what a failure leaves. The source is
validated first — opened, and checked on the handle to be a regular file
(decision 7) — so a source that is a directory or a FIFO is refused before
anything is created. Then the destination is created exclusively, the
bytes are copied, and the source's permissions are applied. A failure
after the destination exists — disk full mid-copy, a permission the
platform will not apply — **leaves the destination as it is**, partial or
complete, and the refusal names both paths and says the destination was
created and left, so the script knows there is something to remove. rnx
does not remove it, because removing a file after a failure the script
did not ask about is the kind of second action a script cannot see
coming, and because a half-copied file is evidence of what went wrong.

### 3. Every path is a string, and a name that is not one is refused

Functions take and return `String` paths. On Unix a name is bytes and on
Windows it is 16-bit units, and either can hold something that is not
Unicode. Record 0031 says do not convert lossily by default. So:

- a path a script *passes* is UTF-8 by construction, and goes to the OS
  as is; on Windows, forward slashes are accepted, as Rust accepts them;
- a name that *comes back* — from `read_dir`, `absolute`, `cwd`,
  `temp_dir` — that is not valid Unicode is refused, naming the directory
  it was found in and the offending name escaped, the way rnx escapes
  everything it prints (record 0009). Nothing is replaced with U+FFFD and
  returned as though it were the name.

A script that has to handle such names needs a bytes-path form, and that
is record 0031's later path type, not this record. What this record
guarantees is that a script never acts on a name that is not the one on
disk.

### 4. `metadata` is a small object, and its time is a number

```
#{ kind: "file" | "dir" | "other", size: u64, modified_ms: i64,
   readonly: bool, symlink: bool }
```

`kind`, `size`, `modified_ms` and `readonly` follow the link, as Rust's
`metadata` does; `symlink` says whether the path itself is one, from
`symlink_metadata`. Those are two calls, not one snapshot, and the record
says so: a link replaced between them can report the old target's kind
with the new path's link-ness. A dangling link is refused — `metadata`
follows it to nothing, and the refusal names the path and says the link's
target is missing, so a script can tell a dangling link from an absent
path by the message and by `exists`, which is `Ok(false)` for both.

`modified_ms` is milliseconds since the Unix epoch as a plain integer,
because record 0031 puts calendar and wall-clock semantics in their own
design and a number is what that design can be built on without this
record guessing at it. A time before the epoch is negative, rounded
toward negative infinity to the millisecond, so that a time and its
rounding never straddle the epoch in different directions. A time outside
what an `i64` of milliseconds can hold — some 292 million years either
way — refuses the whole call naming the path, as does a filesystem that
reports no modification time; neither invents a zero.

### 5. `read_dir` returns names, sorted, bounded

Entry names only, not paths, without `.` and `..`, sorted by bytes so the
order is the same on every run and every platform, and refused past
100 000 entries naming the directory and the bound. The bound is the same
reason as the read bound: a listing is one value in one process under
record 0005's ceiling, and a directory with a million entries is a
directory a script should walk in pieces, which is a later function. A
name that is not Unicode refuses the whole listing by decision 3; a script
cannot receive 999 names and silently miss one.

### 6. `remove_dir_all` refuses the places nobody means

It removes a directory and its contents without following symlinks into
other places, which is Rust's behaviour: a symlink met inside the tree is
removed as a link, and its target is untouched. rnx adds one refusal, and
states its symlink semantics precisely, because the refusal reasons about
where a path leads and removal does not follow the last link:

- If the path itself is a symlink (`symlink_metadata`), the link is
  removed and nothing else, whatever it points at — a link to the working
  directory, to root, or to nothing. No guard applies, because nothing
  behind the link is touched.
- Otherwise the path is canonicalised, and the result must not be a
  filesystem root — `/`, a drive root such as `C:\`, or a UNC share root
  on Windows — the working directory, or an ancestor of the working
  directory. The refusal names the path and which of the three it hit.
  Removal then proceeds by the path the script gave, not the canonical
  one.

Every one of those is a script that computed a path wrongly. It is not a
safety net for a path that is merely large — a script that names its own
project directory gets it removed — and it is not atomic against a path
being changed underneath it between the check and the removal, which the
record states rather than pretends otherwise.

### 7. Synchronous, with no deadline and no way to interrupt a call

Every function here is a plain blocking call, as `host::process`'s
spawning is, and not a tokio future. Upstream's async `read_to_string`
buys nothing for a bounded local read and costs a runtime turn per call.
What synchronous means is stated in full: **no call here has a deadline,
and `Ctrl-C` cannot end one part-way**; record 0032's driver only regains
control when the call returns. The size bounds bound memory, not time. A
read of 8 MiB from a local disk is milliseconds; the same read from a
network filesystem that has stopped answering, a `remove_dir_all` over a
tree of a million files, or a `read_dir` on a directory being written by
something else, takes what it takes, and the prompt waits.

One of those cases is avoidable and is avoided. `read` and `read_bytes`
accept **regular files only**: the file is opened, its metadata read
through the open handle, and anything that is not a regular file — a
FIFO, a device, a socket, a directory — is refused naming the path and
its kind, before a byte is read. The check is on the handle, not by a
metadata call before the open that the file could change under. What the
check does not do is bound the open itself, and the record is precise
about where it does: on Unix, opening a FIFO with no writer blocks until
one appears, and rnx opens with `O_NONBLOCK`, which makes that open
return at once and leaves a regular file reading normally; that is the
one blocking open this record removes. A device whose open blocks for its
own reasons, on Unix or on Windows, is not covered; Windows has no
equivalent flag for `CreateFile`, and this record promises nothing there
beyond the handle check after the open returns. A path that *resolves* to
a regular file is read as that file — `/dev/stdin` with a file redirected
into it is one — because the check is about what was opened, not about
the name; `host::stdin` remains the way to read the stream as a stream.
The remaining case, a filesystem that does not answer, is not solved here
and is the same case `host::process` leaves open for a child on one.

### 8. What this record does not decide

- A path type, path helpers (`join`, `parent`, `extension`), and bytes
  paths: record 0031's later path design.
- `home_dir`, environment variables, and the script's arguments: the
  environment record.
- Walking a tree, globbing, watching for changes, streaming a large file,
  file locks, and permissions beyond `readonly`.
- Atomic replace (`write` via temp-and-rename) as a named function.

## Acceptance gates

Every gate runs in a fresh temporary directory it creates and removes, on
Linux and in record 0025's Windows suite, except where a line says Unix.

1. **Every function names its path on failure.** The existing gate is
   extended to every function above: a missing path, a directory where a
   file was expected, a file where a directory was expected, and a
   permission refusal (Unix) each produce "cannot <verb> <path>: <why>"
   and nothing that could be mistaken for another path.
2. **Reading is bounded and strict.** `read` and `read_bytes` of a file at
   8 MiB succeed and one byte past it is refused naming the limit;
   `read` of a file with byte `0xFF` is refused naming the byte and
   `read_bytes`, and `read_bytes` returns it.
3. **Writing does what its name says.** `write_new` refuses an existing
   file; `write` replaces one and keeps its permissions (Unix: mode bits
   compared before and after); `append` extends one and creates a missing
   one; all three accept `String` and `Bytes` and refuse an integer before
   touching the disk, asserted by the file not existing afterwards.
4. **Asking is accurate.** `exists` is `Ok(true)` for a file and a
   directory, `Ok(false)` for nothing and for a dangling symlink (Unix),
   and `Err` naming the path for a parent directory without search
   permission (Unix); `metadata` reports `kind`, `size`, a `modified_ms`
   within a stated tolerance of the fixture's clock and, for a file whose
   time is set before the epoch (Unix, `utimensat`), a negative value
   rounded toward negative infinity; `readonly` after `chmod` (Unix) or
   the read-only attribute (Windows); `symlink` true only for a link, and
   a dangling link refused with a message that says the target is missing
   (Unix); `read` of a FIFO and of a directory refused naming the kind,
   without blocking (Unix, the FIFO with no writer).
5. **Listing is sorted, bounded, complete.** `read_dir` of a directory
   with names that would sort differently by locale returns them in byte
   order without `.` and `..`; 100 000 entries are listed; 100 001 are
   refused naming the bound; a directory containing a non-UTF-8 name
   (Unix) refuses the whole listing naming the directory and the escaped
   name.
6. **Changing is the same on both platforms.** `rename` and `copy` refuse
   an existing target on Linux and on Windows, in the same words; `copy`
   preserves mode bits (Unix); `mkdir_all` creates three levels and is
   a no-op on an existing directory; `remove_file` on a symlink removes
   the link and not the target (Unix); `remove_dir` refuses a non-empty
   directory; `remove_dir_all` removes a tree, does not follow a symlink
   inside it (Unix), removes a top-level symlink to the working directory
   as a link and leaves the directory intact (Unix), removes a dangling
   top-level link (Unix), and refuses `/`, the working directory, and its
   parent on Linux and the drive root and working directory on Windows,
   naming which. `rename` and `copy` onto an existing target are refused
   on both platforms with **both files unchanged**, asserted by content
   and modification time before and after. The atomicity of `rename` is
   not something a fixture can race into proving, and this record does
   not pretend one does: it is gated at the primitive — a unit test on
   the platform layer asserts that the call made is `renameat2` with
   `RENAME_NOREPLACE` on Linux and `MoveFileExW` without
   `MOVEFILE_REPLACE_EXISTING` on Windows, and that no path-existence
   check precedes it — and rests on those calls' documented guarantee.
   `copy` of a directory and of a FIFO (Unix) is refused before any
   destination exists, asserted; a copy that fails after the destination
   is created leaves the partial destination and names both paths and
   the fact it was left. That failure is injected, not provoked: under
   `test-support` the copy's reader can be told to return an I/O error
   after a stated number of bytes, and the gate asserts the destination
   holds exactly that many. A permission change on the source cannot
   provoke it — an open descriptor outlives a `chmod`, review verified —
   and nothing in the gate pretends otherwise.
7. **The move is complete.** `host::read`, `host::write_new`,
   `host::mkdir`, `host::absolute` are gone from the registered names,
   `fs::` lists exactly the functions in decision 2's tables — the
   eighteen names from `read` to `remove_dir_all`, `absolute` among
   them — `:help` describes each, the README names every old name and its
   replacement, and every existing test that used a `host::` file
   function now uses `fs::` and passes, with the one message change
   decision 1 names asserted as changed.
8. **Nothing else moved.** Both suites, the startup measurements in
   rnx-bench, and the session baseline, before and after; a file read
   through `fs::read` costs what `host::read` cost.

## Guardrails and stop conditions

1. No function returns a name it converted lossily. If a platform makes
   that impossible to detect, stop and say so.
2. No function replaces a file unless its name says so: `write` and
   nothing else. That includes the window between a check and an act:
   `write_new`, `copy` and `rename` refuse an existing target through an
   operation the OS makes exclusive, never through a check rnx makes
   first.
3. One reading bound, record 0003's 8 MiB, shared with `stdin`. If a use
   case needs more, it needs a streaming function and a record, not a
   larger number.
4. If a platform rnx builds for has no atomic no-replace rename, or a
   filesystem in the gates refuses the flag, stop and say so; do not fall
   back to check-then-rename.
5. The regular-file check is on the open handle on every platform; if a
   platform cannot report a handle's kind, stop and say so, and do not
   check by path and then open. The open itself is bounded only where
   decision 7 says (the Unix FIFO); an unbounded open elsewhere is stated,
   not a stop.

## Risks

- **Three platform-specific renames.** One primitive each on Linux,
  macOS and Windows, where Rust offered one call. Each is a few lines
  over crates already linked, and gate 6 exercises the refusal on every
  platform the suite runs on; macOS is not in the suite and is stated as
  unexercised, as record 0025 states for every platform claim.
- **A filesystem that does not answer stalls the driver, with no
  deadline.** Decision 7 says so. Local reads are bounded in size and
  refused for special files; time on a hung mount is not bounded by
  anything in rnx.
- **`remove_dir_all` is not atomic against a moving target.** A path
  swapped for a symlink after the guard and before the removal is removed
  as whatever it is by then. Stated; the guard is for a script's mistake,
  not for an adversary with write access to the parent.
- **`modified_ms` is a number a later datetime record has to honour.**
  Milliseconds since the epoch is the least contestable choice; if that
  record chooses otherwise, this field gets a conversion, not a rename.
- **`remove_dir_all` is a loaded gun with three safeties.** It refuses
  root, the working directory, and its ancestors. It does not refuse
  `$HOME`, because a script run from a home directory that computes a
  path to a sibling must not be blocked from removing it; the working
  directory rule already covers the run-from-home case.

## Forward

Record 0031's gate 5 continues with the environment and arguments, then a
path type that gives decision 3's refused names somewhere to go.
