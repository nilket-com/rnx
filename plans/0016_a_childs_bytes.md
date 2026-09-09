# rnx 0016: a child's bytes

Status: proposed 2026-09-09. The sixteenth record of rnx. Record 0015 found
that `host::process` decodes a child's output with `from_utf8_lossy`, so
mangled output is silent, and separately that no interface returns a child's
bytes at all. Those are two contracts and this record decides both, because
shipping the first without the second would leave a script no way forward.

## Context

`host::read` and `host::stdin` refuse input that is not UTF-8. `host::process`
replaces it and says nothing. A script cannot tell a child that printed a
replacement character from one whose output was mangled on the way in.

Making that strict is a correction, and on its own it helps nobody who
actually has bytes. The remaining Python in the repository is a git lineage
verifier that reads raw tree objects, where a twenty byte object id is not
text and never will be. Refusing to decode it is as useless to that script as
mangling it. Record 0015 wrote the two down as separate requirements:

| Requirement | What it buys |
| --- | --- |
| Refuse output that is not UTF-8 | a mangled capture stops being silent |
| Return a child's bytes | a script that reads binary output becomes possible |

Rune has a byte string. `::std::bytes::Bytes` supports indexing, length,
comparison, `String::from_utf8`, and conversion to a sequence of integers, so
a script given bytes can do real work with them.

## Decision

### 1. `host::process` refuses output that is not UTF-8

It names which stream and points at the interface that can read it, in the
shape every other host error uses. A script that wanted text and got bytes is
told, rather than handed a plausible-looking string with the evidence
replaced.

### 2. Truncation is not an encoding error

The capture stops at a fixed size, which can fall in the middle of a
character. The last bytes of a truncated capture are then an incomplete
sequence, and that is the capture's doing rather than the child's.

So an incomplete sequence at the very end of a **truncated** capture is not an
encoding error: the partial character is dropped and the capture is reported
as truncated, which record 0015 already made impossible to ignore. Anything
else is an encoding error, including an incomplete sequence at the end of a
capture that was **not** truncated, because there the child really did stop
mid-character.

This is the difference between `valid_up_to` with no error length, which means
the input ended early, and an error with a length, which means the bytes are
wrong where they stand.

**Each stream is judged against its own truncation.** The two are captured
separately and fall short separately, so only a stream's own flag can excuse
that stream's last character. Combining them before decoding would let a
truncated standard output excuse a standard error the child ended
mid-character and that was captured whole, which is the same silent dropping
this record exists to remove. The `truncated` field a script sees is still
the two together, because a caller checking it wants to know that something
fell short rather than which half did.

### 3. `host::process_bytes` returns the streams as they came

The same arguments, the same timeout, the same record, with `stdout` and
`stderr` as byte strings and no decoding of any kind. Every other field means
what it means for `host::process`, including `truncated`, which is still the
script's to check.

Two functions rather than one flag, because the return type differs and a
script knows at the call site which it wants. `host::process` stays the one a
script reaches for; the byte one is for when bytes are the point.

### 4. `Bytes` joins the missing-method map

Record 0014 names a missing method by hashing candidates against the instance
type, for the types a script commonly calls methods on. A script handed bytes
will call methods on them, so `Bytes` belongs in that map. Without it the
first mistake anyone makes with this new type reports a hash.

### 5. What this record does not decide

Reading a file as bytes, which no script has asked for; `host::read` still
refuses what it cannot decode. Writing bytes. Streaming a child rather than
capturing it. The capture limit itself, which stays where record 0015 left it.

## Acceptance gates

1. **Output that is not UTF-8 is refused.** A child emitting invalid bytes on
   standard output fails the call, and the message names the stream and the
   interface that can read it. The same for standard error.
2. **A truncated capture is not called an encoding error** when its only fault
   is an incomplete character at the end. The capture comes back truncated,
   with the partial character dropped, and the text before it intact.
3. **A capture that was not truncated is refused for the same tail.** The two
   cases differ only in whether the capture hit its limit, and they must not
   behave alike.
4. **Bytes that are wrong where they stand are refused even when truncated.**
   Truncation excuses the end of a capture, not the middle of it.
5. **One stream's truncation does not excuse the other.** A truncated standard
   output with a standard error the child ended mid-character is refused, and
   so is the reverse, with the fault and the excuse swapped. A stream whose
   own capture was cut is still excused, so the pair is not satisfied by
   refusing every truncated capture.
6. **`host::process_bytes` returns the bytes exactly.** A child emitting every
   byte value gets them back unchanged, compared byte for byte.
7. **It reports the same outcome fields.** The exit code, the timeout, the
   cancellation, and the truncation are what `host::process` reports for the
   same child.
8. **A missing method on `Bytes` is named**, not hashed.
9. **Both are discoverable.** Completion lists them and `:help` describes them
   from their registration, and each description names the result it returns.
10. **The three ports are unaffected.** Their comparisons pass unchanged; none
   of them reads bytes.
11. **Nothing regresses.** The gates of records 0002 through 0015 pass.

## Guardrails and stop conditions

1. No lossy decoding anywhere in the host. If bytes cannot be text, a script
   is told or handed the bytes; it is never handed a guess.
2. The byte interface decodes nothing, including on standard error. A
   diagnostic that is not text is still the child's own.
3. If refusing turns out to break a script that was relying on replacement,
   that is recorded rather than softened.

## Risks

- **Two functions that differ in one field invite drift.** They share
  everything but the final conversion, and gate 6 compares their reports for
  the same child.
- **A script reaches for the byte interface out of caution and then has to
  decode by hand.** That is the right way round: the cost falls on the script
  that has bytes rather than on every script that has text.

## Forward

The git lineage verifier, which is the script this unblocks and which decides
whether a byte string is enough to work with; reading a file as bytes, if that
port asks for it; and record 0011's last suggestion, a grouped float.
