# rnx 0015: the third port

Status: proposed 2026-09-08. The fifteenth record of rnx. Record 0011 asked a
third port to decide whether `join` and `contains` were earned. It decided
against both, for a reason record 0011 did not consider: neither is missing.

## What was ported

`ci/check_blake3_confinement.sh` in the Wendelon crate, 54 lines of Bash. It
greps the source for direct BLAKE3 use, drops the lines that reach it through
the boundary module, reduces the rest to file names, and prints an inventory:
a boundary file, a recorded exception with its reason, or a violation. A
violation goes to standard error and the check exits 1.

It was chosen for being unlike the two before it. Both of those were Python
that read text and printed a report. This one is a different language, and
its shape is the surface records 0012 and 0013 just added: it runs a child
process, writes to both streams, and chooses its exit status.

## How it was compared

`ci/fixtures/blake3_confinement/compare.sh` runs both over five trees: a
fixture with violations, a clean fixture, a tree with no matches, a tree with
no source directory at all, and the crate as it actually is.

| | Result |
| --- | --- |
| Standard output | identical on all five |
| Standard error | identical on all five |
| Exit code | identical on all five |

Standard error is required to match here, where the ported Python scripts only
had their diagnostics recorded case by case. Both implementations run the same
`grep` over the same tree, so every line either writes to standard error is
its own rather than a runtime's, and there is nothing to excuse.

The live case is the one no fixture stands in for: the crate has thirteen real
violations today, and both implementations report the same thirteen.

Three deliberate breakages were checked. Changing an inventory line put three
cases in the standard-output column. Dropping the forwarding of `grep`'s own
complaints failed the missing-directory case on standard error. Dropping the
failing status failed four checks across two cases.

A fourth was found rather than staged. The tree with no source directory is an
empty directory, which git does not carry, so the case would have vanished on
a fresh checkout while both sides failed alike and looked like agreement. The
tree now holds a file explaining why it is empty, and the harness reports a
missing tree as its own failure rather than comparing two errors.

## A defect this port shipped: an incomplete capture read as a clean tree

`host::process` reports whether it captured everything, and the port ignored
it. Reproduced: one file holding a boundary-qualified line larger than the two
mebibyte capture limit, followed by a violation. Both lines are in the same
file, so `grep` emits them in file order and the case does not depend on how a
directory is walked.

| | Result |
| --- | --- |
| The Bash original | reports `src/late.rs` and exits 1 |
| The port, before | prints `BLAKE3 confinement check passed` and exits 0 |
| The port, after | refuses, names the capture limit, and exits 1 |

A violation the port never saw looks exactly like a tree that has none, so
this is the one answer the check must never give by accident. It now refuses
on any of `truncated`, `timed_out`, and `cancelled`, before printing anything
at all: a partial inventory would be as misleading as a clean one.

These are deliberately separate from `grep`'s own exit status. Its ordinary
no-match and cannot-look cases are still passed over, because the original
passes over them; the three flags say something different, which is that the
output rnx holds is not the output `grep` produced.

Three cases drive them, and each has a control. Truncation uses the tree
above, and the harness first asserts that the original does report the
violation, so the case cannot pass by there being nothing to find. The timeout
and the interruption both use a named pipe nothing writes to, which makes
`grep` block; the timeout case runs a copy of the port differing only in the
timeout constant, and the harness fails if that constant moves. Removing each
guard fails exactly its own case.

## The verdict on record 0011's fourth finding

Record 0011 reported that Rune has no `join` and no `contains` on a sequence,
and asked a third port to confirm them before rnx added either. The third port
hand-wrote `contains` again, which is the evidence record 0010's guardrail
asks for.

**Neither is added here, and the reason is narrower than it first looked.**
Both are one line of Rune:

| Wanted | Written as |
| --- | --- |
| `contains(items, wanted)` | `items.iter().any(\|x\| x == wanted)` |
| `join(parts, separator)` | `parts.iter().enumerate().fold("", \|a, p\| if p.0 == 0 { p.1 } else { a + separator + p.1 })` |

The methods do not exist. The capabilities do. Record 0011 measured the first
and concluded the second, and that is the third time this project has made
exactly that error: record 0008 did it twice, with `char::is_numeric` and with
`split` taking a character predicate.

What that settles is only the argument record 0011 actually made, which was
that rnx should supply these because Rune cannot express them. It can, so that
argument fails. It does not follow that a named helper has no value, and the
trap below is a reason to think it might: a helper that is right once is worth
more than an idiom every caller has to get right. This record defers rather
than refuses, and a fourth port that writes the fold again, or writes it
wrong, is the evidence that would settle it the other way.

Both ports now use the idioms. The 22 hand-written lines are gone from the
second port and the 10 from the third, and rnx gains no module, no
registration, and nothing to maintain today.

### The trap that is real, and worth the comment

The fold that looks like `join` is wrong:

```rune
parts.iter().fold("", |acc, x| if acc == "" { x } else { acc + sep + x })
```

It treats an empty accumulator as "nothing yet", so a leading empty part loses
its separator. Measured: `["", "a", "b"]` joins to `a-b` rather than `-a-b`,
and `["", ""]` joins to the empty string rather than to `-`. The version with
`enumerate` is right on both, and on the report the second port builds, which
really does contain blank lines.

That is the one thing worth carrying forward. If `join` is ever added to rnx,
the reason will be that the obvious spelling is subtly wrong, not that no
spelling exists.

## What the exercise exposed

### 1. A child's output is decoded lossily, and silently

`host::process` renders a child's bytes with `String::from_utf8_lossy`. Output
that is not UTF-8 comes back with replacement characters and nothing says so,
where `host::read` and `host::stdin` both refuse. A script cannot tell a child
that printed a replacement character from one whose output was mangled.

That is a defect in what exists: the other two readers already decided this
the other way.

### 2. A child's bytes cannot be read at all, which is a different thing

Deciding finding 1 the strict way would make corruption visible. It would not
make the corrupted case work. The remaining Python in the repository is a git
lineage verifier that reads raw tree objects, where a twenty byte object id is
not text and never will be; refusing to decode it is as useless to that script
as mangling it.

So there are two requirements, and the next cut should not confuse them:

| Requirement | What it buys |
| --- | --- |
| Refuse output that is not UTF-8 | a mangled capture stops being silent |
| Return a child's bytes | a script that reads binary output becomes possible |

The first is a correction. The second is a capability, and it is the one a
real script is blocked on.

### 3. There is no way to list a directory

`host::` reads a path and runs a program, and cannot enumerate. This port
shells out to `grep`, which is what the original does, so nothing here is
blocked. A script that wanted to walk a tree itself could not.

### 4. `position` is missing where `any` is not

`iter().any` works and `iter().position` does not. Noted rather than acted on:
no port has needed it.

## What it cost

| Point | Lines |
| --- | --- |
| The Bash original | 54 |
| The Rune port, with the hand-written helper | 98 |
| The Rune port, using the idiom | 87 |

1.6 times the original. Bash is terse for exactly this shape, and most of the
difference is that the port says out loud what the pipeline leaves implicit.

The second port went from 461 lines to 464 rather than shrinking, because the
idiomatic `join` spread across more lines than the loop it replaced. The win
was not line count. It was that rnx does not grow a module and the trap above
is written down at the one place it matters.

## Both originals are kept

Neither port replaces anything. The Bash script is still what CI runs, and the
comparison is what keeps the two honest.

## Forward

Record 0011's last suggestion, a grouped float, which is a formatting question
rather than a helper. A child's bytes, which finding 2 makes the first
thing a real script is blocked on, with finding 1 beside it as the smaller
correction. And a fourth port, which by now has a clear job: to
find something none of the three has.
