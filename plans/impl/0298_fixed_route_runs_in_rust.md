# Implementation plan 0298: Fixed-route runs in Rust

Status: Implemented

Paired ADR: ../adr/0298_fixed_route_runs_in_rust.md.

1. Pin 7991e30d; generate the witnesses from the published Python with
   controlled values, before any change.
2. `build_environment`, `capture_environment`, the recipe, `capture_key`,
   `reuse_anchor` and `verified`; the stage wrapper for Python children.
3. `rnx-final stage fixed-route`: the sequence, retention and receipt.
4. Catalogue, scheduler, producer lists, README; retire the Python with
   tombstones; move the remaining Python cases to Rust controls.
5. The Rust suites, the Python discovery, preflight and the commit
   checker; sign; source review; one complete Rust final; Results.

## Results

`fixed-route` runs `rnx-final stage fixed-route --out DIR`
(`tools/final/src/fixed_route_stage.rs`) over the ADR 0296 port, as a
producer the child contract admits. Its lane, exclusivity and bulk root
are unchanged. The gate's sequence, its recipe (`build_environment` and
`capture_environment` ported for it), `capture_key`, `reuse_anchor` and
`verified` are the published gate's. Its three Python children run as
before, through the ported stage wrapper and under the guardian, in the
checkout, with the interpreter fixed against the launch directory.
Every build receipt, the anchor's included, must name exactly two builds
with their commits, binaries and digests. An identity receipt must be an
object with an observation verdict. A non-mapping legacy recipe fails as
Python's `TypeError` did. Valgrind resolves from the explicit root, then
`RNX_VALGRIND_ROOT`, then a resolved PATH binary. Inside a final, the
stage binds to its context before any child starts. A standalone run
stops on SIGINT or SIGTERM, drains its child and publishes FAIL.

Two witnesses from the published Python at 7991e30d are matched
exactly:
- the recipe identities, 34 cases;
- the gate's whole sequence, 11 scenarios (every call, receipt key,
  output file and cache state over a planted host).
Further controls cover:
- malformed build, identity and legacy evidence (no panic, no capture);
- final-mode retention after the assigned payload is released;
- a readiness-driven standalone interrupt;
- a relative interpreter;
- Valgrind precedence, including a PATH symlink.
Each repair and refusal family has a mutation that fails its control.
Retired: `fixed_route_gate.py`, `drift_band.py`, `anchor_cache.py`, their
tests, `test_valgrind_resolution.py` and the gate case of
`test_executor_identity.py`; the Python list holds 297 files. All final
stages that publish a producer receipt now run in Rust. Python still runs
the preflight, test discovery and census controls, and the three
fixed-route measurement children.

The source review held the record once, for that evidence validation,
the retention and interrupt controls, and the interpreter's spelling.
The complete Rust final passed in 38.8 minutes with no memory.high,
memory.max or OOM events. All 101 stages passed, fixed-route in 576 s
(578 s in 0294). Its anchor cache was a `miss` with reason `key differs`
under key `65a428b5…`, against `b1277522…` in 0294. The recipe inputs
are identical between the two; only the anchor binary's digest differs.
The same miss has occurred in every complete final since 0282, under the
Python gate too: the anchor bytes differ across finals built under
different payload paths. An embedded path is the hypothesis, recorded in
the backlog to confirm; nothing is changed here.
