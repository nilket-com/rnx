# Implementation plan 0242: REPL sessions on the program domain

Status: Accepted

Paired ADR: ../adr/0242_repl_sessions.md.

1. Pin 61116a2d. Read src/session.rs, src/repl.rs, src/engine.rs, the
   ProgramDomain and DomainRoot APIs from ADR 0238, and session_blocker in
   src/routing_inventory.rs.
2. Bring a boundary note for review before editing:
   - where the session keeps engine-specific state, and the type that makes a
     mixed session unrepresentable (one engine per session epoch);
   - the domain API additions needed for binding merge, named record
     construction and enumeration, rendering and :vars, each with its
     ownership rule; no conversion through legacy Value;
   - the option spelling that selects the domain session;
   - the list of REPL operations refused on the domain session;
   - the transcript corpus format and how one runner drives both engines;
   - which required stages the path table selects.
3. Implement the session-owned domain, publication, diagnostics, limits,
   collection and reset. Extend session_blocker coverage rather than adding a
   parallel witness.
4. Add the differential transcripts, the refusal and lifetime controls, and
   the Miri subset.
5. Run the required stages, preflight and the commit checker. Bring the
   packet to review.

Implementation boundary:

- `InteractiveSession` owns exactly one boxed legacy Session or DomainSession.
  The REPL spelling is `--engine=execution`; default/auto/legacy stay legacy.
  Eval and run reject that spelling explicitly. No existing Session API is removed.
- Preparation is shared and pure. Async wrapper generation remains lazy in legacy;
  execution compilation is synchronous and never retries on the old engine.
- ProgramDomain adds named-record construction/lookup and borrowed bounded views,
  plus observation-only live/capacity/revision queries. These are outside the fixed
  operation-loop inventory. Binding owners are staged before delta publication.
- Every entry lease releases on success or error; idle collection follows
  materialized error text. Reset allocates a replacement domain before retiring
  its old epoch, keeping context, runtime and presenter ownership.
- Native presenters remain installed but are never invoked by execution sessions.
  Their native-value constructors are refused at frontend admission before host
  effects (including with the stock Dense presenter installed). Dependency restart
  commands refuse before preparation. Async and unknown-arity map(f) are separate
  refusal-policy controls, not equality rows.
- Pure-loop interruption checks occur every 1,024 fresh work units. Budget errors
  have the legacy category/text; exact exhaustion points are deliberately not
  compared across engines.
- New committed tests cover differential declarations/aliases/errors, retained
  closures from failed mutations, old origins after renumber, delta preservation,
  budget/cancellation recovery, bounded inspection at a latched ceiling, live and
  reusable-capacity plateaus, reset, foreign/stale handles and allocation refusal.
  The CLI test includes real SIGINT followed by another successful input.
- `repl_session_controls.py` uses the existing child receipts, test discovery,
  required-stage selection and finalization. The exact Miri inventory uses the
  existing per-test keyed cache; it introduces no separate Miri runner.

Development session, inspection, owner and CLI controls passed. Inventory recovery
passed with the noncolliding ProgramDomain::entry lookup; the existing function
lookup remains a compatibility wrapper.
Legacy session tests were not edited. Fixed-loop source and inventory are unchanged.
Final signed gates and publication: pending.

Review corrections:

- Refusals name the unsupported construct and original input location, with the
  remedy of restarting without --engine=execution. Async detection uses typed
  frontend refusal or a compile-only async-wrapper probe, never token matching.
- Cleanup failure cannot replace a primary diagnostic. Its secondary note is
  printed after the primary. An injected collection failure covers both primary
  failure and successful-input cases.
- The committed transcript table compares complete outputs and diagnostics;
  Wording rows explicitly retain the domain's additional generated-source stack
  frames. Policy rows pin the refusal and verify untouched bindings. Real SIGINT
  transcripts run separately in subprocesses against both engines.
- Repeated failed root reservations leave reusable empty slots, proved through
  capacity/live counts and the next successful handle's slot identity.
- Engine selection is an enum. Unknown REPL options and trailing arguments are
  now rejected rather than silently ignored; README discloses that change.

Review development gate: target/0242/DEV-v3/RESULT.json PASS.
