# rnx 0036: what the process was given

Status: implemented 2026-09-13 after review. Linux gates and measurements
are in the accompanying evidence. The environment module type-checks for
Windows; Windows execution remains outstanding. The
thirty-sixth record of rnx, and record
0031's gate 5, second part: the script's arguments and the process's
environment. Both are things the process was handed before the script
ran, both can hold bytes that are not Unicode, and rnx's rule for such
bytes — record 0035's — is that they are refused by name, never converted
into something that looks like text. This record is read-only on purpose,
and it fixes one panic on the way.

## Context

A file's `main` has taken the arguments since record 0009: everything
after the path, verbatim, as a `Vec` of `String`, so that an argument that
reads like a flag reaches the script instead of changing rnx. Measured
today:

```
rnx run args.rn one "two words" --flag     → ["one", "two words", "--flag"]
```

Measured today as well, with an argument whose bytes are `FF FE`:

```
thread 'main' panicked at library/std/src/env.rs:878:51:
called `Result::unwrap()` on an `Err` value: "\xFF\xFE"
```

exit status 101. `main.rs` collects argv with `std::env::args()`, which
the standard library documents as panicking on an argument that is not
valid Unicode. Nothing in rnx decided that; it is a default, and a
backtrace notice is not the answer rnx gives to bad input anywhere it has
decided. This record replaces it with an escaped refusal.

The environment is untouched by scripts today. rnx itself reads two
variables a user may set, `RNX_HISTORY` and `RNX_MEMORY_CEILING`, plus
`RNX_TEST_*` under `test-support`, and `HOME`, `XDG_STATE_HOME` and
`LOCALAPPDATA` to find its state directory. A script cannot read any of
them, or `PATH`, or the variable that tells it which account to use. That
is the gap record 0031 named, and Codex's brief for this record asks four
things: trace what `main` receives and make any accessor agree with it;
tell missing, empty and non-Unicode apart and refuse lossy conversion;
start read-only; and gate with a controlled environment and explicit
argv, never the developer's shell.

## Decision

### 1. One module, `env`, read-only

```
env::args()      -> Vec<String>
env::var(name)   -> Result<Option<String>>
env::vars()      -> Result<Object>
env::home_dir()  -> Result<Option<String>>
```

Four functions and no setter. Rust 2024 made `std::env::set_var`
`unsafe`, and its safety contract is platform-shaped: on Windows the
call is permitted from any thread, while on Unix it is undefined
behaviour if any other thread may be reading the environment at the same
time, because the C library's `getenv` and `setenv` share one table with
no lock a caller can take. rnx is one program on both platforms, and it
does have other threads at times — a child's capture threads under
`host::process`, whatever a tokio runtime has spawned — so a setter that
is sound on one platform and a data race on the other is not one
contract, and the record does not offer it. A process-wide mutation from
a script would also change what `host::process` children inherit, which
is a contract those records did not sign. A script that wants a child to
see a different variable wants an option on `host::process`, and that is
a later record's addition to that function, not this module's. If a use
case for changing rnx's own environment appears, it gets a record that
says on which thread, with which others stopped.

### 2. `args` is exactly what `main` was given

`env::args()` returns the same arguments `main` received, in the same
order, element for element. The command line is parsed **once**, in
`main.rs`, into an immutable host-side snapshot; `main`'s argument and
every call to `args()` are each built from it as an **independent Rune
value**. Rune vectors are mutable, so this is the only arrangement in
which one parse does not become shared mutable state: a script that pops
from `main`'s vector, or from one `args()` result, changes nothing the
next call sees. Under `run` the snapshot is everything after the file
path. Under `eval` and in a session there is no
file and no argument list, and `main` is rnx's generated wrapper, so
`args()` is empty; a session is not a command line. The record does not
add an argument syntax to `eval`, because `eval` takes one expression and
that is the whole of its contract.

An argument that is not valid Unicode is **refused before anything runs**,
by rnx and not by the standard library: argv is collected as OS strings,
and the first one that is not Unicode ends the command with

```
rnx: argument 2 is not Unicode: "\xFF\xFE"
```

Positions count from the command word as argument 1 — `run` in `rnx run
file.rn x` is 1, the file 2, `x` 3 — so the number in the message is the
one a person counting the words they typed after `rnx` arrives at. The
executable's own name, `argv[0]`, is not numbered and not decoded: rnx
never reads it, so a non-Unicode `argv[0]` refuses nothing.

exit status 2, with the bytes escaped the way record 0009 escapes
everything rnx prints. Measured today, rnx exits 2 for an unknown command
and 1 for a missing file or a bad flag value; a bad argument is refused
before a command is chosen at all, which is the unknown command's
situation, so it takes that status. The other two are not reconciled
here; that is record 0013's territory and this record names the
inconsistency rather than widening it. That is every argument
after `argv[0]`, including the command word and the file path, because
`std::env::args` was refusing all of them by panicking and this is the
same refusal with a sentence. There is no bytes form of `args`; a script that must accept
arbitrary bytes on the command line is record 0031's later bytes-path
concern, and today it is refused rather than mangled.

### 3. `var` tells missing, empty and unreadable apart

| the environment has | `env::var(name)` returns |
| --- | --- |
| no such variable | `Ok(None)` |
| the variable, empty | `Ok(Some(""))` |
| the variable, valid Unicode | `Ok(Some(value))` |
| the variable, not Unicode | `Err`, naming the variable and the escaped bytes |
| — and `name` is not a valid name | `Err`, naming the name and why |

`None` and `""` are different answers because they are different states,
and every shell and every language that collapses them has a bug report
about it. The refusal for a value that is not Unicode says which variable
and shows the bytes escaped; a script that needs those bytes has no way
to get them in this record, and gets a sentence instead of a string with
U+FFFD in it.

A name is valid when it is not empty and contains neither `=` nor NUL.
The standard library treats such a lookup as "not present", which would
report a malformed name as a missing variable; rnx refuses it instead,
because `env::var("A=B")` is a script's mistake and a mistake is named.
Nothing else about names is checked — case, spaces, unusual characters —
because the OS decides what a name is, and on Windows the lookup is
case-insensitive as the OS makes it, stated and not adjusted.

### 4. `vars` is the whole environment or a refusal

`env::vars()` returns an object, name to value, of every variable. If any
value or **name** is not Unicode the whole call is refused naming that
entry — the name escaped if the name is the problem — for the reason
`read_dir` refuses a whole listing in record 0035: a script must not
receive most of the environment and silently miss one entry. A script
that wants one variable in an environment that has a bad one uses `var`.

On Unix the process environment is a list, not a map, and can hold the
same name twice; `getenv`, and so `var`, returns the first. `vars` keeps
the first occurrence too, so the two cannot disagree about a name. "Any
value" in the paragraph above means any entry in the list, not any entry
that survives deduplication: a duplicate whose first value is fine and
whose second is not Unicode makes `vars` refuse while `var` still returns
the first, and both are gated. The gate builds the child's environment
by hand, because it is exactly the kind of thing nobody tests and
everybody assumes.

### 5. `home_dir` is the one platform-dependent lookup this record makes

A script that wants the home directory writes `var("HOME")` on Unix and
`var("USERPROFILE")` on Windows, which is the difference record 0025
exists to remove. `env::home_dir()` is the standard library's, and that
is more than a variable lookup, so its semantics are stated: on Unix,
`HOME` if it is set **and not empty**, otherwise the account database
through `getpwuid_r`, which can consult a name service and so can block
for as long as that service takes; on Windows, `USERPROFILE` if it is
set **and not empty**, otherwise the profile directory from the
user-profile API — the same rule on both platforms, an empty variable
counting as absent. `Ok(None)`
when neither source answers, `Err` naming the source when what it says
is not Unicode — and the fallback can say something that is not Unicode
just as the variable can, so the record does not promise `Ok` when the
variable is unset. Record 0035 deferred it here; nothing else path-shaped
is added, and the path helpers are their own record.

### 6. What rnx reads for itself is documented, and stays rnx's

The README gains a section listing the variables rnx reads directly —
`RNX_HISTORY`, `RNX_MEMORY_CEILING`, and the state-directory lookups
`HOME`, `XDG_STATE_HOME` and `LOCALAPPDATA` — with the test-only
`RNX_TEST_*` family named as existing and reserved, and, separately, the
variables read on rnx's behalf by a dependency: the proxy configuration
reqwest honours for record 0034's `http::`, in both letter cases. The
list is scoped to what is read, not to what might be. Nothing in rnx
detects a dependency starting to read a new variable — record 0029's
workflow checks licences and notices, not behaviour — so the list is
maintained by hand at each dependency change, and the record says so
rather than claiming a check that does not exist.
Nothing in `env::` changes what rnx does with them, and a script that
reads `RNX_MEMORY_CEILING` sees a string like any other; the record 0005
rule that no host function sets the ceiling is untouched because there
is no setter.

### 7. What this record does not decide

- Setting or removing a variable, in rnx or for a child: decision 1.
- Arguments for `eval` or a session, or a bytes form of `args`.
- The current executable's path, the process id, and the user's name:
  small, but each is its own platform question, and none has a use case
  in front of it.
- Path helpers.

## Acceptance gates

Every gate spawns rnx with `env_clear()` and an explicit set of
variables, and with an explicit argv, so the developer's shell can neither
pass nor fail one. A gate that needs a variable rnx itself reads sets it.

1. **`args` agrees with `main`, and neither is the other's storage.** A
   file whose `main(a)` returns `(a, env::args())` shows the two equal for
   `[]`, `["one"]`, `["two words", "--flag", "run"]` and an argument that
   is the empty string; under `eval` and in a session `env::args()` is
   `[]`. A `main` that pops from `a`, then pops from one `args()` result,
   then calls `args()` again, gets the full list from the last call; a
   `main` that pushes onto a string inside `a` finds the same string
   unchanged in the next `args()` result, because the promise is
   independent values, not merely independent vectors.
2. **A bad argument is a sentence, not a panic.** With bytes `FF FE` as
   the third word after `rnx` (Unix, `run file.rn <bad>`), rnx exits 2,
   writes the refusal in decision 2 to standard error naming position 3
   and the escaped bytes, and writes nothing to standard output; the same
   with the bytes as the file path (position 2) and as the command word
   (position 1). A non-Unicode `argv[0]` (Unix, `exec` with a chosen
   name) refuses nothing and the command runs. No `panicked` and no
   `RUST_BACKTRACE` appear anywhere in the output.
3. **`var` distinguishes three states and refuses two.** Against a child
   environment of exactly `{A: "1", E: ""}`: `var("A")` is `Some("1")`,
   `var("E")` is `Some("")`, `var("M")` is `None`; with `B` set to bytes
   `FF` (Unix), `var("B")` is `Err` naming `B` and `"\xFF"`; `var("")`,
   `var("A=B")` and `var("A\0")` are `Err` naming the name; on Windows,
   `var("a")` finds `A`.
4. **`vars` is whole or refused.** Against `{A: "1", E: ""}`, `vars()` is
   exactly those two; with `B` set to a non-Unicode value (Unix), `vars()`
   is `Err` naming `B` and `var("A")` still works; with an entry whose
   **name** is not Unicode (Unix, hand-built envp), `vars()` is `Err`
   showing the escaped name. With an environment built by hand (Unix,
   `execve` from the test's pre-exec hook) listing `D=first` and
   `D=second`, `var("D")` and `vars()["D"]` are both `"first"`; listing
   `D=first` and then `D=<FF>`, `var("D")` is `"first"` and `vars()` is
   `Err` naming `D`.
5. **`home_dir` is the platform's, fallback included.** With `HOME` set
   to a fixture directory (Unix) or `USERPROFILE` (Windows), `home_dir()`
   is `Some(that)`; with `HOME` set to a non-Unicode value (Unix) it is
   `Err` naming the source; with `HOME` **empty** (Unix) the result is
   the account database's answer, compared against `getpwuid_r` called
   by the test itself, so the fallback is asserted and not assumed; with
   `HOME` unset the result is reported as whatever it is, `Ok` or `Err`,
   because the fallback may itself be non-Unicode and the record does
   not promise otherwise.
6. **The panic is gone everywhere.** A bad argument to `eval`, `repl`,
   `help` and to an unknown command each get the decision 2 refusal, so no
   entry point still reaches `std::env::args`.
7. **Read-only, and rnx's own variables untouched.** `env::` lists exactly
   four names and no setter; `RNX_MEMORY_CEILING` set in the child
   environment still bounds the session as record 0005 says, and a script
   reading it gets the string; both suites, the startup measurements and
   the session baseline before and after.

## Guardrails and stop conditions

1. No lossy conversion, anywhere: not in `args`, `var`, `vars` or
   `home_dir`. If a platform cannot report the raw bytes of a variable
   that failed to decode, refuse without them and say so.
2. No setter, and no function that changes what a child inherits.
3. One parse of the command line, held immutably on the host side;
   `args()` and `main`'s argument are independent values built from it.
   If keeping one snapshot proves impossible, stop.
4. If the standard library's argument iterator cannot be replaced with
   the OS-string one at every entry point, stop and say which.

## Risks

- **The duplicate-name gate needs a hand-built `execve`.** A few lines of
  `libc` in a Unix-only test. If it proves unreliable, the decision stands
  and the gate is stated as unexercised, not deleted.
- **`home_dir` differs by platform in what it consults.** The record
  hands that to the standard library and gates only what is set.
- **A script that used to see a panic now sees exit 2.** The status and
  the message both change for that input; the record does not claim
  nothing observed the old behaviour, only that the new one is a
  sentence.
- **`home_dir` can block.** The Unix fallback consults the account
  database, which may be a network service. Stated in decision 5; a
  script that must not block reads `var("HOME")` and decides for itself.

## Forward

Path helpers and a path type that gives refused names somewhere to go,
then an option on `host::process` for a child's environment if a use
case asks for one.
