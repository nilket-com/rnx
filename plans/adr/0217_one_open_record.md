# ADR 0217: One open record at a time

Status: Accepted

Records must be worked one at a time and land in number order. Two records
were interleaved: 0211 landed before 0209, and 0216 was inserted inside 0215.
The resulting history reads out of order and invites batching. The user set the
rule: never batch ADR work; finish one record at a time, in order. Enforce it
in the root commit checker, not by convention.

After a new fixed cutoff at the 0215 implementation
`956ed4d5e89611ccb7140c54d4861390c94db49b`, require first-parent history to
alternate strictly. Each plan for record N is followed immediately by the
single implementation of N. That implementation is followed only by the plan
for the next record. A plan may stand alone only at the head, pending its
implementation. Each new plan number is exactly one more than the highest ADR
number present in the tree, counting both the root and `rune/` records.
Therefore numbers are taken when work starts, gaps cannot appear and landing
order equals number order.

A record that is abandoned still closes through its one implementation commit.
That commit changes only the record's own documents and sets the ADR status to
a withdrawn state. It never opens a second record, and the next number stays
sequential. A prerequisite discovered mid-record closes or withdraws the open
record first. It is never interleaved.

History before the cutoff is grandfathered unchanged. Keep `MAX_IMPLEMENTATIONS`
at one and all existing checks. No rewording, renumbering or relaxation. The
CLI gains no override; tests use isolated policy fixtures.

This is tooling only. Use synthetic repository controls for the following:
- interleaved plans;
- a skipped number;
- a reused or lower number;
- an implementation without its immediately preceding plan;
- two consecutive plans;
- a pending plan at the head;
- a withdrawn record;
- the cutoff boundary.

Also validate the real history. No runtime benchmark or Miri run is required.
