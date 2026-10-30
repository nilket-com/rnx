# rnx 0198: default one-shot execution to native code

Authority: chatd rune-routing seq24288. Base: rnx ff24cc81, pinned Rune
145e754086816eb479b0a07eddac02d4ce39819f. Native is already compiled in. The
user approved enabling it by default and assignment coverage next.

Set the runtime default to native on for the one-shot routes introduced by
0197. Keep --native=off, --engine=legacy and the existing whole-program
compile/prepare refusal boundary. Keep explicit internal interpreter probes as
such; sessions, eval, Jupyter, servers and retained-Unit selfchecks keep their
existing routes. Project and assembly prefixes remain runtime choices and do
not enter artifact fingerprints. Update help and documentation.

Scope ruling seq24295: this record is rnx-only, with no Rune prerequisite.
--debug-engine reports only the requested native policy, explicitly labeled
native-requested=on|off. It makes no claim about native region entry or the
actual cached environment policy. The read-only native_report(&Program) design
and PERF_BUILDID_DIR visibility remain backlog work. No private-symbol or
environment-guessing workaround belongs in the product.

Measure the actual short-run cost before landing the default. Use the final
exact-source release CLI and one compiled-native-in binary for off/on. Run on
the measurement core under the existing lease, with explicit environment, perf
children wrapped in env -u PERF_BUILDID_DIR, unscaled instructions and perf
duration_time as executable wall. Retain parent elapsed separately.
Full-source startup, census/demos and small no-region scripts are separate
classes. Use the existing hygienic corpus adapter and source hashes; compile
or arity controls are labeled separately from executed programs. Keep every
raw sample, exit status, output and admission/entry observation. Compare
on/off live output and effects; never normalize a mismatch away.

Use interleaved paired short runs, with more pairs for tiny no-region
witnesses to expose sub-millisecond cost. Record the total cold analysis/JIT
cost through actual instructions and wall; no new telemetry API is required.
No synthetic busy loop may hide per-process setup cost. Record thermal
counters per batch; retain contaminated data with that label. Do not run a
full M8 study or repeat the long throughput comparison. Use a finite detached
operator with a wall ceiling, terminal receipt and exit notification. Never
poll or run progress watchers. If measured no-region overhead exceeds about 1
ms per invocation or 5% of startup, bring the paired deltas, spreads and
sample counts to review before landing; do not quietly add lazy analysis.

Correctness: keep the real-CLI stock/Auto and native off/on comparisons over
the existing corpus, demos, projects, generated assemblies and adapter file
runs. Add explicit default-on versus --native=on, --native=off and
requested-policy diagnostics, including a child with the perf environment
variable set (which must not be described as proof of entry). Preserve
lifecycle, budgets, runtime-failure no-retry, typed refusal, error rendering
and named 0197 differences. A refused program remains on the stock VM before
effects. Fresh exact-hash acceptance runs under the build verification
account. Fix new lint findings; preserve the approved unchanged-file Clippy
baseline with no new diagnostics.

One signed plan and implementation commit on the rnx repository main; no new
branch or worktree. Publish only after acceptance and review. There are no
Rune changes or CGU1 per-record engine gates in this record. Run correctness
and the specified startup-cost measurement.

After 0198 lands, N1 starts with a read-only report of the exact assignment
RegionRefusal and the smallest sound inference rule change, with aliasing,
overflow, checked arithmetic, fuel and deopt witnesses. No assignment,
recursion or collection eligibility implementation belongs in 0198.
