# Implementation plan 0265: a zero-size arena allocation keeps strict provenance

Status: Proposed

Paired ADR: ../adr/0265_zero_size_arena_allocation_keeps_provenance.md.

1. Pin f064fcb9. Read the arena's allocation paths and their callers,
   the Miri receipts of ADR 0264 that failed, and what the pinned
   toolchain offers for a dangling pointer without a cast.
2. Before editing, send a note: the committed case failing under Miri
   with its diagnostic; the proposed construction and why the address
   and alignment are unchanged; the sweep table of integer-to-pointer
   casts with the proposed disposition of each; whether any is inside
   an inventoried item; anything in the ADR the code makes wrong.
3. Fix the arena and the casts the note's ruling puts in scope. Restore
   the two tests of ADR 0264.
4. Commit the inventory.
5. Run Miri on the new inventory and on the 0264 inventory, a final in
   required mode, preflight and the commit checker. Bring the packet
   with the before and after receipts, the sweep table and the
   per-stage durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.

## Source-review evidence

Before-fix checkpoint 980c0394 is unsigned and local, containing only the new
controls and inventory. It will be amended into this record's single signed
implementation after review. MIRI-BEFORE-v2 FAIL as expected: independent
legacy empty-text and execution empty-text cases both stop at arena.rs:169;
legacy integer-pointer-format case stops at runtime/format.rs:548. All three
report integer-to-pointer cast unsupported under strict provenance. Original
receipts and exact diagnostics retained in target/0265/BEFORE-FAILURES.json;
no Miri flags changed. Native baseline 18 controls PASS before production fix.

The arena now constructs the same aligned nonzero address with
ptr::without_provenance_mut(layout.align()), allocating no slab. Direct tests
assert address and alignment for 1/64/4096, repeated empty bytes/text, and
aligned ZST reference/write/slice operations without chunks. Pointer
formatting uses ptr::without_provenance::<()>(n as usize), preserving the
prior address truncation on the target architecture; memory counter's test
sentinel uses NonNull::dangling. Both 0264 refusal tests are restored to empty
text.

Published binary baseline: signed integer pointer formatting 0 => 0x0,  1 =>
0x1, 4660 => 0x1234, i64::MAX => 0x7fffffffffffffff, -1 => 0xffffffffffffffff
on this 64-bit host. All five are checked in the legacy compile-and-run test.
The execution compiler refuses pointer formatting; its control pins that
refusal. Unsigned u64::MAX gives legacy IllegalFormat, and execution refuses
at admission (raw baseline retained); no new unsigned formatting support.

Inventory: one compile per test, separate legacy/execution paths for empty
text/vector/object/tuple/block/no-statement function/constant text; legacy
empty bytes runs, execution empty bytes has an admission-refusal control.
Pointer formatting has one test per path. One direct arena allocation test.
New native arena suite 21 PASS, clippy PASS, root memory control 1 PASS.

### Integer-to-pointer sweep

Baseline raw query (103 lines, retained as target/0265/CAST-SWEEP.txt): ```sh
cast_pattern='as \*(mut|const)|transmute.*(usize|isize)'
cast_pattern+='|with_exposed_provenance|from_exposed_addr' rg -n
"$cast_pattern" rune/crates/rune/src \   rune/crates/rune-alloc/src
rune/crates/rune-core/src src ```

Each cast operand's type was inspected; pointer-to-pointer casts, references
coerced to raw pointers, pointer-to-integer identities and documentary
examples were excluded from the three integer-to-pointer sites below.
Secondary searches covered transmute and raw-pointer casts in local
project/module/native support; no further integer-to-pointer reconstruction
was found there.

| Baseline site | Disposition |
| --- | --- |
| rune/crates/rune/src/hir/arena.rs:169 | Fixed: zero-size dangling address from explicit without_provenance_mut, same alignment/address, no allocation. |
| rune/crates/rune/src/runtime/format.rs:548 | Fixed under step-2 ruling: explicit without_provenance pointer for address-only formatting, never dereferenced. |
| src/memory.rs:137 | Fixed test sentinel with NonNull::dangling; counter only checks nullness. |

None is in the 40 protected fixed-route source inventory. All are ordinary
tracked inputs to their respective package checks. Existing explicit
without_provenance address constructions in Global, hashbrown ZST buckets and
slice iterators are retained: ZST/index/sentinel uses never read nonzero
bytes. Arena::with_addr retains the allocation pointer through
wrapping_offset. Pointer identities/order keys are never reconstructed into
pointers. JIT pointer-to-function transmute is not integer-to-pointer and
native execution is excluded under Miri. Signal handler pointer-to-integer FFI
is outside Miri.

Source review, both full Miri inventories, required final, preflight and
commit checker remain pending. No toolchain or protected executor/policy edit.

Source review seq 31032 accepted, four repairs installed: exact Unsupported+
ByteStr or FormatSpec refusal assertions; width, left/center/right alignment,
custom fill and zero-padding comparisons derived from published output;
tab-indented macro; unrelated Display indentation reverted (not required by
committed preflight/required final). All 23 arena controls PASS; clippy
including tests with warnings denied PASS. Pointer padding output is
preserved, including legacy's unusual `{:010p}` result. New inventory 21
tests. Both full Miri inventories and ordinary required final proceed on the
signed candidate.

First signed candidate passed both full Miri inventories (21 + 52). Required
final v1 stopped at the no-execution rune-clippy configuration: Diagnostics
import in new tests needed the same execution feature gate as its use. That
test-only gate is repaired. The ordinary final is rerun first; both
inventories will then be rerun for the amended tracked inputs. Earlier
receipts retained, not used as matching-input acceptance for the amendment.
