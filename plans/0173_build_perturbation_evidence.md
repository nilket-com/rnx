# rnx 0173 evidence: build-perturbation diagnostic

**Status:** implementation by Claude, for Codex's review. Descriptive only, with no tolerance, no gate and no re-classification of 0171/0172, which both stay STOP. The probe is `probes/build-perturbation-0173` in rnx-bench; the results are in `results/build-perturbation-0173`. The tooling was committed as `bc5b6219` before any run, and re-applied unchanged as `49156af3` when rnx-bench's master was rewritten (see §7). Every job ran under `/tmp/rnx-runtime-bench.lock` (`ledger/lock.log`):
- suites 15:13:55–15:23:43;
- builds 15:23:57–15:28:36;
- diagnostic 15:28:44–15:32:15.

## 1. Subjects

- **Fork edits:** sibling commits from `3e7d4da9ce908eeb7e0e1ac119dec24f68d5449a` (base `bb8e6937` plus 0172's tests-only inventory fixture), on the diagnostic branches below. They're exactly the frozen edits in plan §5.1:

	| Edit | Branch | Commit |
	|---|---|---|
	| C0 | `w-0173-c0` | `20aeb673ce7eee0831fcd7961b6729a252e4418a` |
	| C1 | `w-0173-c1` | `420fdfcc5264a52ef4c5092ab94fba04553897e1` |
	| C2 | `w-0173-c2` | `7f84fb769a7a42a25612b5350d0ced95e4b79e32` |
	| C3 | `w-0173-c3` | `59aba32ea330c0efb521e685092550fcc24038a3` |
	| C4 | `w-0173-c4` | `1e2b9ac2e1460f48687580421f3bd0349e0be1af` |

- **Builds:** every subject was built from **one fork worktree path**, checked out at its commit, and one harness path (exact copies of fb56b1d's sources), into equal-length target directories. So embedded source paths are identical across subjects.

## 2. Correctness: every edit passed

- **Fork suite** (`cargo test -p rune --all-targets --all-features`): C0, C1, C2 and C3 597/597; C4 598/598 (its added test). The suite includes 0172's registration-inventory golden and its negative controls, so **every edit registers exactly the same context**.
- **Differential corpus:** 0169's 28 corpus files and 8 fixtures, each subject against A0, strict `same()`: 252/252 identical. That covers all 7 non-A0 subjects, including those whose binary is byte-identical to A0.

## 3. Binary identity

| Subject | Edit | File sha256 | `.text` sha256 | Measured? |
|---|---|---|---|---|
| A0 | parent, clean build 1 | `7a66b042e865…` | `3f829d83686e…` | reference |
| A1 | parent, clean build 2 | **identical to A0** | identical | no (identity answers A) |
| A2 | parent, clean build 3 | **identical to A0** | identical | no |
| C0 | comment only | **differs** (`1f61f947df74…`) | **identical to A0** | yes |
| C1 | unused private fn | identical to A0 (compiled away) | identical | no |
| C2 | two private methods swapped | identical to A0 | identical | no |
| C3 | private helper renamed | **differs** (`ebdadf383a4b…`) | **differs** (`66c3fe31082b…`) | yes |
| C4 | `#[inline(never)]` fn used only from a test | identical to A0 (absent from release) | identical | no |

- **Same-source rebuilds are byte-reproducible here (A).**
- Of the null edits, only C0 and C3 changed the release binary. C0's change is outside `.text` (presumably line-number data, not shown).

## 4. Instruction changes (whole-process instructions:u, median of 10 per side, 5 ABBA repetitions)

The figure is change = right ÷ left − 1. Every sample's raw perf JSON, argv, environment, cwd, affinity, hashes (staged subjects verified before and after) and stdout are retained in `run1/*.jsonl`.

| Workload | B1 path (equal) | B2 path (equal) | B3 path −5 | B4 path +20 | B5 path +60 | E0→E1 (+1 KiB var) | E0→E2 (full env) | C0 comment | C3 rename |
|---|---|---|---|---|---|---|---|---|---|
| floor | +0.022% | −0.044% | +0.056% | +0.016% | +0.014% | +0.020% | **+2.280%** | −0.007% | −0.033% |
| empty-context | +0.009% | −0.034% | +0.006% | +0.031% | +0.048% | +0.007% | **+2.313%** | +0.012% | +0.085% |
| context | 0.000% | 0.000% | −0.001% | 0.000% | +0.001% | +0.001% | +0.039% | 0.000% | 0.000% |
| runtime | −0.002% | 0.000% | 0.000% | +0.001% | +0.001% | +0.001% | +0.036% | 0.000% | −0.001% |
| compile answer | 0.000% | 0.000% | 0.000% | −0.004% | −0.003% | +0.002% | +0.034% | 0.000% | +0.001% |
| run answer | −0.001% | 0.000% | −0.001% | −0.004% | −0.004% | 0.000% | +0.036% | +0.001% | 0.000% |
| run empty | 0.000% | 0.000% | +0.001% | −0.006% | −0.005% | +0.001% | +0.038% | 0.000% | +0.001% |
| run numeric … run overwrite_mixed (9 VM workloads) | ≤0.001% | ≤0.001% | ≤0.001% | ≤0.001% | ≤0.001% | ≤0.001% | ≤0.005% | ≤0.001% | ≤0.001% |
| run overwrite_deep | 0.000% | 0.000% | 0.000% | 0.000% | 0.000% | 0.000% | +0.004% | 0.000% | 0.000% |
| run overwrite_alias | −0.001% | 0.000% | −0.001% | +0.002% | +0.001% | +0.002% | +0.036% | −0.001% | +0.001% |

The full per-workload values are in `run1/diag.json`. The floor and empty-context whole-process windows are about 450k instructions; their per-side ranges are about ±0.2% (for example, B0 floor 449,764–450,976).

## 5. Reading (descriptive)

- **(A) Same-source rebuilds:** these three clean same-source builds were byte-identical.
- **(B) Launch:**
	- path changes of −5 to +60 bytes (B1–B5), and an added 1 KiB variable, move every workload by under 0.06%;
	- the **full inherited environment against a 3-variable environment** moves the tiny whole-process windows by about +2.3% (roughly +10k instructions) and every other workload by ≤0.04%.

	Environment size and content matter for startup-dominated measurements; paths, at these lengths, barely do.
- **(C) Null edits:**
	- three of five (C1, C2, C4) left the release binary byte-identical; C0 and C3 changed it;
	- C0 changed only non-`.text` bytes and moved everything by ≤0.012%;
	- C3 changed `.text` and moved everything by ≤0.085% (empty-context), with the VM workloads at ≤0.001%.

	No C workload moved more than 0.3%, so no S2 attribution was triggered (the plan's selection threshold).
- **Compared with 0171/0172 (allowed by plan §3):** the shifts that stopped them, VM workloads at +0.06% to +0.70% in 0172 and startup at +1.5% to +1.8% in 0171, are **outside the spread of every null edit measured here** (≤0.085% anywhere, ≤0.001% on VM workloads).
	- This record doesn't establish what caused those shifts.
	- These measured rebuilds, launch conditions and null edits did not reproduce those shifts. Other conditions and mechanisms remain untested, and no general causal exclusion follows from this small diagnostic.
	- It doesn't re-label either record, and it doesn't propose a gate.

## 6. Limitations

- **Departure, disclosed:** `diag.py` records `os.sched_getaffinity(0)` with each sample. That's the **controller's** affinity ({4}), which children inherit. It doesn't independently check the subject process's affinity, as plan §2 intended, so the field isn't observed child affinity.
- **Historical producer:** `diag.py` captures the raw `os.environ` for E2; the retained files were redacted after capture (§7). It's kept, as the producer that ran, labelled historical, and must not be rerun as is. Future probes redact at capture, with a fake-secret sentinel control.

- One machine and one toolchain; five specific null edits, of which only two changed the binary.
- C0's non-`.text` difference isn't attributed byte by byte.
- B's E2 is the controller's environment at run time (captured once; recorded in `run1/diag.json`).
- No wall-clock measurement (none was planned).

## 7. Credential redaction (affects 0172's bench commit and this record)

0172's pushed bench commit `65a3b49b` recorded the controller's full environment in `run4/measure.json` and `diag-aa/diag_aa.json`, and that environment contained a session credential and LAN addresses. With the user's explicit approval, rnx-bench's master was force-pushed (with a lease) to a redacted replacement, `1eece5fad775429be982a8d8a05ea44a6bbe8795`. It has the same parent and the same tree, except for those two files and an added `REDACTION.md`. The rnx 0172 evidence's references to `65a3b49b` now mean `1eece5fa`.

This record's E2 environment is redacted the same way (`results/build-perturbation-0173/REDACTION.md`): keys and value byte lengths kept, no measured count changed. Later probes redact at capture time.
