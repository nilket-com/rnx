# Implementation plan 0346: Gate rules for declared and loader changes

Status: Proposed

Paired ADR: ../adr/0346_gate_rules_for_declared_and_loader_changes.md.

1. Boundary: the published base. Find what the dispatch
   observation records today and what the loader rule needs from
   the profile. Report anything in Decision 2 that the kept
   profiles can't support before building.
2. Declared transitions as Decision 1 states, with Control 1.
3. The loader rule as Decisions 2 and 3 state, with Control 2.
4. Score the two kept pairs again as Control 3 states.
5. fmt, Clippy, preflight and the targeted tests. Then the whole
   final-controls stage by its stage command. Sign. Source review,
   one complete final, Results.

## Results

Both retained pairs, `file` and `file3`, gave the same result for
`shared_rows` with native code on at sizes 1 and 5. The outer loader
count rose from 714,207 to 714,345 instructions. The excluded stack
query contributed zero loader instructions and inner setup contributed
36 on each image. The loader therefore rose from 714,243 to 714,381
against a per-record allowance of 3,571. The rest of setup changed by
zero against its allowance of 11. Ten added relative relocations
accounted for 120 instructions and loader symbol lookup accounted for
the other 18. The earlier rule allowed four added relocations.

The replay verified the retained outer, query and inner setup profiles
against their recorded hashes. Of the 32 per-record verdicts in each
pair, only the installed native-on T verdict for `shared_rows` changed.
All 32 passed. Each pair's 32 cumulative rows reproduced the earlier
scores exactly. The loader and remaining setup each have to pass even
when their changes cancel in the total.

The policy adds an empty `source_transitions` list. In
`relative_relocations`, version 2 keeps the loader path and digest and
removes `symbol`, `unit_Ir`, `max_added` and `on_loader_change`.
The two recipe witnesses use the new capture and accounting script
digests and keys derived from those digests.

The candidate and published base have identical product source.
A replay using the verified 0345 product executable matched all
6,404 baseline observations. Of 3,202 cases per native mode, 70 ran
successfully only on the legacy route in each mode on both revisions.

The complete final-controls stage command passed 318 tests with two
ignored. Earlier pre-sign attempts exposed stale recipe witnesses and
three local environment differences: an overridden Cargo target path,
a missing Valgrind root and insufficient free space on the temporary
filesystem. The passing run used the default target path, the installed
Valgrind and a private directory on the root filesystem. Formatting,
Clippy, preflight and the ten instruction-accounting controls passed.

The complete final ran on 812e98f6 as FINAL-v1 and passed all 107
stages. RESULT.json has sha256
9a2b853a35f1bb6e22d548d98c9b78254233a3d2391bf2ede567656664255faf.
Entry through cleanup took 4,274.3 seconds against the 5,100 second
limit. The run entered with 229,454,839,808 bytes free against the
151,152,450,560 required. The longest chain was the gate, which ran from
3,650 to 4,229 seconds. The Rune suite ended at 2,127 seconds with 918.0
seconds in the contracts. The capture and accounting scripts changed, so
no published gate inputs matched and the gate rebuilt and measured the
base on the clock. The anchor lookup missed and the anchor build took
96.6 seconds. The source condition passed over 40 items with no
transition declared and none needed. All 32 per-record rows passed and
all 32 were exact. The eight installed native-on T rows were scored
under version 2, with the loader and the rest of setup each unchanged.
All 32 cumulative rows passed. The fetch stayed offline and took 12.6
seconds. The build peaked at 120,982,999,040 bytes and shared memory at
5,668,864 bytes. The cleanup cleaned 37 targets and removed the run. The
memory controller reported no events. The run left no new entry in /tmp
or /var/tmp.
