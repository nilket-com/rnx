# ADR 0230: Reuse for masked F64 write-only outputs

Status: Accepted

ADR 0229 gave nullable single-operation F64 maps (const, add, sub, mul with
validity) a write-only `NumericOutput`. ADR 0228's `map_reusing` still
declines them: its reuse-only preparation admits flat and F64-driver outputs
only, so these calls drop `previous` and allocate fresh. Warm 100M calls
still take about 1,133 minor faults and 108–153 ms.

Decision: admit the ADR 0229 masked write-only route to `map_reusing`,
under ADR 0228's rules without change:
- exclusive whole-buffer donors only; any other donor is dropped before a
  fresh allocation;
- the needed length is at most the capacity, which is at most twice the
  needed length (checked arithmetic), and zero-length outputs never reuse;
- capacity is charged once, with the same refusal boundary as a fresh
  allocation (a capacity that does not fit falls back to fresh);
- no hidden retention.
The change is confined to the reuse-only preparation's eligibility and its
output selection. Plain `map`, `allocate_batch`, `masked_float::run`, the
primitives and the executor are unchanged.

Acceptance against published `8d104dd1`:
- `map_reusing` is bitwise identical to `map` for the masked shapes across
  the 0226 matrix and the ADR 0229 block/length matrix, both with reuse and
  with every fallback;
- plain `map` program vectors are exact, apart from reviewed fixed per-call
  pins, with no per-row or per-block slope. The instance guard and resolved
  identity pass;
- warm 100M reuse calls on the six nullable const/add cells (three layouts)
  take at most 1% of plain-map minor faults;
- in the ADR 0224 harness at 100M (5 process samples, warm), the reuse median
  is faster than plain `map` in those six cells;
- the ADR 0228 retention test passes for a masked donor;
- Miri is clean for the new reuse tests.

Stop rule unchanged: any plain-map per-row or per-block slope stops the work
for attribution, with no codegen-tweak loop; a rule exception needs the user.
