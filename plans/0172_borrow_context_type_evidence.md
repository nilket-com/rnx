# rnx 0172 evidence: borrow the container type during associated installation

**Result: STOP** in run 4, by the pre-registered precedence. Nothing is merged, and the fork's main stays at `bb8e69372353c50e271c9f115bc771c77aa6b83e`. The startup work dropped as hypothesised, but two VM workloads exceeded the 0.5% instruction no-regression gate. Earlier attempts stopped on plumbing (runs 1 and 2) and on the original reproduction gate (run 3). All are retained.

The probe is `probes/borrow-context-type-0172` in rnx-bench; the results are in `results/borrow-context-type-0172/run1…run4` and `diag-aa`. Every build, test, measurement and diagnostic ran under `/tmp/rnx-runtime-bench.lock`; the windows are in `probes/borrow-context-type-0172/ledger/lock.log`.

## 1. Fork commits

On branch `w2-0172-borrow-context-type`:

| Commit | Content |
|---|---|
| `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a` | Tests only, on the base. A registered-context inventory golden over every `Context` field, with per-category counts: 4,104 functions, 2,531 metadata entries, 1,934 constants, 2,523 `hash_to_meta`, 1,431 `item_to_hash`, 90 types, 81 associated, 65 implemented-traits, 11 macros, 8 traits, 6 deprecations, 5 unique, 1 crate, and names, for stdio true and false. Plus the missing-container error, and negative controls that refuse a removed function, a changed constant, a removed metadata entry and a removed name entry. Function and macro handlers are opaque closures, so they're described by their registration hash. Type entries are described by item, hash, `TypeInfo` display and type parameters; the first version printed `AnyTypeInfo`'s name-function pointer and failed its own cross-process determinism check, so the address was removed. The golden matched in three separate processes. |
| `7e748d020d763f330b64f2d33411e559de999231` | The change: `install_associated` borrows the container only to compute `item.extended(name)` and copy `type_parameters`. The `MissingContainer` clone stays on the error path; insertion and error order are unchanged; no attributes are touched. |

**The fork suite** (`cargo test -p rune --all-targets --all-features`) passed 597/597 on base+tests and on the candidate, including the inventory golden and its negative controls.

## 2. Measurement history (each attempt retained, nothing reused)

| Attempt | Stop point | Cause / repair |
|---|---|---|
| run1 | base reproduction, the first FIFO read | `<not counted>`. The controller wasn't pinned, so the FIFO counter child inherited no affinity (verified from the launch path). E-core placement is a plausible explanation, not observed. Repair: pin the controller to {4} and assert each child's affinity. |
| run2 | after calibration and the 0169 reproduction reads | The 0171 reference path pointed at a stale root checkout. Repair: read both references from this worktree's committed copies, with 0169's taken from its raw archive and verified against `rune-runtime-0169-SHA256SUMS`. |
| run3 | the original 2% reproduction gate | `floor` 0.952 and `empty-context` 0.961 against 0169 (about 2.3k-instruction FIFO windows). Every other row passed. Run 3's individual reproduction samples weren't persisted, which is a limitation. **It stays a STOP.** |
| A/A diagnostic | complete | Base only, no candidate anywhere. 540 FIFO samples of the byte-identical base counter (`0ae53261…`) from two launch paths. 0169's path reproduces the references (2304 / 2773 / 26,416,987); this probe's path gives a stable −115 in the tiny windows. The mechanism is unproven; 0169's environment and cwd weren't recorded. Codex independently checked all 540 records. |
| run4 | complete; **decision STOP** | Plan amendment 7 (agreed before run 4): only the historical FIFO floor and empty-context windows become descriptive. Every other gate is unchanged. |

## 3. Run 4

**Gates:**
- **Correctness:** 36/36 (28 corpus + 8 fixtures, keys qualified by origin), identical after only the two known normalizations.
- **Allocation direction:** context allocation calls 32,304 → 30,027 (−2,277); allocated bytes 3,562,368 → 3,495,929; peak 1,767,837 → 1,767,810.
- **Calibration:** resident driver vs `hyperfine --output=pipe`, true −0.020 ms, floor −0.021 ms (gate 0.15 ms).
- **Base reproduction:** 16 gated rows all at ratio 1.0000. The descriptive rows are floor −111 and empty-context −109 instructions.
- **Recorded:** subject argv, cwd, environment, affinity and hashes; every reference sample's value (`run4/reproduction-samples.jsonl`).
- **Retention limitation:** raw perf records (JSON and running percentage) survive for the **FIFO** reproduction samples only. For the whole-process perf samples (the 0171 reproduction rows and every candidate PMU repetition), `parse()` checked the JSON and running percentage (≥99%) at run time, but only the instruction and cycle values were kept. That isn't reconstructed here.

**Instructions:u (median of 5 alternating paired repetitions) and wall time (resident driver, median of 30):**

Protocol departure, disclosed: `measure.py` ran the PMU repetitions as five **alternating pairs** (AB, BA, AB, BA, AB), not the plan's ABBA repetitions. The wall clock ran the stated ABBA/BAAB blocks, 3 rounds × 10 per subject. This departure is part of a STOP and couldn't have supported a WIN.

| Workload | instr base (M) | instr cand (M) | Δ instr | wall base (ms) | wall cand (ms) | Δ wall | base p10–p90 (ms) |
|---|---|---|---|---|---|---|---|
| floor | 0.460 | 0.461 | +0.12% | 0.460 | 0.466 | +1.24% | 0.049 |
| empty-context | 0.461 | 0.461 | −0.02% | 0.461 | 0.456 | −0.96% | 0.028 |
| **context** | 26.876 | 25.903 | **−3.62%** | 3.618 | 3.523 | −2.62% | 0.091 |
| runtime | 28.251 | 27.279 | −3.44% | 3.858 | 3.745 | −2.94% | 0.040 |
| compile answer | 29.908 | 28.936 | −3.25% | 4.214 | 4.123 | −2.18% | 0.056 |
| run answer | 29.915 | 28.943 | −3.25% | 4.233 | 4.134 | −2.34% | 0.028 |
| run empty | 28.397 | 27.426 | −3.42% | 3.915 | 3.818 | −2.46% | 0.025 |
| run numeric | 1757.096 | 1758.125 | +0.06% | 124.682 | 112.074 | −10.11% | 2.091 |
| run fib | 831.057 | 832.628 | +0.19% | 55.457 | 51.947 | −6.33% | 0.545 |
| run strings | 227.724 | 226.872 | −0.37% | 19.000 | 18.243 | −3.98% | 0.250 |
| **run while** | 1136.115 | 1142.143 | **+0.53%** | 70.511 | 69.290 | −1.73% | 0.704 |
| **run compare** | 1709.628 | 1721.579 | **+0.70%** | 108.109 | 106.419 | −1.56% | 0.843 |
| run calls | 1773.172 | 1779.200 | +0.34% | 98.952 | 94.790 | −4.21% | 0.613 |
| run vector | 1308.713 | 1313.441 | +0.36% | 84.671 | 83.379 | −1.53% | 0.725 |
| run overwrite_inline | 1137.222 | 1141.251 | +0.35% | 69.020 | 68.522 | −0.72% | 0.896 |
| run overwrite_mixed | 746.301 | 747.130 | +0.11% | 50.569 | 48.289 | −4.51% | 0.546 |
| run overwrite_deep | 254.491 | 253.919 | −0.22% | 22.280 | 20.785 | −6.71% | 0.852 |
| run overwrite_alias | 30.342 | 29.370 | −3.20% | 4.310 | 4.223 | −2.02% | 0.135 |

**Decision, by the frozen precedence:**
- Correctness and measurement passed.
- Instruction regressions above 0.5%: **run while +0.53%, run compare +0.70%**. That's a **STOP**.
- No wall regressions (floor's +0.006 ms is inside its 0.049 ms band) and no disagreements.
- Context's −3.62% would otherwise have met WIN (≥1.0%).

## 4. Reading (descriptive)

- **The startup hypothesis held.**
	- Default-context construction and drop run 0.97M fewer instructions (−3.6%) and about 0.1 ms less wall time (−2.6%), with 2,277 fewer allocations.
	- Every startup-dominated workload improves by about 3.2–3.4%.
	- The registered inventory is identical.
- **The VM-heavy workloads moved by −0.37% to +0.70% in instructions,** with no change to VM source:
	- up: compare +0.70%, while +0.53%, vector +0.36%, overwrite_inline +0.35%, calls +0.34%, fib +0.19%, overwrite_mixed +0.11%, numeric +0.06%;
	- down: strings −0.37%, overwrite_deep −0.22%.

  Wall time on them was lower in every case (−0.7% to −10.1%).

  This resembles 0171's pattern in the opposite direction. Code-generation/layout perturbation is an **unverified hypothesis**. This run doesn't establish that the shifts happen in untouched code, and it doesn't measure an instruction noise floor. The large wall improvements on VM workloads, for example numeric −10% at +0.06% instructions, are also unexplained, and they're not claimed as a benefit of this change.

## 5. Implication for the campaign: a proposal, not part of this record

Both W3's first change (0171) and W2's first change (0172) were stopped by sub-1% instruction shifts on workloads outside the code they changed. The 0.5% per-workload no-regression gate was fixed before either run and is kept for both. But the campaign needs that noise floor *measured* rather than assumed.

**Proposed next record:** a **build-perturbation A/A**. Make semantically null source changes in the fork (each must leave the context inventory and the corpus identical: an unused function, a reordered private item, a renamed private helper), and measure the distribution of per-workload instruction changes they cause under this exact protocol. Future records would then pre-register their regression gate from that measured distribution, before seeing any candidate. 0171 and 0172 stay STOPs under their own protocols, and they'd be re-evaluated only as new pre-registered records.

## 6. Limitations

- One machine and one toolchain.
- 0169's launch environment and cwd weren't recorded.
- Run 3's individual reproduction samples weren't persisted; persistence was added for run 4.
- Equal-length base and candidate paths (91 characters) are a mitigation only; they don't prove symmetric path effects.
- The codegen-perturbation explanation is a hypothesis.

## 7. Bench commit mapping

The bench probe commits are squashed into one for push. Producer commits of each attempt, all pre-squash on branch `w2-0172`:
- **run1:** produced by **uncommitted** tooling: `4b362bd4`'s `measure.py` without its affinity lines (the repair, plus the child-affinity assertion added on review). Run1's results are retained in `4b362bd4`. Its exact producer bytes were never committed; that's a limitation;
- **run2:** tooling `4b362bd4`, retained in `6116c332`;
- **run3:** tooling `2acdf315`, retained in `e4de7a62`;
- **A/A diagnostic:** tooling `c9bdd2a8`, retained in `ac2ec45c`;
- **run4:** tooling `7f188744`, retained in `50891c4f`.

`results/borrow-context-type-0172/PRODUCERS.md` gives the same map and the final source.
