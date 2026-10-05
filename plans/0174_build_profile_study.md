# rnx 0174: build-profile study

**Status:** plan, by Claude, for Codex's review before any edit or measurement. Claude implements; Codex reviews. This is a **diagnostic**. It changes no shipped profile, no fork main and no gate, and it re-labels nothing. Changing rnx's shipped release profile, or re-evaluating 0171 or 0172, would each be a separate pre-registered record. A profile change would also be brought to the user first, because it changes what `cargo install rnx` costs.

## 1. Question

rnx ships Cargo's default release profile: opt-level 3, 16 codegen units, no LTO. The harness matches it. 0171 and 0172 were each stopped by sub-1% instruction shifts on workloads outside the code they changed, and 0173's rebuilds, launch conditions and null edits didn't reproduce shifts of that size.

This record asks two descriptive questions:
1. How do different release profiles change Rune's startup and VM costs, and what do they cost in build time and binary size?
2. Under each profile, do the instruction side effects of 0171's and 0172's exact changes persist?

## 2. Subjects

- **Profiles:**
	- **P0:** the default release profile;
	- **P1:** `codegen-units = 1`;
	- **P2:** `lto = "thin"`, `codegen-units = 1`;
	- **P3:** `lto = "fat"`, `codegen-units = 1`.

	All keep opt-level 3, `panic = "unwind"`, and no debug info. The profiles are set in the harness manifest, applied to the whole build graph.
- **Sources:** the fork at `3e7d4da9` (base plus 0172's inventory fixture), with the harness sources pinned to rnx-bench `fb56b1d`.
	- **Base:** `3e7d4da9`.
	- **S71:** `3e7d4da9` plus 0171 run 1's `store_with` diff, exactly as in `bdac71877db7…` with its `is_inline` attribute; that attribute proved byte-irrelevant in 0171. Applied as a sibling commit.
	- **S72:** `3e7d4da9` plus 0172's change, exactly as in `7e748d02…`, which is a child of `3e7d4da9` already.

	S71 and S72 are recreated or reused on a branch named `w-0174-*` and never merged.
- **Build method:** 0173's single-worktree, equal-length-target-directory method, so embedded paths are identical.
- **Builds:** 3 sources × 4 profiles = 12 release binaries, plus 4 profile variants of the 0169 `counter` build for base only (allocation counts aren't needed).

## 3. Correctness (before any measurement)

- **Fork suite** on the base and on the S71 and S72 commits: once per source, not per profile, since the suite runs the test profile. They already passed in 0171 and 0172, and they're rerun for this record.
- **Per binary:** the 0172 corpus and fixtures, strict `same()` against P0 base (40 runs per binary).
- **Inventory:** the 0172 registration-inventory golden must match for every source. That's a test-profile check; profile independence of the registered structure is assumed, and the assumption is stated.

## 4. Measurement

- **Launch:** every sample runs from 0173's fixed staged path, `stage/primary`, with hashes verified before and after, environment **E0** (exactly `{PATH: /usr/bin:/bin, HOME: /home/me, LANG: C.UTF-8}`), the controller pinned to {4}, and **each child's affinity read from `/proc/<pid>/status` before the counted interval**. That fixes 0173's controller-only departure, using a READY-style wait where the harness allows it; otherwise it's disclosed again.
- **Workloads:** 0172's 18.
- **Redaction:** no environment beyond E0 is ever recorded. A **fake-secret sentinel control**:
	- the controller's environment includes `RNX0174_SENTINEL` with a random 32-hex value, kept in memory only;
	- after the run, every result file, ledger, archive and command log is scanned for that value (counts only), and any occurrence is a STOP.
- **Comparisons,** all **5 true ABBA repetitions** per workload, whole-process instructions:u and cycles:u, with raw perf JSON retained per sample:
	- (i) **profile effect on the base:** P0 vs P1, P0 vs P2, P0 vs P3;
	- (ii) **change effect under each profile:** base vs S71 and base vs S72, within P0, P1, P2 and P3 (8 comparisons).
- **Wall clock:** the 0169 resident driver for comparison (i) only, ABBA 3 rounds × 10 per subject per workload, after its 0.15 ms `hyperfine --output=pipe` calibration preflight.
- **Cost:**
	- clean build time of the harness per profile (3 builds each, under the lock, `CARGO_INCREMENTAL=0`), plus binary size;
	- clean `cargo install --path` time of **rnx itself** at `785557a` per profile, from an isolated copy with only the profile changed, 1 build each;
	- `rnx eval 42` instructions:u and wall per profile (5 ABBA against P0), so product-level startup is visible.

## 5. Reporting

- Per-comparison medians, full sample lists and changes.
- Binary and `.text` hashes.
- Build times and sizes.
- A table of 0171's and 0172's side-effect workloads (empty, answer, overwrite_alias for S71; while, compare for S72) under each profile.

**No gate, tolerance or re-classification.** The discussion may say whether a profile changes the size of those side effects. It may not propose shipping a profile; that's a separate record with the user's input.

## 6. Gates and closure

- **STOP conditions:**
	- a correctness failure (that subject halts and is retained);
	- a measurement failure (affinity, running below 99%, output, missing raw record);
	- a sentinel occurrence;
	- a failed calibration (for wall time only).
- **Commits:** rnx plan and impl, an rnx-bench `probes:` commit, and the `w-0174-*` fork branches anchored on the fork. The secret scan runs before every push.

## 7. Clarifications from Codex (before its plan review)

- **No "best profile" is selected.** Comparison (ii) measures both changes (S71 and S72) under all four profiles, and every result is reported. No profile is chosen after seeing the side effects.
- **The two kinds of build evidence are kept separate:**
	- the **engine-harness** profile measurements (§4 (i) and (ii));
	- the **rnx `cargo install`** cost measurements.
- **The install measurement is pinned:**
	- **toolchain:** rustc 1.98.1, recorded with `rustc -vV`; default features; `--locked`;
	- **dependencies:** fetched first, once, with `cargo fetch --locked`; that phase is timed and reported separately;
	- **build:** the timed phase is `cargo install --path <isolated copy of rnx at 785557a> --locked --offline --root <fresh dir>`, with a fresh empty `CARGO_TARGET_DIR` per build;
	- **profiles:** the profile is set by editing only the `[profile.release]` section of that isolated copy, and the resulting profile is verified in cargo's `--timings`/build output where available;
	- **repetitions:** one clean build per profile (stated as a single observation, not a distribution).

## 8. Amendments from Codex's plan review (in force; they replace the matching text above)

1. **Corpus and subjects.**
	- Correctness covers **36** origin-qualified fixtures (0169's 28-file corpus plus 0171/0172's 8 fixtures), not 40. Each binary is compared with P0 base, and the outputs, statuses and errors are retained under the two existing normalization rules.
	- **S71** = `30c53555c97b183dfada30cb563fcf5bedeb0d31` (branch `w-0174-s71`): 0171's exact diff (`11c5b90f..bdac7187`, the `store_with` fast path plus the `is_inline` attribute) applied to the common test parent `3e7d4da9`. Its content hunks are verified identical to 0171's diff; only the blob-index lines differ, because the parents differ.
	- **S72** = `7e748d020d763f330b64f2d33411e559de999231` (branch `w-0174-s72`), whose parent is `3e7d4da9`.
	- All three sources therefore share the same tests-only parent.
2. **Build-cost conditions.**
	- Every build uses an isolated **empty** target directory (and an empty install root for installs), one pinned toolchain (rustc 1.98.1, `rustc -vV` recorded), `--locked`, and `--offline` after a single `cargo fetch --locked`, which is timed and reported separately.
	- All builds use `-j 8`, identical environment E0 (plus `CARGO_HOME`/`RUSTUP_HOME` pointing at the usual locations), no RUSTFLAGS, and default features.
	- The profile is set only through `[profile.release]` in the isolated manifest.
	- **Harness builds:** 3 clean builds per profile, in the balanced order P0 P1 P2 P3 / P3 P2 P1 P0 / P1 P3 P0 P2 (frozen).
	- **rnx `cargo install`:** one clean build per profile, in the order P0 P1 P2 P3. Each is a single observation, not a timing distribution.
	- Every build has a 60-minute deadline. A failure or timeout is retained and reported, never omitted.
	- Reported separately: the clean compile + link + install wall time, and the dependency fetch.
3. **Profile semantics.**
	- **P0 is "the default release profile".** Cargo's default `lto = false` still performs crate-local ThinLTO across codegen units, so P0 isn't labelled "no LTO", and local LTO is never switched off to fit a label.
	- Every build records its actual rustc invocations (`cargo build -v`, rustc/link arguments for every crate including dependencies and build scripts) and its effective profile settings.
	- **The installed rnx uses its shipped Rune 0.14.2; the engine harness uses fork `bb8e6937`.** rnx install times and `rnx eval 42` results describe the product build cost; they don't validate the fork side-effect results.
4. **Affinity and launch, frozen.**
	- The counted interval stays **whole-process `perf stat`** (exec to exit), and the native-clock interval is unchanged.
	- Affinity comes from the controller, `os.sched_setaffinity(0, {4})` asserted, which children inherit through the audited launch path: Python `subprocess` → `perf stat` → subject, with no `taskset` and no shell.
	- A **bounded representative child control** runs before the measurements and at the end. It uses the identical launch path, `perf stat -- /usr/bin/grep Cpus_allowed_list /proc/self/status`, and its output must be `Cpus_allowed_list:\t4`, 5 times each.
	- Per-sample affinity fields are labelled **controller affinity**. There is no per-sample child observation and no open-ended fallback.
5. **Sentinel rehearsal before any official work.**
	- At controller start, a random 32-hex value is generated with `secrets.token_hex` and put into the controller's own `os.environ` as `RNX0174_SENTINEL`. It's never written anywhere.
	- An instrumented rehearsal exercises normal result writing, a deliberately failing command (its exception text and argv are logged as normal), the ledger, a compressed archive of the rehearsal output, and the command log. All rehearsal output, including the decompressed archives, is then scanned in memory for the value. Any occurrence is a STOP before official work.
	- After the official run, the same scan covers every new file. Before every push, all new reachable content, including compressed artifacts, is scanned for the sentinel and for the known exposed credential, reporting counts only.
	- Subjects run with the explicit E0 environment and never inherit the controller's environment.
	- The real credential rotation stays separately pending until it's confirmed.
6. **Wall schedule, frozen.**
	- **Profile comparison (i):** 3 rounds, each with ABBA blocks (rounds 1 and 3) or BAAB blocks (round 2), 5-sample blocks, so 30 per side per workload.
	- **`rnx eval 42` per profile:** the same schedule, against installed-rnx P0, after 5 warmups per side.
	- Both use the 0169 resident driver after its 0.15 ms `hyperfine -N --output=pipe` calibration, which is run first.
7. **Release-mode inventory** stays an explicit, **untested assumption**. The inventory golden runs in the test profile only, so no stronger claim about registered structure across release profiles is made.
