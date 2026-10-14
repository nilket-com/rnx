# rnx 0187: enrol closed host functions for fresh admission

Advance the root and project-tool Rune pin from
`f98eb825934d4994478a1759cf340e90d8c5f12f` to
`27d8b6711fa15a20726ebefe4c6ed3934a56190b`. Enable `fresh-core` on the
root dependency, preserving `std`, `fmt`, the public Rust 1.95 declaration
and stock execution. H1a installs the same Rust handler with an admission
certificate. This record changes registration, without changing the runner
or adding fallback routing.

## Exact enrolment

The allowlist contains 30 functions:

- fs: read, exists, absolute, cwd, temp_dir, mkdir, mkdir_all, copy, rename,
  remove_file, remove_dir, remove_dir_all.
- path: join, parent, file_name, file_stem, extension, with_extension,
  is_absolute, separator.
- time: now_ms, monotonic_ms, format, rfc3339, parse.
- text: find, group_digits.
- io: stdin (host::stdin_read), eprint (host::eprint).
- web: decode_component.

Every other registration remains ordinary, including process::exit,
extensions and test-support. Each selected body and its helper chain is
audited separately for the trusted H1a promise: no stock VM interface,
captured Rune Value, callback into Rune, or reentry. Standard operating-system
state is allowed; synchronous host duration and standard-library allocations
remain outside script fuel and Store budgets. This is distinct from signature
eligibility.

## Pin inventory

Update Cargo.toml, tools/project/Cargo.toml, the root and Jupyter RUNE_VERSION
constants, and the release-metadata pin assertion. Prepare all eight active
locks: root, tools/project, adapters/polars, adapters/candle,
adapters/postgres, demos/polars, demos/candle and servers/http-postgres.
Historical probe locks stay out of scope. Jupyter has no Rune lock entry.
Use the record 0186 template/generated-fixture procedure in validation; review
any resulting output change explicitly. Candle is compile-only.

## Reviewed stages

1. Commit this plan locally, prepare the exact dependency/registration patch
   and per-body audit, and submit the uncommitted source for Claude review.
   No compilation, subject execution, measurements or push in this stage.
   A textual lock draft is not a Cargo resolver result; resolve and verify
   locks in the next authorized stage before compilation.
2. Validate the root suite, relevant standalone workspaces, Jupyter pin test,
   formatting, Clippy, current toolchain and the declared Rust 1.95 floor.
   Run the host-aware compile census using the actual serve context, without
   extensions. Compare registration before/after on the same Rune revision,
   features, locks, sources and toolchain. The old-to-new pin compatibility
   comparison is separate. Stock-compiling inputs are the denominator;
   failing inputs and overlapping H0 cohorts are reported separately.
3. Freeze and review a separate hermetic execution corpus for all 30 functions,
   including errors and String/Option/Result outputs, before any subject run.
   Do not execute the live H0 corpus. Retain the exact compiling Context's
   RuntimeContext and set_host_runtime in the isolated fresh harness. Absent,
   wrong, cleared and unused-host-table controls must have no effects.
4. Execute the reviewed corpus with identical filesystem setup at the same
   absolute fixture path; use fresh children and explicit pipes for stdin and
   capture actual stderr bytes for eprint. Clocks use source/body equivalence
   and bounded observations, not forced equal timestamps. Compare typed value
   and error projections with H1's retained arity, fuel and resource differences.

Work in rnx-0187 from origin/main 8c89b76f with the reusable rnx-0185 target.
Check disk/quota before later builds. Preserve failed attempts and source pins.
Signed subject-only implementation commits follow completed gates and review.
No benchmark-performance claim follows from compile admission or body fit.

This plan incorporates Claude's R0187 proposal and the six binding amendments
accepted in chatd seq 12510, following Codex's review seq 12503. H1b dynamic
value returns remain a separate design study after this record's census.
