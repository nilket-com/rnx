# rnx 0119: the Arrow value layer, starting from the owned array bridge

Status: plan. Record 0118 is closed on `origin/main` at `e0c3878`. The user approved the Arrow value layer (option (a) of the parked Arrow note: `polars_arrow` values in Rune, a parity move) as this record. Arrow-rs interop (option (b)) stays separate and later. Claude and Codex agreed this shape before the plan (Codex, 0119 design critique).

The starting v2 scoreboard is 3,988 available and 2,642 value-tested of 6,441 applicable callables (61.9%).

## The measurement, and what it does not prove

The current generator was run at v2 from a scratch copy of `v2.toml` with `polars_arrow` added to `api_crates`.
- **polars_arrow rows:** 92 of 387 generated and 20 adapted; 275 stayed unsupported.
- **Other crates' rows:** 12 unsupported rows elsewhere generated.
- **Losses:** no previously generated row was lost.

That run relaxed the oracle's callback-recipe check in a throwaway worktree. It found a real hazard: a recipe calling `polars::Field::with_dtype` no longer resolved to the core `Field`. **So the scratch run is a census, not admission evidence.** Mechanically available rows do not prove that a script can reach an Arrow value.

## Stages

Each stage is measured at both pins before the next begins.

**Stage 1: a dedicated namespace, and frozen paths.**
- **The namespace.** Every wrapper of a `polars_arrow` type is registered under `polars::arrow` (Rune item `::polars::arrow`), whatever its short name, for example `polars::arrow::Field`, `polars::arrow::ArrowDataType` and `polars::arrow::Bitmap`. The wrapper builder today picks a namespace from the set of same-short-name candidates (`world/mod.rs`), so admitting Arrow types in the shared namespace could move an existing `polars::Field`, `Schema` or `DataType` path even if the Arrow type were renamed. The dedicated namespace removes that coupling.
- **Frozen paths.** Every pre-0119 Rune path and binding id is frozen. Generation compares them with the 0118 surface and refuses (exit 2) if any moves or disappears. A new duplicate Rune path also refuses generation.
- **The recipe check** is kept unchanged. A callback recipe that resolves to a different binding than in 0118 stops.

`polars_arrow` joins `api_crates` at both pins only under these gates.

- **Controls:** an Arrow type whose short name matches a core type is placed under `polars::arrow` and leaves the core path unchanged; a moved old path refuses generation; a duplicate new path refuses generation; the recipes resolve exactly as in 0118.

**Stage 2: the owned array bridge, before any concrete array.** The script path to an Arrow value is `Series::to_arrow(chunk, compat)` → an owned array → inspection and checked downcasts → `Series::from_arrow(name, array)`. This stage makes that path exist and proves it end to end.
- **The value.** `ArrayRef` is `Box<dyn polars_arrow::array::Array>` at both pins. It is wrapped as an owned, opaque `polars::arrow::ArrayRef`. Nothing claims cheap cloning or sharing. `Array::to_boxed` makes a new owned trait object, and a concrete implementation may share its reference-counted buffers with the original rather than copy them. What each concrete type does is verified per pin from the pinned sources before any clone is exposed, and recorded as found. It is never described as a blanket deep copy (review of the plan).
- **The object-safe surface, audited row by row before exposure:**
  - `len`, `null_count`, `is_null(i)`, `dtype`, `validity`, `sliced(offset, len)`;
  - every index-taking method gets a pre-call bound guard (0091's pattern), because `is_null`, `slice`/`sliced` and `to_arrow`'s chunk index panic out of range;
  - mutable accesses (`slice(&mut self)`) and any unchecked method are refused by name unless a guard makes them total.
- **Checked readers, which ship in this stage so that it stands alone (review of the plan).** Stage 2 introduces no concrete array wrapper. Its downcasts return **bounded snapshots**, built with the 0099–0106 snapshot helpers, and are named:
  - `values_i64`: an `Int64` primitive array as `Vec<Option<i64>>`;
  - `values_str`: the string view array Polars uses for strings, confirmed per pin, as `Vec<Option<String>>`;
  - `values_bool`: a boolean array as `Vec<Option<bool>>`.

  Each downcasts through `as_any()` to exactly one concrete type, and returns a `ConversionError` naming the actual Arrow dtype on mismatch; a downcast is never assumed. Each is bounded by the materialize limit before any copy, as the snapshot families are. Concrete array wrappers and their own readers are deferred together to stage 3.
- **Round trip:** `Series::from_arrow` and `from_arrow_chunks` take the owned arrays back.
- **Proof, by an end-to-end value test at both pins:** a script builds a Series (numeric with nulls, string with nulls, boolean, all-null), takes each chunk with `to_arrow`, and inspects its length, nulls, validity and dtype. It reads concrete values and nulls through the matching reader, is refused on a wrong reader with the dtype named, and round-trips into a Series equal to Rust's. **The all-null chunk** is expected to carry the `Null` dtype; a typed reader is called on it only as a checked-refusal case. The out-of-range chunk index is a named refusal, and so are `is_null` and `sliced` past the end.
- **The 0099–0106 snapshot families** (`chunks()`, `downcast_*`, `layout()`) are unchanged, and their tests pass unmodified. They copy into Rune values; this bridge sits beside them.

**Stage 3: the concrete generic arrays, only if the record stays bounded.** `Utf8Array<i64>`, `BinaryArray<i64>`, `ListArray<i64>` and `PrimitiveArray<T>` would be listed per instantiation through `dtype_instantiations` and 0118's reachability gate. They are constructed only by checked downcasts from the stage-2 value, and bring their own readers with them. Stage 2 does not depend on this stage.
- **The boundary, decided deterministically after stage 2's measurement (Codex).**
  - **Defer to 0120:** if stages 1–2 at either pin add 150 or more new rows, or their audit stops being reviewable, 0119 closes on the proven bridge and the whole concrete-instantiation batch moves to record 0120 intact.
  - **Include in 0119:** stage 3 is included only when both pins' audits and the launch budget remain small with it included.
  - **In both cases** it is never cut down to fit.

## Proof

**Per-row delta.** Every row whose status moves at either pin is listed, with its disposition and cause. Two figures are kept apart:
- **callable availability**, meaning generated;
- **script reachability**, meaning a generated path from a Series to that value exists (the stage-2 bridge or a downcast from it).

Rows available but not reachable are named, and are not presented as usable.

**Every final Arrow row gets a disposition.** The scratch census's 275 refusals are a baseline, not a denominator (review of the plan). Every `polars_arrow` row in the final surface at both pins has one disposition: generated (with its reachability), refused (with its exact cause) or deferred (to 0120 or its owner). The audit reconciles to the surfaces.

**The 12 cross-crate gains** (`DataType::from_arrow_field`/`to_arrow_field`, `Field::to_arrow`, the readers' `arrow_schema`, `schema_to_arrow_checked`, `materialize_empty_df`, `apply_projection`, `ensure_matching_dtypes_if_found`, `From` for `Field`) are each audited for ownership and safety before they are scored. A borrowed or unchecked one is refused by name.

**Oracle** at both pins, with every old status movement reported separately. Arrow-value fixtures come from Series chunks through the bridge, so no new data path is needed.

**Suites and launch.** The usual suites run, debug last. Launch is measured against 0118 (`e0c3878`) with a budget of +5 ms; this record adds many wrapper types, so the registration cost is measured.

**Carry-over.** The 0118 review nit, the `sink_tests` assert message that says "clear waits" where the behaviour is refusal, is corrected in this record's implementation commit.

## Stop rules

- Any pre-0119 Rune path, binding id, status or oracle case moves outside approved row-order policies: stop and report. Approved named exceptions only.
- A callback recipe resolves differently from 0118: stop.
- An Arrow type needs a lifetime or a raw pointer to be exposed: refuse it by name, and continue.
- A clone or sharing claim that cannot be shown from the pinned sources: refuse the clone by name.
- The bridge's end-to-end proof fails at either pin: stop before stage 3.
