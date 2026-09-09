# rnx 0017: the fourth port

Status: proposed 2026-09-09. The seventeenth record of rnx. Record 0015 found
a script that could not be ported at all, record 0016 gave it what it needed,
and this is that script. It is the first port to read bytes, and the first
whose original could not be run on its own inputs when the port began.

## What was ported

`plans/product/migration/verify_nilket_graft.py`, 128 lines. It checks that a
prefix-rewritten git lineage says exactly what its frozen source said: the
same commits under a commit map, the same parent topology once mapped, the
same metadata apart from signatures a rewrite invalidates, the same messages,
and every tree wrapped under a single `nilket` entry.

It reads raw git objects. A commit is mostly text; a tree is not, because it
holds object ids as twenty raw bytes. That is why record 0015 recorded it as
blocked and record 0016 named it as the thing being unblocked.

## Half the inputs are gone, so the fixture builds a lineage

The commit map this migration produced **is** kept, at
`plans/product/migration/0001_nilket_commit_map.tsv`, and it holds 1,346
mappings. Its target is this repository: every `new` commit in it resolves
here. The frozen source repository is what is gone, and its objects are
nowhere.

An earlier draft of this record said both were lost. That was a search that
did not go deep enough rather than a fact, and the correction matters: what
cannot be run is the comparison against the source, not the map.

So the original has not been runnable on real inputs since the freeze, and
there was nothing to run it against here.

`fixtures/graft/build.sh` builds a lineage with exactly the relationship the
verifier checks: three source commits, a target whose every tree wraps the
source tree under `nilket`, matching metadata, mapped parents, and a commit
map between them. The tip of the source is rewritten by hand to carry a
`gpgsig` header, so the case where a signature is dropped is reached without
needing a key.

The build is deterministic, which is what lets a diagnostic naming a commit be
recorded rather than matched loosely.

## How it was compared

| | Result |
| --- | --- |
| Standard output | identical on all nine cases |
| Exit code | identical on all nine |
| The port's diagnostics | as recorded, per case |

Eight are built: the verified lineage, wrong arguments, a message the rewrite
did not preserve, a wrapper tree under the wrong name, a rewritten commit that
carries a signature, a commit map missing an entry, and the same map written
with tabs and with carriage returns. Removing the message comparison, the tree
check, or the signature check each fails exactly its own case, and splitting
the map on spaces alone fails exactly the tab and carriage-return pair.

A killed invocation is reported as a timeout rather than compared, because two
sides that both time out would otherwise look like agreement. Driving the port
through a wrapper that sleeps past the limit fails every case.

The ninth uses the real commit map and this repository as the target, with a
source that no longer exists. **It proves one thing: that a missing source is
refused, and that both implementations refuse it alike.** It is not a check of
the map or of the target, because the port resolves the source path before it
opens the map or looks at the target, so neither is ever read. The harness
fails if the map stops being there, which keeps the case from quietly becoming
a test of nothing, and that is the whole of what it is worth.

The diagnostics are recorded rather than compared, because the original raises
and the port explains. Where the original ends with `AssertionError` and a
commit hash, the port says `the message differs at <hash>`, and for the tree
case it says which relationship failed.

## What the byte interface proved

It works. A script can index a byte string, compare two of them, build one
with `Bytes::new` and `push`, and decode a range it knows to be text. Every
byte-level thing this verifier does was expressible.

What it cost is five helpers the script wrote by hand:

| Helper | Lines | What it does |
| --- | --- | --- |
| `slice` | 9 | a range of a byte string, which `get` does not offer |
| `index_of` | 10 | the first occurrence of a byte |
| `header_end` | 10 | the first occurrence of two newlines |
| `line_starts_with` | 14 | whether a range begins with given text |
| `hex` | 11 | twenty raw bytes as forty characters |

Fifty-four lines, and `slice` is the one the rest are built on. Record 0015's
correction applies here rather than its conclusion: these are expressible, so
nobody is blocked, and that is not the same as their being free. A second
script that reads bytes is what would decide whether `Bytes` wants a slice.

## A trap this port fell into

A commit stores an object id as forty ASCII characters. A tree stores it as
twenty raw bytes. The port hex-encoded both, and reported a parent as eighty
characters of hex-of-hex before anything else went wrong.

The original does not make this mistake because Python's byte strings decode
where it says `.decode()` and stay bytes where it does not, and the two sites
read differently. In Rune both are a byte string and both are sliced the same
way, so the difference lives in the reader's head. It is worth writing down
because the next script that reads a git object will meet it.

## What is still missing

### 1. A child that stays open

The original runs `git cat-file --batch` once per repository and streams every
object through it. rnx runs a child to completion, so the port runs one
`git cat-file` per object.

| | git invocations |
| --- | --- |
| The original | 6 |
| The port | 12 |

Twice, on three commits. It grows with the lineage rather than with the number
of repositories: the real commit map carries 1,346 commits, which is about
four thousand invocations against six.

What that costs, measured on this repository rather than guessed: a single
`git cat-file` takes 4 ms and a `rev-list` over the whole history 9 ms, and
the port's twelve invocations on the built fixture take 74 ms altogether. Four
thousand of them would be seconds rather than minutes. The shape is wrong
before the cost is, and this is the finding that a fifth port reading many
objects would make hurt.

### 2. The ninety-second deadline is not a limitation here

`host::process` refuses a deadline above 90000 ms, and the port asks for
ninety seconds. An earlier draft of this record called that a limit a real
lineage would run into.

Measured, it is not. The ceiling is **per child invocation**, and every
invocation this port makes is one small `git` command: 4 ms for a `cat-file`,
9 ms for a `rev-list` over the whole of this repository's history. A longer
lineage means more invocations, not longer ones, so nothing here approaches
ninety seconds and splitting the work would not be forced.

The ceiling is still there and still worth knowing about. It is not this
script's problem, and saying it was is the kind of claim that should have been
measured before it was written.

## What it cost

| Point | Lines |
| --- | --- |
| The Python original | 128 |
| The Rune port | 309 |

2.41 times, the worst of the four ports, and 54 of the 181 extra lines are the
byte helpers above. The rest is what the other ports also paid: explicit
returns where Python has an `assert`, and a diagnostic where Python has a
traceback.

## Both originals are kept

Neither implementation replaces the other. The original is the comparison, and
the comparison is what keeps the port honest.

## Forward

A second script that reads bytes, which decides whether `Bytes` wants a slice;
a child that stays open, which is the shape finding 1 says is wrong; and
record 0011's last suggestion, a grouped float.
