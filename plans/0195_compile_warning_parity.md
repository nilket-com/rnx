# rnx 0195: compile-warning parity

Base: rnx 0194 (`6e9d4d8`). Authority: Rune W1 Step 0, chatd rune-w1
seq 23526. Rune W1 implements the compile-time warning analysis. This record
adds live warning comparison to every admitted census program and pins the
accepted Rune W1 source after its gates pass.

Compare warnings as a multiset of kind, source id, span, displayed message and
auxiliary cause, context and variant spans. Preserve multiplicity. Never read
stored warnings as expected output. Keep the current admission policy, runtime
observations and registration context unchanged.

The runner and session currently discard compile warnings on successful
compilation; they do not print in emission order. W1 adds no presentation policy.
If the pin changes, update every active manifest, lock graph and release constant
together, including tools/project and Jupyter. Run the complete cross-suite in a
fresh clone as rnxverify on STRIX after the final Rune source is available.

Land one signed plan commit and one signed implementation commit. Amend later
plan changes into the plan and implementation fixes into the implementation.
