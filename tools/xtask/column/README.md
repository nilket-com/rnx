CK1 prerequisites run with one command, before any expensive gate:

    python3 -B tools/xtask/column/ck1.py --out target/ck1-development-NEW --notify

The output directory must be new. Add --probe for release probe controls and
--full for the full library suite, both after prerequisites. The staged-artifact
guard requires the probe target to be ignored and rejects build directories,
object/library suffixes and executable/archive magic in staged files under tools/. Formatting, dependency/feature closure,
standalone std/core primitive controls, Clippy and column controls run in that
order. Commands block on exit, with stdout and diagnostics in distinct files.
The final RESULT.json binds source hashes and the parent commit; development
receipts explicitly include uncommitted files and are not signed release proof.
Do not modify source while a job is running. --notify sends only a terminal
chatd receipt; no watcher or polling is involved.

The dependency baseline has no CK1 exceptions. Normal/build edges and enabled
features are checked for execution/native-code on x86_64 Linux and aarch64
Linux; the root manifest also pins optional dependencies and feature definitions.
The guard's negative controls inject normal/build dependencies and feature growth.
`dependencies.py --capture-new NEW` creates a review artifact; it cannot overwrite
the accepted baseline. Accepting a new dependency requires explicit review of
crate, exact version, enabled features and reason, plus measured benefit or a
substantial unsafe reduction. A future approved exception must also replace the
standalone no---extern proof explicitly; it cannot silently bypass that proof.

CK1.json holds the approved limits. Kernel limits include production block
dispatch through PreparedBatch::invoke, with allocation/validity preparation
outside that window. The script end-to-end headline includes those costs and is
report-only. The rnx adapter, pin, production CLI witness and README matrix are
in the separate rnx record. No publication follows from development controls.

After a clean signed implementation and its --probe prerequisite PASS, run:

    python3 -B tools/xtask/column/measure.py --prerequisite PATH/RESULT.json --out target/ck1-kernel-NEW --notify

This checks the exact commit/signature/source/binary bindings, uses the shared
CPU2 measurement lease and unchanged load/memory/thermal/PMU policy, and captures
4096/10M/100M actual rows. It records portable versus AVX2 controls, includes the
production batch dispatcher in every profile, enforces CK1.json only at 100M,
and preserves raw profiles on failure. A fixed compilation cooldown and fixed
inter-sample cooldown are declared, with no readiness polling/retry. Valid cycles
are userspace cycles; elapsed logical GB/s counts inputs plus output, not physical
DRAM traffic. Allocation/preparation metrics are recorded separately. These
kernel measurements do not replace the rnx script end-to-end headline.

The B1 ambient-budget record reuses these prerequisites and adds focused budget
controls before the full suite:

    python3 -B tools/xtask/column/ck1.py --budget --full --out target/b1-development-NEW --notify

No column kernel measurement is added by this flag. B1 receipts identify record
B1 and require its signed-source regression and fresh-clone/Miri checks before
publication. Launch services with an explicit Cargo PATH; remote controllers
also carry SSH_AUTH_SOCK and preflight BatchMode access before transfer.

B2 adds terminal host-budget-error controls with the same ordered prerequisites:

    python3 -B tools/xtask/column/ck1.py --budget --budget-halt --full --out target/b2-development-NEW --notify

The budget-halt flag adds constructor/predicate, actual VM equivalence and
uncatchable host error controls. Existing VM and budget paths are unchanged.
