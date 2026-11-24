# Implementation plan 0332: Guardian controls observe exits exactly

Status: Proposed

Paired ADR: ../adr/0332_guardian_controls_observe_exits_exactly.md.

1. Use published 8e2370be and the guardian capture receipts to
   distinguish exit assertions from reaping assertions in the guardian
   suite and identify the write-then-exec path from attempt 100.
2. Pin the hung guardian and child before raising stop. Check both are
   live and then check immediate exit readiness independently of the
   production wait helper. Preserve explicit reaping checks.
3. Surface the drain's identification, signal and wait failures with
   available member identity. Keep PID reuse checks, finish cleanup on
   error, and apply the exit-event rule in both pidfd wait paths.
4. Replace the generated hung-guardian executable with an existing
   executable fixture and preserve the ready-child handshake.
5. Add the negative, event-mask, injected-failure and fixture controls.
   Run the one-time soak against the candidate binary and retain its
   receipt.
6. fmt, Clippy, preflight, the full final-controls command and the
   targeted checks under the final's real recipe. Sign. Source review,
   one complete final, Results.
