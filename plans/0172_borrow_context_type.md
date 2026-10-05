# rnx 0172: borrow the container type during associated-item installation

**Status:** plan, by Claude, for Codex's review. This is W2's first change, in `plans/rune_roadmap.md`. Claude implements; Codex reviews and reruns the gates independently. It's an **experiment** under a protocol fixed before any measurement, as in 0171. It changes no rnx dependency, no shipped behaviour and no Context API; the fork's main stays at `bb8e6937` unless both of us accept a WIN.

## 1. Hypothesis

0169 measured the complete default-context registration on fork main:
- the disjoint install stages are trait implementations 1.503 ms, associated 0.296 ms, types 0.096 ms and items 0.052 ms;
- there are 172 trait implementations, 1,717 native registration attempts and 2,531 metadata insertions.

Trait default methods reach `Context::install_associated` through `TraitContext::function_inner`, once per implementing type per default method.

`install_associated` (compile/context.rs:1104) begins with:

	let Some(info) = self.types.get(&assoc.container).try_cloned()? else { … };

It clones the container's whole `ContextType` (an `ItemBuf`, a `TypeInfo`, a hash and type parameters) on every call, but reads only `info.item` (to build `item.extended(name)`) and `info.type_parameters` (a `Hash`, which is `Copy`). The clone exists only because the function goes on to mutate `self`.

**H1:** taking the two needed pieces while the borrow is live (build the extended `ItemBuf` and copy `type_parameters`), then releasing it, removes one `ContextType` clone and its allocations per associated item, with no observable change.

## 2. The change

- **Fork branch** `w2-0172-borrow-context-type` from `bb8e69372353c50e271c9f115bc771c77aa6b83e`, in an isolated worktree.
- **First commit (tests only, on the base):** pin the context's full registration inventory. That's every function hash, meta hash and kind, constant key and value, names-tree entry and the error paths. Record it as a golden on the base, like 0171's halt-state golden. Add a test that registering an associated item for a missing container still produces the same `MissingContainer` error.
- **Second commit (the change):** in `install_associated` only, replace `try_cloned()` of the container with a borrow scoped to computing:
	- `(hash, item)` for instance names;
	- `type_parameters`;
	- for the error path only, the existing `container_type_info` clone.

  The order of every later insertion into `constants`, `functions` and meta stays exactly the same. No other function changes; no `#[inline]` or attribute changes (0171 lesson). Any second site found while implementing needs a reviewed amendment first.

## 3. Correctness gates (before any measurement; any difference is a STOP)

1. **Fork suite** `cargo test -p rune --all-targets --all-features`: base+tests first, then the candidate, with identical results.
2. **Registration inventory golden:** recorded on the base and matched exactly by the candidate. That covers every entry, in sorted order, with its kind and value.
3. **The 0168 corpus plus 0171's fixtures:** base vs candidate, strict `same()` with only the two known normalizations. Keys qualified by origin (0171 R3 lesson), so 40 executed and 40 retained.
4. **Allocation summaries** for floor, empty-context, context, runtime and compile-answer (counting-allocator builds). This is a **direction check**, pre-registered: the candidate's context allocation calls must not rise. A fall is expected but not required for correctness.

## 4. Measurement (pre-registered; the 0171 protocol, adapted)

- **Subjects:** the 0169 harness built twice, from the same source, lock, release profile and features. The base binary must reproduce 0169's retained base counts within 2% before any candidate change is attributed.
- **Workloads:**
	- the harness modes floor, empty-context, context and runtime;
	- compile and run of answer;
	- run of empty, numeric, while, fib, calls, compare, vector, strings and 0171's four overwrite fixtures.
- **Primary metric:** whole-process instructions:u, 5 interleaved ABBA repetitions, pinned to core 4, ≥99% running, under the shared lock.
- **Secondary metric:** wall time with a **resident** native driver (0169's architecture, which calibrated against hyperfine `--output=pipe` within 0.03 ms), ABBA with 3 rounds × 10 per subject. The calibration preflight is the 0169 gate (0.15 ms), run first.
- **Decision rule, fixed now, in precedence order:**
	1. any correctness or measurement failure (including a failed calibration or base reproduction) is a STOP;
	2. any workload's median instructions:u rising by more than 0.5%, or median wall time worse by more than its base p10–p90 band, is a STOP;
	3. a wall/instruction disagreement (opposite signs, both above 3%, outside both noise bands) is a STOP;
	4. a **WIN** needs the `context` mode's median instructions:u to fall by **at least 1.0%**, with **no regression on any workload**;
	5. otherwise **NO-WIN**.
- **Expected size:** it's honestly unknown. The clone happens about once per associated item (on the order of the 1,717 native registration attempts). Predicting a saving from the clone's cost would multiply an assumed cost by a count, which 0170 rules out. So only the measured result counts.

## 5. Closure

- **On WIN:** the fork branch is offered for a fast-forward into fork main after both reviews, with an unfiled upstream-PR draft in `plans/0172_upstream_*.md` (the user's call to file).
- **On NO-WIN or STOP:** the branch is retained but not merged, and the evidence records why.
- **Commits:** rnx plan and impl, rnx-bench `probes:` and the fork branch, all integrated serially after review.

## 6. Amendments from Codex's plan review (in force; they replace the matching text above)

1. **The base-reproduction reference is pinned by workload and metric.**
	- 0169's retained instruction rows are the reference for its own workloads; 0171's retained base rows cover the fixtures 0171 added.
	- Any workload without a historical reference is a contemporaneous base/candidate comparison only, and is labelled so.
	- The 2% historical reproduction gate applies to instructions only, never to wall medians.
	- Each reference's source path, hash, and exact command or mode are recorded before measuring.
2. **The inventory golden fails closed.**
	- The traversal is deterministic and exhaustive, with explicit counts per covered category. No opaque constant value or metadata kind is ignored; an entry that can't be represented fails the test instead of being left out. Function handlers are opaque closures, so they're represented by their registration hash and recorded as such.
	- Negative controls remove or change a function, a constant and a metadata/name entry, and prove the comparison refuses each one.
	- It covers default modules with stdio true and false, plus the missing-container error fixture.
	- The inventory establishes the registered structure; the differential corpus establishes executable behaviour. The two claims are stated separately.
3. **The resident driver is an exact source copy of 0169's,** pinned to rnx-bench `fb56b1d`, with each file's hash recorded.
	- It keeps the argv/order receipts, a fresh target process per sample, pipe capture, and the 0.15 ms `hyperfine --output=pipe` gate.
	- Any adaptation needs a reviewed diff before timing.
	- TMPDIR is on the results filesystem, or the runner fails a filesystem check before measuring, so the post-run corruption controls can't repeat 0169's EXDEV failure.
4. **The scope of "no observable change" is narrowed:** identical successful registration and identical existing semantic errors. Removing a fallible clone necessarily changes where an allocation failure can happen, so allocation-failure points are not promised to be identical.

## 7. Method amendment after run3 (agreed with Codex, applies from run4)

- **Run 3 stays a STOP** under the original protocol. It isn't retroactively a pass.
- **Base-only evidence for the amendment:** an A/A diagnostic of the byte-identical base counter build showed a repeatable tiny-window difference that depends on the launch condition. Launched from 0169's original path, it reproduces 0169's floor and empty-context references exactly. Launched from this probe's path, it's about −115 instructions in those ~2.3k-instruction windows. The mechanism is unproven. 0169's environment and cwd weren't recorded, and run 3's individual reproduction samples weren't retained; both are stated limitations.
- **What changes for run 4:** **only** the two historical FIFO windows `floor` and `empty-context` become **descriptive**. Their historical and current counts and differences are reported, not gated.
- **What stays the same:**
	- every other historical reproduction row keeps the 2% gate;
	- contemporaneous whole-process floor and empty-context remain in all candidate instruction and wall no-regression gates;
	- the context WIN threshold, calibration, correctness, the allocation direction check and the decision precedence are unchanged.
- **The equal-length base/candidate paths** (base/ and cand/, 91 characters) are a **mitigation only**. They don't prove that path effects are symmetric.
- **Recorded:** each subject's exact argv, cwd, environment, affinity and binary hashes. Each reference sample and its raw record are persisted as it finishes, so a future STOP keeps the reproduction evidence.
- **No further loosening:** if any remaining historical row or candidate gate fails, it's a STOP.
