# rnx 0171 evidence: store fast-path experiment

**Result: STOP**, under the pre-registered rules, in both runs. Nothing is merged into the fork's main; the fork root stays at `bb8e6937`. The probe is `probes/store-fast-path-0171` in rnx-bench; the results are in `results/store-fast-path-0171`. Every build, test, measurement and diagnostic ran under `/tmp/rnx-runtime-bench.lock`; the windows are in `probes/store-fast-path-0171/ledger/lock.log`.

## 1. What was tested

Fork branch `w3-0171-store-fast-path`, in the worktree `~/work/rune-w3-0171`:

| Commit | Content |
|---|---|
| `11c5b90fd39ddd3bf417bb1bfb2690f67600ff87` | Tests only, on base `bb8e69372353c50e271c9f115bc771c77aa6b83e`. Ten `store_with` order/drop tests with a native `DropSpy` (A2): the four inline/non-inline combinations, a failed conversion, a bad address, discard, aliasing, depth 100,000, and multi-slot drop order. Plus the budget halt-state golden test: suspended `VmExecution` at budgets 1/2/3/10/1e3/1e6, recording ip, last_ip, a native side-effect counter and an address-free description of every stack slot. Its shifted-halt control fails as required (A3). |
| `bdac71877db7740089c298e2f7c43cde0ce5c80d` | Run 1 candidate: the fast path in `Stack::store_with` (write directly when the overwritten slot `is_inline()`), **plus `#[inline]` on `Value::is_inline`**. That attribute was outside the reviewed "only `Stack::store_with`" scope (A1); this departure is named here. |
| `ddc92e7375fa546c64e9522d557c5823ad9fdf62` | Variant B (amendment agreed before the run): removes only that attribute. |

**Audit of other `Worklist::replace` callers** (none touched):
- `memory.rs:230` (`Memory::store` for slices);
- `vec.rs:188` (`Vec::set`);
- in `vm.rs`: `try_tuple_like_index_set`, `try_object_slot_index_set`, `call_generator_fn`, `call_stream_fn`, `call_async_fn` and `op_environment`.

## 2. Correctness gates: all passed, both runs

- **Fork suite** `cargo test -p rune --all-targets --all-features`: 607 passed, 0 failed. Identical on base+tests (`11c5b90f`), run 1 (`bdac7187`) and variant B (`ddc92e73`). It includes the `DropSpy` order tests and the halt-state golden, which was recorded on the base and matched unchanged by both candidates.
- **Differential corpus:** the 0168 corpus (27 files) plus 13 fixtures, so **40 executed invocations** per measurement, each asserted identical (base vs candidate, after only the two known normalizations), with fixture outputs matching their independent oracles. **Retained transcripts: 35.** `measure.py` keys results by directory name plus stem, both directories are named `fixtures`, and the 5 overlapping stems (answer, empty, fib, numeric, strings) overwrote the corpus entries in the JSON. Every execution was still asserted when it ran, but 5 transcripts per run aren't retained. Any future probe keys by origin.
- **Base reproduction:** a fresh base build reproduces 0170's retained whole-process counts (numeric, fib) at ratio 1.0000.

## 3. Measurements (frozen protocol: 5 interleaved PMU repeats, ABBA native wall clock 3 × 10)

**Variant B's binary is byte-identical to run 1's** (SHA-256 `c1e06940…`, rebuilt from the changed source), so the attribute had no effect on the release binary. Run 2 is effectively a replicate, and it reproduced run 1 within noise. Run 1:

| Workload | instr base (M) | instr cand (M) | Δ instr | wall base (ms) | wall cand (ms) | Δ wall | base wall p10–p90 (ms) |
|---|---|---|---|---|---|---|---|
| empty | 28.4 | 28.9 | **+1.80%** | 3.95 | 3.98 | +0.9% | 0.13 |
| answer | 29.9 | 30.4 | **+1.54%** | 4.25 | 4.30 | +1.2% | 0.11 |
| numeric | 1757.1 | 1709.6 | −2.71% | 125.18 | 110.79 | −11.5% | 1.60 |
| while | 1136.1 | 1084.6 | −4.54% | 69.47 | 65.22 | −6.1% | 0.44 |
| fib | 831.1 | 802.9 | −3.39% | 53.30 | 50.14 | −5.9% | 0.59 |
| calls | 1773.2 | 1720.6 | −2.96% | 94.86 | 90.77 | −4.3% | 1.55 |
| compare | 1709.6 | 1645.2 | −3.77% | 107.06 | 99.85 | −6.7% | 1.11 |
| vector | 1308.7 | 1286.6 | −1.69% | 84.03 | 79.99 | −4.8% | 2.21 |
| strings | 227.7 | 226.8 | −0.40% | 19.11 | 18.57 | −2.8% | 0.26 |
| overwrite_inline | 1137.2 | 1087.7 | −4.36% | 68.31 | 66.24 | −3.0% | 0.61 |
| overwrite_mixed | 746.3 | 734.2 | −1.63% | 50.46 | 47.99 | −4.9% | 0.73 |
| overwrite_deep | 254.5 | 254.8 | +0.10% | 22.24 | 21.21 | −4.7% | 0.80 |
| overwrite_alias | 30.3 | 30.8 | **+1.52%** | 4.35 | 4.38 | +0.7% | 0.20 |

**Decision, by the frozen precedence:**
- Correctness passed.
- Instruction regressions above 0.5%: empty, answer and overwrite_alias. That's **STOP**.
- No wall regression (each startup-dominated wall change is inside its band) and no disagreements.
- Gate wins on while and fib would otherwise have met WIN's condition (b).
- Variant B: the identical decision.

## 4. Diagnosis of the startup regression (descriptive, pre-agreed for the case that B also regressed)

1. **Harness phase counts** (3 repeats each):

	| Mode | base | cand |
	|---|---|---|
	| floor | 460.6k | 460.5k |
	| empty-context | 461.2k | 461.1k |
	| `context` (whole context-mode invocation: process start, `Context::with_default_modules` construction and drop, teardown) | 26.876M | **27.436M (+0.56M, +2.1%)** |
	| runtime | | +0.51M |
	| compile of answer | | +0.46M |

	The increase is in the whole context-mode invocation (construction and drop of the default context, with process startup and teardown; floor and empty-context are unchanged), which runs no VM and no `Stack::store_with`.
2. **Allocation summaries** (counting-allocator builds, `--features allocation`): context, runtime and compile-answer have **identical** allocation calls, allocated bytes, live bytes and peak bytes on base and candidate. That's consistent with the same registration work, but it doesn't prove it.
3. **S2 instruction samples of whole context-mode invocations (including startup and teardown)** (line tables, no frame pointers, period 5e5, 3 × 130 invocations; ~20.7k/21.1k target samples). Symbol-level shifts consistent with different inlining/outlining in registration code:
	- `btree::map::infallible_cmp::<Component>` appears out of line only in the candidate (0 → 485 samples);
	- `RawTable<…ConstValueBuf>::reserve_rehash` appears out of line (0 → 417);
	- `RawTable<(Hash, Vec<usize>)>::reserve_rehash` disappears (382 → 3);
	- `xxhash64::write` falls (772 → 337) while `memcmp`, `_int_malloc` and ahash `hash_one` rise.

	These show different code generation in registration paths unrelated to the store change. **That this is caused by the store_with change's effect on code-generation unit partitioning or inlining heuristics is a hypothesis:** it's consistent with identical allocations and the symbol shifts, but not demonstrated (Codex's qualification).

## 5. Limits and departures

- The `#[inline]` attribute in run 1 was outside the reviewed scope (A1). It turned out to have no effect on the binary.
- The wall clock used `subprocess.run` with the native helper and **no hyperfine preflight** (the 0171 plan didn't require one). Base and candidate share one clock, so a fixed launch offset largely cancels. Instructions are the decision metric.
- Two diagnostic attempts failed on shell quoting before producing any data; their lines are in the ledger.
- Wall sampling as run (`measure.py`, unedited since 13:25:02, before run 1 at 13:25:11; committed as-is):
	- 3 rounds; per round and workload, the block order ABBA (base, cand, cand, base) on even rounds and BAAB on odd rounds;
	- 5 samples per block, so 10 samples per subject per round and 30 per subject in total (the plan's 3 × 10).

	An independent checker (`check_decision.py`, written separately) recomputes both runs' decisions from the retained arrays: STOP, regressions answer/empty/overwrite_alias (instructions), no wall regressions or disagreements, gate wins fib and while. Both match the reports.
- One machine, one toolchain.

## 6. What this means for W3

On these kernels the candidate executes fewer instructions on the VM-heavy workloads: while −4.54%, overwrite_inline −4.36%, compare −3.77%, fib −3.39%, calls −2.96%, numeric −2.71%, vector −1.69%, overwrite_mixed −1.63%, strings −0.40%. Wall time is 2.8–11.5% lower on them, with identical observed behaviour. It's held back by ~0.5M more instructions in the whole context-mode invocation, which fails the pre-registered 0.5% gate on startup-dominated workloads. The cause of those instructions (different code generation in registration and teardown code) is a hypothesis: it's consistent with the symbol shifts and allocation summaries, not demonstrated. Revisiting C1 is proposed for one of:
- (a) after the W2 registration rework, which changes that code;
- (b) under a separately pre-registered build-configuration protocol, e.g. measuring base and candidate both at `codegen-units = 1`, as a product build decision rather than a tolerance change.

The rules aren't relaxed for this record.
