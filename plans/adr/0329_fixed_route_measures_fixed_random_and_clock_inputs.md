# ADR 0329: Fixed-route measures fixed random and clock inputs

Status: Proposed

Base: published 4f0195becab680abb7e6caddc0be4b6fdb629eec (ADR 0328).

## Problem

The scratch-space candidate's third complete final failed fixed-route
on native-on `numeric_calls` setup growth: +11 against an allowance of
5. The same production code had passed in the previous final. The
observer argv, cwd, ordered environment and fixture matched 0328's for
the failing cell. The independently built image hashes differed.

All 11 instructions are in Cranelift 0.135.5 `VCodeConstants::insert`.
Its u64 constant map uses std `RandomState`, which is seeded once per
thread from `getrandom`. With some seeds two constants share a hash tag
and cost one extra key comparison. Changing only those keys on one
retained binary reproduces exactly +11 with identical execution.

The measured setup also reads other per-process inputs: the hashes for
Cranelift `MemFlagsData` and module symbol strings, and monotonic clock
reads for Rune's `analysis_ns`, `compile_ns` and `load_ns`. The gate
compares 5-instruction cells across processes that each draw fresh
values. It measures those values as well as the work.

## Decision

1. The fixed-route observer supplies measured processes with fixed
   random keys. Use the existing GDB/vgdb observer, armed from the first
   client stop and covering libraries loaded before Rust's first use.
   At the real libc `getrandom` call that seeds `RandomState`, verify
   the calling image and the thread-local initialization stack, let the
   call return, then overwrite its 16 bytes with the declared key pair
   before the caller resumes. Other entropy requests are left alone.
   Account for entries and successful returns per guest thread and std
   copy. Record which artifact supplied the caller and verify the bytes
   before resuming. Demonstrate the monolithic CLI and the installed
   image on fd 9 with `glibc.rtld.nns=1`. The helper is C. Exercise
   the installed management CLI separately. A missing expected init,
   short read, or unrecognized Rust seed path invalidates the capture.
   The same rule covers glibc's allocator key in the scored outer
   launcher: `__ptmalloc_init` makes a raw 8-byte `getrandom` before
   Rust starts, and a rejected word takes a clock-based retry (+46).
   This key protects the heap, so the observer doesn't write it. At
   that read's verified libc identity, call site, length and flags, it
   checks that the real read succeeded and that glibc took the
   accepted-key path. A capture that took the retry is invalid, with
   the reason recorded. The real syscall and validation stay charged,
   and the stack query exclusion must still reconcile. A bypassed,
   missing or wrong-site read also invalidates the capture.
2. Guest monotonic time is fixed as well. Inventory each clock ID,
   caller (including the vDSO), phase and per-thread order. Supply a
   declared epoch and tick on successful guest reads, consistently
   across start and elapsed pairs. Host timeouts and the debugger's own
   clocks stay real. Keep the real calls and their instructions in the
   measured window. Supply each successful read once even if a libc
   call passes through the vDSO. Bind the timeline to the guest clock
   domain and verify ordering wherever timestamps are compared.
3. Before freezing, run an input census over the scored workload:
   loader, image, heap, stack and JIT mappings across independent
   launches, ASLR and `AT_RANDOM`, pointer-keyed constants, allocator
   layout, PID and TID, scheduling, and CPU time instructions where
   reached. Classify each source as bound, outside the window, or read
   without affecting counts, with a perturbation witness for that last
   class. An out-of-window read whose value reaches measured code must
   still be traced. Stop if a source could affect counts and the census
   cannot establish its disposition.
   The guest PID is such a source: the installed outer window's stack
   query reads `/proc/self/maps`, and a PID crossing from 99 to 100
   costs 4 more instructions, so the whole capture and its isolated
   query no longer reconcile exactly. Both installed outer captures run
   with the same guest PID in fresh private PID namespaces. The
   observed PID is verified and recorded as part of the method, exact
   query reconciliation is kept, and a crossed-PID pair remains as a
   negative control.
   Host files read inside a scored window are inputs too. The installed
   outer loader consults `/etc/ld.so.cache`; adding two unrelated
   entries cost +224 instructions. Both outer namespaces get a retained,
   known-good cache bound read-only, its SHA-256 in the receipt, the
   capture key and the frozen outer reference, and a mismatch is
   rejected before comparison. The loaded loader and library identities
   are verified, and `/etc/ld.so.preload` must be absent. An alternate
   valid cache is a control. Further host inputs of this kind found by
   the census are bound the same way and recorded in Results. Stop
   only for a source that can't be bound without changing the product
   or the measured work.
4. The shipped binaries do not change. There is no global entropy or
   wall-clock override, and host ASLR stays on.
5. Freeze the method, seed pair, epoch, tick and fixture list before
   reference or acceptance counts. Bind the observer implementation
   and those parameters to the capture key and receipt.
   References measured without them are re-measured under the same
   method, and the transition deltas are disclosed. Bands and
   allowances are unchanged.

## Controls

1. Repeatability: the same ELF hashes, two fresh processes per seed,
   fixture and size, both roles and native modes. Compare repeats at
   the same seed and size. Native-on setup and execution each require
   identical per-function instruction counts (product, libc and other);
   compare native-off's unsplit vector and the applicable outer totals.
   Check guest stdout, stderr and exit separately from debugger and
   Valgrind logs. Cover sizes 1 and 5 independently.
2. A second seed through the same path gives different derived hash
   state at the u64 map and identical output. The forced-collision
   +11 pair remains as a sensitivity control.
3. A valid start/elapsed pair spanning the setup/execution boundary
   uses one timeline and passes. Plant a missing seed hook, a
   misattributed write and a clock reset at the split; each must
   invalidate its capture. A planted real-work change still fails
   the cost gate with the observer active.
4. Keep the failed scratch FINAL-v3 receipt under its original name.
   Rebase the parked scratch successor `d92bec4b` as 0330 after this
   record publishes. Its final runs on that successor.
5. Finals: targeted pre-sign checks under the final's real recipe,
   with every measured delta checked against its band, then one
   complete final on the signed candidate.

## Not in this record

Build reproducibility. Independent builds of the same source differ:
the build identity inherits the enclosing checkout's HEAD, and random
nested build paths reach `.rodata`. Build reproducibility follows the
0330 scratch work as 0331, before the 0332 `dbg!` record.

## Constraints

- No product change. No new third-party crate. Tabs.
