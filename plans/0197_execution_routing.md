# rnx 0197: route one-shot programs through the execution engine

Authority: chatd rune-routing seq23650, seq23661/23666 and revised plan
seq23869. Base: rnx 6a067181, Rune E1 plus landed V4 (3eea6d30).
Prerequisite E1a is landed at 145e754086816eb479b0a07eddac02d4ce39819f;
all active rnx graphs and reported Rune versions use that accepted source.
M8 is closed by E1; this is product record one after that milestone.

One Context and one host installation serve either engine. Auto is the default;
`--engine=legacy` selects the stock VM before compilation. The prefix option
is forwarded by project and generated-assembly commands without entering their
cache fingerprints. `--debug-engine` reports selection and refusal category on
stderr only when requested. Rune native-code is built in; its runtime default
waits for the benchmark and the user's decision.

Route file runs, project/source-map runs, generated assemblies, adapter file
runs, the config evaluator and the startup probe. An unsupported source loader
selects legacy before effects. Otherwise compile, then typed prepare, then
invoke. Only a compile-time refusal or PrepareError::Refused selects legacy;
PrepareError::Invalid and every execution failure are terminal. Preserve source
read accounting and original sources, with no eager legacy compilation and no
repeated host installation. Keep eval, sessions, Jupyter, live :dep, all servers
and retained-Unit selfchecks on legacy.

Preserve rendering, error positions and escaping, styles, exit status and
lifecycle ordering. Read execution results through E1's borrowed RootView,
including identity-based path cycle detection and nominal field validation;
do not reconstruct legacy Values or use legacy Debug as a projector. Keep
Executor, Store and roots alive until rendering is complete, then release and
close before outer teardown. The default budget remains 2,000,000 work units.
Use typed remaining_work for exhausted diagnostics. Script formatting uses
permissive limits; preview limits remain separate.

Step 0 delta found one Rune follow-up: permissive cyclic formatting reaches
allocation failure, while legacy returns its typed depth-64 error. Resolve the
reviewed formatting policy in Rune E1a, including anonymous Object Debug
bounded markers using engine-owned addresses (seq24041). Validate marker
structure and raw addresses; any address-bearing CLI corpus row requires its
own nondeterminism ruling, never general normalization. Include live
self-cycle, mutual-cycle, repeated-alias and depth-boundary witnesses on both
engines. The same pair proves Refused and Invalid prepare preserve Store and
argument generations and counts. E1 already owns the executor API invariant;
remove rnx's Rune source parser and replace entry anchors with routing-wiring
assertions. Retain the method_policy/protocol_policy upkeep obligation.

Pin all active manifests, eight lock graphs and both RUNE_VERSION constants to
the accepted Rune source. Acceptance runs actual rnx CLI Legacy versus Auto
live over census, demos, CLI-compatible L1 programs and adapter file runs, with
native off and on. Use a hygienic entry adapter; compile/arity agreement is not
execution coverage. Require byte-identical stdout, stderr and exit status,
except the two approved census clocks allow-23/allow-24, reported separately
as nondeterministic-clock with existing live wall/monotonic predicates
(seq24018), the named unit-clone difference for SHA-pinned L1 rows01903,
01966 and 02213 (seq24062: exact legacy ()\n/exit0 versus new empty output/exit0
with live typed Unit root evidence, reported separately by row/count), and
named budget classes reaches-further/success, reaches-further/error,
budget-annotation and both-halt/further with two-way prefix proofs. Prove each
routed path is admitted through debug-engine evidence, and prove refusal before
effects with a counter. Include runtime-failure no-retry controls. No stored
expected execution output.

The Rune E1a prerequisite also promotes the closed M8 registry to 9ee355f3
(seq24029); it does not reopen M8. rnx builds native-code in and exposes
--native=on|off, initially off, through the approved per-Executor setter.

Run appropriate CGU1 per-record gates and fresh-clone acceptance by exact hash
on the build host. V4 is single-machine xtask only: remote provisioning and
exit notification belong in a tiny uncommitted operator script. Block on exit,
read terminal receipts once, and never poll. State duration before long gates.

After correctness, benchmark actual rnx run on the measurement host under an exclusive lease:
Legacy, execution/native off, execution/native on and LuaJIT. Commit equivalent
kernels for fib, loops, numeric/calls, shared_rows and admitted AWFY. Prove equal
outputs and safe integer domains first. Calibrate fastest mode to at least
0.5 seconds with startup below 5%; seeded interleaving, at least five repeats,
wall/instructions/cycles and spreads, with the cycle soft rule. The six paired
kernels are fib, loops, numeric_calls, shared_rows, AWFY Sieve and Permute.
Check all modes live and prove execution admission independently. Measure
full-source zero-round startup controls for every mode/kernel; every capture
must meet the duration and startup checks. Keep all intermediates below 2^53
and checksum output integral. Compute results
live; do not commit measurements. Report ratios to LuaJIT, native binary size
and build costs and a runtime-native recommendation for the user's ruling.

One signed plan and one signed implementation commit on local main; no new
branches or worktrees. Fresh acceptance clones by candidate hash; publish main
only after acceptance and review. No repeat M8 or V4 timing gate.

Result-shape ruling (seq24062): consume E1a ViewKind::Moved(Type), with only
type metadata and identity readable. Keep payload/field/child failures and
shared view budgets. The shared renderer uses its existing opaque type token;
no AccessConflict prose parsing or reconstruction. Unit.clone remains Unit;
do not import legacy's owned-empty-Tuple artifact or hide its difference in
byte-identical counts. Any census/demo occurrence requires a new ruling.

Scoped budget witness ruling (seq24173): a runaway native iterator count after
one printed "start" may stop differently in the two engines. Legacy reports a
located native-call budget error plus its exhausted-budget annotation;
execution reports one typed work-units halt. Keep exact exit 1, one start
effect and one halt report in both native modes, preserving the legacy
assertion under --engine=legacy and the execution assertion separately. This
named both-halt diagnostic difference applies only to this witness shape.
Do not broaden the corpus comparator; a census or demo occurrence requires
a new ruling.

Entry-boundary ruling (seq24195): until Rune exposes re-exported entry
functions on Program, use the temporary legacy_alias_entry_refusal wrapper.
A missing direct main plus a root AST import exposing main (alias, final
component, group or wildcard) yields typed Refusal::Import before prepare,
lifecycle or effects. Keep the same Sources and Context. A direct entry pays
no extra parse; an ordinary missing main remains terminal. Live CLI tests
cover each import shape and both native modes. Delete this wrapper when
"Rune: expose re-exported entry functions on Program" in BACKLOG.md lands.

Clippy ruling (seq24195): require no new diagnostics against the unchanged
base sources under the same toolchain. The 14 existing diagnostics are
complete.rs:161 clone_on_copy and :181 collapsible_if; host.rs:1130
drop_non_drop; presentation.rs:270 needless_late_init and :314/:317
if_same_then_else; session.rs:727 collapsible_if, :1704 manual_strip and
:1707 manual_pattern_char_comparison; text.rs:72 manual_is_multiple_of;
time.rs:175 collapsible_if; source_fmt.rs:289 and worker.rs:300
items_after_test_module; memory.rs:137 manual_dangling_ptr. Record the live
base and candidate Cargo JSON diagnostics and unchanged source identities.
Existing lint cleanup stays in its backlog record. Fix the six diagnostics
in new code, using local type aliases and narrowly documented
result_large_err allowances to retain Rune's allocation-free typed failure.
Project gate scratch and captures stay outside source and native roots.
