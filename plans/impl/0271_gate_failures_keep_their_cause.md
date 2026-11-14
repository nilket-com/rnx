# Implementation plan 0271: a failed gate keeps its cause, and admission counts real memory

Status: Proposed

Paired ADR: ../adr/0271_gate_failures_keep_their_cause.md.

1. Pin d7596016. Read the final driver's terminal and release
   sequence, the stage runner the gates share, the fixed-route gate
   and its capture tool, the shared resource reading and every caller
   of it, and the Miri runner's admission.
2. Before editing, send a short note: where each failing receipt and
   log lives today and what release removes; the proposed retained
   layout and size bound; the kernel accounting fields proposed for
   headroom with the reasoning and a measurement from a limited unit
   before and after large builds; and every caller whose admission
   changes.
3. Implement the three decisions.
4. Add the controls of the ADR to the tooling's own tests so the
   final runs them.
5. Run the complete final (gate tooling changes), preflight and the
   commit checker. Bring the packet with a sample retained failure,
   the admission numbers from the live control and the per-stage
   durations. The candidate is frozen once the packet is sent.

Results are added here by the implementer.
