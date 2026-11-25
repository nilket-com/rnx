# Implementation plan 0338: The final builds once and cleans up

Status: Proposed

Paired ADR: ../adr/0338_the_final_builds_once_and_cleans_up.md.

1. Inventory first. List every use of the checkpoint, the lane
   marks and the cache lookup in the final's tool, and say for each
   whether a verdict depends on it or only speed. Say what the
   tool's other modes do today and which are removed. Report
   anything that depends on the checkpoint for a verdict before
   editing.
2. Give the run one owned output directory, with private home, tool
   and temporary paths inside it. Group the existing sharing groups
   as target directories under it. Replace the cache lookup that the
   Jupyter stage uses and the early release of ADR 0335 with paths
   of the current run and the same release signal.
3. Move the fetch of locked sources, with the retired-source
   mappings and the Polars inventory inputs, inside the clock.
4. Keep the baseline gate inputs by commit with the checks of
   Decision 4. Run the lint baselines from source in the run's
   directory.
5. End every run with the copy-out, the drain, the clean and the
   removal of Decision 7, and write the cleanup evidence. Sample
   disk use as Decision 9 states. Add the limits record and the
   refusals of Decision 8.
6. Remove the preparation, its modes, the store's disk floor, the
   lane marks and sweeps and the cache snapshots, with the controls
   that existed only for them. Add the controls of the ADR.
7. fmt, Clippy, the tool's tests and the targeted checks in separate
   state. Sign. Source review, one complete final, Results. Report
   the elapsed time, what dominated it, the peak disk use and the
   limit that follows from Decision 8.

## Results

The checkpoint inventory found two consumers of baseline inputs. The
lint stages build baseline source inside the run. The fixed-route stage
can reuse verified images and anchor measurements from a published
commit. The controller revalidated the five complete receipts from 0333
through 0337 with the old store hidden.

Before signing, formatting and Clippy passed. The controller suite
passed its enabled tests with two ignored. The release controls
exercised Cargo cleanup on success, failure and interrupt, and recovery
after a killed run. An early scoped trial exposed empty target
directories that Cargo had never written. The release now cleans targets
marked by Cargo and removes unused directories with the enclosing run.

The census already fills the private work and report mounts to ENOSPC
at 64 MiB and 4 MiB respectively. The final measures aggregate cgroup
shared memory separately. Its verdict is independent of sampling a
short-lived case. Each run starts with a private Cargo home. FETCH.json
records the elapsed time and disk allocation there. A fetch failure
ends the run before any stage starts. A scoped cold fetch took 202.26
seconds and occupied 1,166,995,456 bytes in the private Cargo home;
this measures allocated storage, not network traffic. The bootstrap
keeps the measurement in its owned directory until receipt copy-out.

The complete final on a5ab9e9c failed before any stage, 203.4 seconds
after entry. DNS resolution for index.crates.io failed while fetching
the Candle manifest. Its bootstrap failure receipt has SHA-256
6c016c582c936b5c009ce64b308c367dad4555e1bb049d4a387d031873564366.
The successor keeps verified downloads under the persistent run area.
Each archive copied into the private Cargo home is hashed against
its lock. Git bundles supply checked objects for Cargo's locked
commit and tree checkout. Sparse index entries, archives and Git
objects survive; unpacked sources and compiler output stay private.
Every fetch first tries locked offline resolution. A cold store incurs
the online fetch time measured above. The scoped launch seeds the
same store used by the calibration final.

The complete final on 0a4ef605 failed in clippy and adapter-clippy.
Bootstrap fetched the candidate's locked sources offline in 6.7 seconds.
Both comparisons then tried to retrieve the retired Rune Git source for
the fixed lint baseline. Entry through cleanup took 1,286.9 seconds.
Cleanup released seven targets and removed the run. The result receipt
has SHA-256
2060ce609c5bda935dad8573f73716b6e2c0122fa98ed5218e216a3cd33e49db.

Bootstrap now shares the fixed lint revision with both stages and
fetches the published record and fixed-route anchor from their own
source exports. Cargo runs offline after fetch. A fresh private Cargo
home restored the verified downloads and fetched all 35 manifest
entries locked and offline. Both historical lint manifests resolved
Rune commit a68e739e6213dd02877258503f16df7ddaae6fc7 with checkout
tree f03268d541b1665d41737ce84d11b2ba42e49287. Removing the Git input
made the historical fetch fail offline.

The complete final on 91f9b555 passed both historical Clippy
comparisons. It failed in root-tests when the session interrupt fixture
bound its readiness socket. Stage 55 had an 86-byte temporary directory,
compared with 62 bytes in the published baseline's final. Entry through
cleanup took 2,409.4 seconds. Cleanup released 31 targets, removed the
run and left no new temporary entries. The result receipt has SHA-256
8ef5b402469e2922a1cff9f35072ef88d36316da34cc91ae32590f4302c1422f.

Clean stages now have a journaled temporary root at run/t. Stage 55
gets a 29-byte path at the default runs root. Fixed-route keeps its
stable recipe path. Entry counts the bytes in the root before claiming
it and reserves 47 bytes for the session socket suffix. The longest
admitted path, including the suffix, fits Linux's 107-byte limit.
All nine snapshot host tests passed with a 29-byte TMPDIR under the
real runs root. The same interrupt test reproduced the SUN_LEN error
with an 86-byte TMPDIR. Under the failed final's load, rune-suite ran
from second 116 to 2,379 and released its debug lane at 395. The
contracts took 1,142 seconds. Inventory ran from second 1,186 to
1,752 and polars-inventories from 2,071 to 2,345.

The complete final on 230792a5 passed 107 of 107 checks in 3,894.2
seconds through cleanup. Its result receipt has SHA-256
3534e95a562e99cb5e39c1f7aac4c47d8d06247bd6be9e6fc9f861d5da138eb6.
Fetch ran offline in 11.3 seconds. Rune-suite ran from second 115 to
2,144, including 950 seconds of contracts. Production CLI build ended
at 2,940. Fixed-route ran from 3,270 to 3,848 with baseline images
built on the clock. All 32 per-record rows were exact and all 32
cumulative rows passed. Build storage peaked at 120,918,523,904 bytes
and shared memory at 987,136 bytes. Memory events were zero. Cleanup
cleaned 37 targets and removed the private run.

Two release-failure fixtures left empty scratch directories in
/var/tmp. They now recover their own journaled roots after asserting
the failed verdict. The limits-bearing amend also failed commit
discipline because the calibration sidecar was outside the paired
Markdown documents. The checker now admits that exact file for the
0338 implementation. The fourth run's limits were not adopted.

A signed fixture history exercised the public docs-successor command.
It passed preflight and commit discipline when the amend first copied
the derived limits, and again when a further amend changed only the
Results text. Both successor receipts revalidated against the same
synthetic complete receipt. The rehearsal also exposed a missing
workspace lock in the docs bootstrap. It now fetches the controller's
workspace and the execution guard before building offline.

The complete final on af79d3e3 passed all 107 stages. It was the
measuring run of Decision 8, without a wall limit. Entry through
cleanup took 4,056.3 seconds; the 0337 final took 6,989.0 seconds
with its preparation. Rune-suite ran from second 116 to 2,347, with
1,132 seconds of contracts, and released its debug lane at 383.
Production CLI build ended at 2,904. Fixed-route ran from 3,436 to
4,012 with baseline images built on the clock. All 32 per-record
rows were exact and all 32 cumulative rows passed. The fetch stayed
offline and took 11.3 seconds. Build storage peaked at
120,921,960,448 bytes and shared memory at 1,638,400 bytes. Memory
events were zero under the performance profile. Cleanup cleaned 37
targets and removed the private run. A comparison of /tmp and
/var/tmp before and after the run found no new entry, so Control 8
is met. The gate inputs were retained with the receipt and indexed
for the next record. The result receipt has SHA-256
166ad51005a86f88496bbb3bbda8b354df1feae5e00c58371a12e906dadaa861.

The derived limits are 5,100 seconds, 151,152,450,560 bytes of build
space and 2,048,000 bytes of shared memory.
plans/impl/0338-final-limits.json is an unmodified copy of that
receipt's DERIVED-LIMITS.json. The byte limits are free space
required at entry. A running final stops on the wall limit or on the
fixed guard against low space.

Choices made within the record: Gate inputs are looked up by the
published baseline and never rewritten. Any mismatch is a miss that
rebuilds inside the clock. The measuring run is recognized by this
record's baseline and plan commit, which the tool fixes in code;
removing that branch is later work, as is a way for a later record
to change the limits. The runs root defaults to $HOME/.rnx-finals
because the system ages /var/tmp, and it must fit 32 bytes. The old
checkpoint store of 108 GB was deleted before the first final.
