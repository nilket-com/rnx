# ADR 0333: Finals start from the published baseline

Status: Accepted

Base: published ee52b405e9cea581c514ec87463faaa34d803b13 (ADR 0332).

## Problem

The complete final's 60-minute limit covers the whole run, including
compilation inside the stages. How much compiles depends on what the
shared Cargo lanes last built, and nothing fixes that state before
the clock starts.

The 0332 record shows the effect. Its first complete final on
6aca3ace passed all 107 stages and both fixed-route bands, then failed
the limit at 60.3 accepted minutes. The previous final in the same
lanes had built the `dbg!` candidate, a Rune change outside 6aca3ace's
history. Cargo build time across stages rose from 2,682 to 3,863
seconds against the 0331 final, while the fixed-route stage took 674
seconds against 677.

The 0332 Results then reported a second complete final on the same
commit as the passing final. Nothing had changed between the two runs
except the cache, which the first run had rebuilt. The second run
measured a warm repeat after that rebuild. Its 36.4 minutes do not
establish that the first run met the limit. That first run was 0332's
final: its checks passed and it failed the limit. 0332 stays published
on the strength of the first run's checks.

The limit's meaning also depends on the order of finals today: a
pre-sign run or a final from another branch changes what the next
candidate pays for.

## Decision

1. Baseline checkpoint. A checkpoint is defined by the published
   baseline's own build inputs, toolchain, recipes and compatible
   physical cache paths. Reserve the canonical final lanes for the
   published lineage. Pre-sign and other branch runs use separate
   writable state. The pinned toolchains are external inputs recorded
   in the checkpoint identity, and a mismatch refuses admission.
2. Reusable state. Every state a complete run can reuse, inside or
   outside the lanes, belongs to exactly one of two classes:
   - checkpoint state, such as the Cargo lanes, the bootstrap target,
     a final-owned dependency home seeded from the baseline's locked
     sources, and the fixed-route anchor cache, which follows
     Decisions 3, 5 and 6. Candidate writes to it, including new
     dependency fetches and anchor entries, stay private or staged
     and are timed;
   - invocation state, such as the project and image caches used by
     product tests, which is private to the run, starts empty and is
     timed.
   A developer's home caches and Cargo home are never read, adopted,
   written or deleted by a final. Measured paths stay as they are.
3. Preparation. Before any candidate work, hold the lanes
   exclusively and establish the checkpoint. A lane that is foreign,
   missing, interrupted or unproven is restored or rebuilt from the
   baseline's inputs alone. Until admission succeeds, preparation
   builds nothing else. A lane whose state can't be established is
   refused, never adopted. A recipe the candidate introduces starts
   from declared empty state where no compatible baseline output
   exists.
4. Clock. The 3,600-second end-to-end limit is unchanged. It starts
   at admission and covers the candidate's bootstrap, its first
   compile from the checkpoint, scheduling waits, the stages,
   fixed-route, finalization and release. Report preparation, the
   timed final and the total wait, which includes admission and lease
   waits, also on refusal. The final receipt binds the baseline
   identity, the preparation outcome and the monotonic boundaries, so
   revalidation can check them.
5. Ownership. Exclusive ownership is held continuously across
   verification, handoff and candidate writes. The lanes are marked
   in use before the first canonical write. After an interruption or
   a failure they stay ineligible for reuse until prepared again.
6. Promotion. Only a verified terminal, publication-eligible PASS can
   become the checkpoint for a matching published revision. That
   includes a docs-only successor with identical build inputs. Marks
   or a HEAD stamp alone can't authorize promotion. Only units with
   compatible inputs are promoted. Recipes whose inputs include the
   revision itself are prepared again on the published revision
   before the next final.
7. Overrun. A final that fails only the limit keeps its check results
   and is that candidate's final. Stop and take it to the user. A
   repeat on the same commit can't repair the verdict.
8. Scheduling control. The candidate's own pre-sign run failed
   `a_competing_client_delays_the_waiter_until_it_returns` once. A
   capture attributed it to the fixture, which is unchanged since the
   base. Its simulated competitor can take the last free token before
   the gate stage is admitted. The fixture then waits for a controller
   block that follows the gate, and the gate waits for that token, so
   the 20-second readiness wait expires. The scheduler lost no wake.
   Fix the fixture only: the competitor takes its token after the
   admitted gate has entered, with no cyclic wait. Keep the assertions
   that the waiter is refused while the competitor holds the token and
   starts after the release and before the returner ends.
9. Disk at admission. The first candidate's complete final, on
   16b0a07a, was refused 57 seconds into the clock, before any check
   ran. Cold preparation had filled the store to 112.7 GB and left
   118.0 GB free, and the unchanged disk rule needed 124.7 GB before
   the first stage. The host had 229.8 GB free at the start, so the
   outcome was fixed an hour before it was reported. Refuse such a
   run before preparation, by a bound on the preparation's remaining
   growth plus the run's first disk admission. Check again on the
   prepared state before the clock starts, by the controller's own
   rule. Both refusals name the free and needed bytes. Neither is a
   final.
10. Scope: the controller, its bootstrap, the cache handling, the
    cache environment of stage children and that one test fixture. No
    scheduler change. No change to the other checks, the bands, the
    measured paths or the stage catalogue.

## Controls

1. Inventory every reusable state a complete run uses, including the
   bootstrap, stage-owned, home-directory and dependency caches, and
   assign each to one class in Decision 2. For each one, show how
   ownership, leases and marks exclude foreign writes and the output of
   a failed candidate. Return any cache whose state can't be
   established to review before implementation.
2. Planted routes on small recipes:
   - foreign warm state is refused or restored before candidate work;
   - an interrupted or failed run leaves the lane marked for
     preparation;
   - a real baseline-to-candidate compile is charged to the clock;
   - an overrun stays FAIL even when the candidate's cache is warm
     afterward;
   - a late overrun or crash between the cache transaction and the
     terminal verdict leaves no checkpoint (the controller journals
     its transaction before its last wall check today);
   - a recipe that only the candidate has starts from declared empty
     state and is timed.
3. On the real catalogue, preparation from a cold lane and from a
   foreign lane must reach the same admissible starting state. Report
   each one's duration, bytes and peak disk use. These are baseline
   builds and need no candidate final. Don't assume a full copy of the
   final caches per candidate (about 106 GiB today). On first
   adoption, treat the existing rnx-final-dev lanes as unproven,
   including those written by the crossed dbg! pre-sign run.
4. Write boundary. A complete run under the final's real recipe
   leaves the developer's home caches and Cargo home unchanged, by a
   path, inode, size and mtime snapshot taken before and after it.
   Cold and foreign preparation reach the same state in every
   checkpoint class.
5. Scheduling fixture. Force the early competitor order as a
   regression control. With the gate handshake removed, the control
   reproduces the captured failure. With it, a bounded loop of the
   scheduling test binary under CPU load passes.
6. Disk refusal. With planted free-space readings, a shortfall is
   refused before preparation and again before the clock, each naming
   the bytes, and no timed final is recorded. The bound before
   preparation covers the growth the real cold preparation recorded.
7. Finals: targeted pre-sign checks in separate state, then one
   complete final on the signed candidate, reporting preparation, the
   timed final and the total wait.

## Constraints

- No new third-party crate. Tabs.
- The disk admission rules and the 20 GiB reserve are unchanged.
