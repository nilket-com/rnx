rnx 0192 plan (Claude, 2026-10-25): move the Rune pin to the renamed execution engine and certify json::parse

Commit as plans/0192_<slug>.md (you pick the slug, e.g. rune_pin_execution), then one impl commit, per RULE #1. Subject-only commits: "plan: rnx 0192 ..." / "impl: rnx 0192 ...".

WHY. rnx pins Rune by git rev 0dcbba74 (C10, 2026-10-15) with the fresh-core feature. Rune has since renamed the engine surface (R2, f736a56b): the feature is now execution, Module::fresh_host_function is now Module::host_function, and dynamic host outputs must carry an output certificate (S7b1). These three cannot land separately. The new names need the new pin, and the new pin makes json::parse an undeclared dynamic output, so 9 census programs would fall back to the legacy engine unless json::parse is enrolled with OutputCertificate::JSON_RESULT.

CHANGE (goals; you design the edit):
1. Cargo.toml: rune rev -> f736a56b (or the pushed R2 head if it differs), features ["std", "fmt", "execution"]. Update Cargo.lock with the smallest change that moves the rune crates; report every other lock edge that moves (the 0190 lesson: list them, don't assume).
2. The 7 registration files (src/fs.rs, io.rs, json.rs, path.rs, text.rs, time.rs, web.rs): fresh_host_function -> host_function, plus the reg! macro engine tags in fs/web/text. Do not rename ordinary words or upstream names (Candle temp-dir helper, Polars names, script variables).
3. src/json.rs: register parse with Module::host_function_with_output(..., rune::host::OutputCertificate::JSON_RESULT). Confirm parse's actual output is exactly the certified shape (Result<Value, String> over unit/bool/i64/u64/f64/string/vec/object), with no other shape reachable.
4. Product prose and diagnostics: no "fresh" engine wording; say "new engine"/"execution engine" where needed. Never mention the site project (rnx stays generic).
5. Regenerate third-party notices if the lock moves crates that appear in them; the notices test must pass.

NOT IN SCOPE. No default-engine routing change: rnx still uses the engine it uses today, with the same fallback behavior. No adapter enrollment, no :dep thin-LTO, no json certificate for other functions.

CROSSING. The pin jumps across C11 through S7b1 and R2: store and frame defaults (S5, S6), runtime-refusal removal (S7a), output certificates (S7b1), wider admissions. Expect admission and observation changes in rnx-visible behavior only where those records approved them. Report every rnx test or demo transcript that changes, each with the Rune record that explains it. An unexplained change STOPs the record.

GATES.
- rnx root suite and all adapter suites that build against Rune; the three root demos and the checked notebook transcripts; the 0187/0188 host-enrollment tests.
- Census: run the real rnx registration (not the harness copy). All 244 admissions and observations must equal the S7b1 certified copy-only control byte for byte (the 9 json cases restored).
- A release build with the committed lock on nano (the canonical LLD environment; record the linker and rustc identity), plus a startup/demo wall-time sanity check against ea343850. Report it; no gate threshold.
- cargo fmt on every touched crate before the impl commit; the notices test.

DELIVERABLE. Evidence file plans/0192_<slug>_evidence.md: lock delta, census equality proof, the changed-transcript table with explanations, linker/rustc identity, and the wall-time comparison.

AMENDMENT (Claude, chatd rnx-0192 seq22993). Also align the compiler-only Rune pin in tools/project/Cargo.toml to f736a56b, preserving its [std, anyhow] features, and update its independent Cargo.lock as the eighth active Rune-bearing lock. Keep one Rune source identity per rnx build. Include tools/project tests; historical probe locks stay frozen.

PIN INVENTORY (implementation review). Synchronize the root/Jupyter Rune-version constants and release-metadata assertion with the actual pin, following the inventory in records0187/0188.

AMENDMENT (Claude, chatd rnx-0192 seq22998). Update two test-only expectations in src/runner.rs and src/session.rs for Rune D1 (a7d85572): preserve the linked named diagnostic when Rnx origin attribution is unavailable. Retain all origin assertions, assert structured hash identity, and distinguish the caller with an absent decoy method name. Record both changed expectations in the evidence.

AMENDMENT (Claude, chatd rnx-0192 seq23003). Update diagnostic expectations only in adapters/polars/tests/{arrow_values,clone_contract,json_boundary,protocols,wide_shim}.rs for D1/D1c/D1d. Package existing ignored 0072 inventory fixtures unchanged with hashes. The dense_alloc cap and test remain unchanged; 0192 waits for attribution and a ruling on the observed refusal-allocation failure.
