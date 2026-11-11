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

Results and publication: pending.
