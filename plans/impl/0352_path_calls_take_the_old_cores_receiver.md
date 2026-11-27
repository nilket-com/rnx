# Implementation plan 0352: Path calls take the old core's receiver

Status: Proposed

Paired ADR: ../adr/0352_path_calls_take_the_old_cores_receiver.md.

1. The boundary was reported before this plan: the two kinds of
   pinned combination, what the selected function does on the
   old core, the order of effects and failures, and the stored
   form.
2. The contract of Decision 1, with Controls 1 and 5.
3. Decisions 2 to 6, with Controls 1, 3 and 4.
4. Empty and remove the pinned lists. Add the inputs of Control
   2 to the control.
5. Plant each old behavior back. Replay the public admission
   corpus. Send the list for review before signing if any source
   changes its exit, output or effects.
6. Score the file gate and both project screens. If a row is
   outside its band, stop and send the counts.
7. fmt, Clippy, preflight and the targeted checks in separate
   state. Then the stages of Control 10 by their stage commands.
   Sign. Source review, one complete final, Results.

## Results

The named integer clone and the iterator reductions use the selected
handler's existing registration. For integer clone the lowerer checks
the standard integer module's installation certificate and verifies
that the named alias and the CLONE protocol share a handler allocation.
For sum and product it checks the installed Iterator trait and the
selected specialization's trait metadata. Neither family has a
HostSignature in the host bridge. The repair adds no registration
field and changes no public host ABI.

Of 1,873 installed handler identities, the sweep probed 2,752 argument
positions and admitted 459 discovery callers with 643 positions.
Expanding the input kinds from 43 to 49 raised the admitted
comparisons from 25,738 to 29,596 per native mode. Both modes
completed with zero pinned combinations. Both exception lists were
removed. The sweep recorded 105,270 compile refusals separately.

Restoring the old integer receiver check reproduced its two historical
failures per mode. Restoring the old reduction check reproduced its
420 historical failures plus 252 failures on the expanded inputs
(56 advanced, 84 exhausted, 56 mixed-integer, 28 float and 28 overflow
cases). Each mutation also failed its direct CLI control.

Among the 72 direct and stored CLI comparisons, 26 succeeded and
46 failed on stock. In both native modes the candidate matched the
exit and stdout for all 72, and matched the complete stock diagnostic
and location for all 46 failures. A conversion error and overflow
after the first pull left the cursor's tail item, 37, available to
the caller. The open-range sum exhausted the work allowance and
released its store entries in both modes. Vec::len and f64::clone
retained their nominal receiver checks. The mapped-cursor witnesses
retained their fallback route. The public corpus replay against
published 285862bd found zero changes across 6,404 rows; 67
successful legacy-only cases remained in each native mode.

The initial candidate was stopped by two native-on T setup rows.
Registration widened ContextMeta from 104 to 112 bytes and added a
lookup when forwarding a trait alias. Even a program that returned
42 paid 68,470 extra instructions in the full child. In the measured
setup windows the product instructions were identical, while the
changed allocation history affected the glibc allocation and copy
paths. The repaired candidate matched the published base's call and
self-instruction counts in install_meta, install_associated,
TraitContext::find and function_inner. ContextMeta was again 104
bytes. Relative relocations fell from 31,591 to 31,587. Detail
remained 32 bytes, aligned to 8, and setting None compiled to
one byte store of zero at offset zero on both builds.

The file screen passed all 32 per-record and all 32 cumulative rows.
The anchor lookup missed. The screen built both the published base
and the candidate and the source condition passed. The shared_rows
native-on T setup fell 48 instructions. Permute rose 33,818 against
an allowance of 213,276. Its loader saved 48 instructions and
glibc memcmp accounted for the 33,866 extra inner instructions.
The count of calls from str::contains was 58,036 in each build,
with 64,503 calls to memcmp overall. Product instructions and
execution were identical; alignment versus content was not
established. The empty and paired-adapter projects each passed
all 64 instruction comparisons, including both sizes and both native
modes. Each had 56 exact rows and eight decreases.

The table lists the diagnostic text that changed on inputs failing
on both engines. The count includes both spellings where tested.

| Cases | Count | Published 0351 | Candidate and stock |
|---|---:|---|---|
| float-int | 2 | Expected type `std::object::Iter` but found `std::slice::Iter` | Expected type `std::f64` but found `std::i64` |
| plain-bad | 2 | Expected type `std::object::Iter` but found `std::slice::Iter` | Expected number type, but found `std::string::String` |
| plain-over | 2 | Expected type `std::object::Iter` but found `std::slice::Iter` | Numerical overflow |
| dropped | 2 | Expected type `std::object::Iter` but found `std::slice::Iter` | Cannot write the value: it has been moved |
| negative | 2 | Expected type `std::u64` but found `std::i64` | Failed to convert value `-1` to integer `u64` |
| too-large | 2 | Expected type `std::i64` but found `std::u64` | Failed to convert value `18446744073709551615` to integer `i64` |
| no-next | 2 | Expected type `std::object::Iter` but found `std::vec::Vec` | Missing protocol function `NEXT` for `std::vec::Vec` |
| int-clone-struct | 1 | Expected type `std::i64` but found `Probe` | Expected number type, but found `Probe` |
| sum-struct | 1 | Expected type `std::object::Iter` but found `Probe` | Missing protocol function `NEXT` for `Probe` |
| float-product-struct | 1 | Expected type `std::slice::Iter` but found `Probe` | Missing protocol function `NEXT` for `Probe` |
| int-clone-function, int-clone-closure | 2 | Expected type `std::i64` but found `std::ops::Function` | Expected number type, but found `std::ops::Function` |
| sum-function, sum-closure | 2 | Expected type `std::object::Iter` but found `std::ops::Function` | Missing protocol function `NEXT` for `std::ops::Function` |
| float-product-function, float-product-closure | 2 | Expected type `std::slice::Iter` but found `std::ops::Function` | Missing protocol function `NEXT` for `std::ops::Function` |
| int-clone-unit, int-clone-tuple | 2 | Expected type `std::i64` but found `std::tuple::Tuple` | Expected number type, but found `std::tuple::Tuple` |
| sum-unit, sum-tuple | 2 | Expected type `std::object::Iter` but found `std::tuple::Tuple` | Missing protocol function `NEXT` for `std::tuple::Tuple` |
| float-product-unit, float-product-tuple | 2 | Expected type `std::slice::Iter` but found `std::tuple::Tuple` | Missing protocol function `NEXT` for `std::tuple::Tuple` |
| int-clone-char | 1 | Expected type `std::i64` but found `std::char` | Expected number type, but found `std::char` |
| sum-char | 1 | Expected type `std::object::Iter` but found `std::char` | Missing protocol function `NEXT` for `std::char` |
| float-product-char | 1 | Expected type `std::slice::Iter` but found `std::char` | Missing protocol function `NEXT` for `std::char` |
| int-clone-error-result | 1 | Expected type `std::i64` but found `std::result::Result` | Expected number type, but found `std::result::Result` |
| sum-error-result | 1 | Expected type `std::object::Iter` but found `std::result::Result` | Missing protocol function `NEXT` for `std::result::Result` |
| float-product-error-result | 1 | Expected type `std::slice::Iter` but found `std::result::Result` | Missing protocol function `NEXT` for `std::result::Result` |
| int-clone-iterator-of-tuples | 1 | Expected type `std::i64` but found `std::object::Iter` | Expected number type, but found `std::object::Iter` |
| float-product-iterator-of-tuples | 1 | Expected type `std::slice::Iter` but found `std::object::Iter` | Expected type `std::f64` but found `std::tuple::Tuple` |

Formatting, the manifest's Clippy configurations and preflight passed.
The whole rune-suite, root-tests, cross and final-controls stages passed
through their stage commands on the repaired source. The separate
in-process and CLI controls passed. An initial ad hoc Rune library
test command disabled default features and failed to compile the
test harness. The supported library configuration passed.

The complete final ran on 5a93615e as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
3a6afe66e256613e0e25e9a35634b8f9e5f71876759fad4a6cd040856b01037c.
Entry through cleanup took 3,731.6 seconds against the 5,100 second
limit. The run entered with 161,435,201,536 bytes free against the
151,152,450,560 required, and ran on 12 job tokens. The gate ran from
3,448 to 3,688 seconds. It reused the inputs published with ADR 0351:
the anchor lookup was a verified published anchor and took 0.3 seconds,
and the base was not rebuilt. This is the first final under the reuse
identity of ADR 0351. The Rune suite ended at 2,178 seconds with 921.2
seconds in the contracts. The source condition passed over 40 items with
no transition declared and none needed. All 32 per-record rows passed
and 22 were exact. One native-on T row rose, loops in setup by 119
instructions, 0.02 per mille against the 5 per mille band. The other
nine fell. All 32 cumulative rows passed. The cross stage ran the host
path control in 20.4 seconds with 29,596 sources compared in each native
mode, no pinned combination and none unexpected. The fetch stayed
offline and took 11.8 seconds. The build peaked at 120,542,121,984 bytes
and shared memory at 987,136 bytes. The cleanup cleaned 37 targets and
removed the run. The memory controller reported no events. The run left
no new entry in /tmp or /var/tmp.
