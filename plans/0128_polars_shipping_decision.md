# rnx 0128: which Polars rnx ships (a decision, with a bounded probe)

Status: decision, as the user settled it on 2026-09-30.

**The question.** At v2 (Polars' `py-2.0.0-rc.2` tag, git `da47b74`), all eight probe workflows compute, match Rust and display. At 0.55.2, the shipped crate, three do (W2, W3, W5). Which Polars should rnx ship?

## The decision

**rnx keeps shipping Polars 0.55.2, the latest Rust crate release, and keeps v2 as its forward target.** v2 is a Python release candidate from a git commit, and no published Rust release carries its API yet (crates.io's latest `polars` is 0.55.2, checked 2026-09-30).

**The point to reassess the switch is a published Rust release that carries the API rnx targets** (review: not a crate literally numbered 2.0). It is not an automatic trigger. A switch then still needs the gates below:
- a stable build;
- a new inventory for the published crate's feature set;
- migration checks against today's shipped surface.

This decision changes nothing in the build.

## The probe (`probes/0128/stable_probe.sh`, bounded at 45 minutes per build)

Measured on stable Rust 1.98.1, 28 cores. rnx's MSRV is 1.95, and neither Polars pin declares a `rust-version`.

| case | what | result |
|---|---|---|
| A | Polars v2 alone, with the shipped adapter's features plus `pivot` | **builds on stable**, 117 s; resolves 17 features (adds `pivot`, `rows`) |
| B | rnx's hand-written adapter (no generated bindings) against A's v2 | **builds on stable**, 114 s |
| C | Polars 0.55.2 with `pivot` added: the full shipped adapter | builds, 249 s; resolves `pivot` and `rows`; **no new crate**, the lock unchanged; binary 175.6 MB (about +0.4 MB) |

**So the nightly requirement belongs to the Python wheel's feature set** (its `nightly` and `simd`), not to v2. A useful v2 configuration builds on stable. Generated bindings for that configuration were not built: they need an inventory for its feature set first (below).

## What a switch to v2 would cost (priced now, for the reassessment)

**Bindings that today's users would lose.** Of the 5,145 bound Rune paths at 0.55.2, 302 aren't bound at v2. These are v2 numbers under the wheel's broad feature set; a stable feature set would need its own count.

`probes/0128/classify_dropped.py` classifies them reproducibly from the retained inputs: the two surfaces, the v2 inventory, and the v2 source. Two runs give byte-identical results (`out/dropped.json`, one row per binding with its evidence). It separates an absent inventory path from a confirmed removal (review). A replacement is never inferred; it needs a named equivalent.

| kind | count | how it is established, and what it is |
|---|---|---|
| **confirmed removed** | 63 | not in the v2 inventory, and **no definition in the v2 source** of its crate, or its owning type is gone. These are `ChunkedArray::downcast_slices` and `is_optimal_aligned` (16 aliases each), `polars_io::predicates::ColumnStats` and `ColumnPredicates`, `DataFrame::slice_par`, `CategoricalNameSpace::get_categories`, `DslBuilder::with_context`, `append_trusted_len_iter`, and some others |
| **absent from the v2 inventory, present in its source** | 9 | moved, feature-gated or unreachable, to be followed up, not called removals: the `JoinOptionsIR`/`JoinTypeOptionsIR` impls (8; the types now live in `polars_plan::plans`) and `DataFrame::serialize_to_bytes` (a `Series` function of that name exists, so this is ambiguous). The name search is conservative: a name found anywhere in the crate counts as present |
| **lost in rnx (in the v2 inventory, not bound)** | 218 | exists at v2, but the generator doesn't bind it there (see below) |
| **lost for some types only** | 12 | `downcast_as_array`, `downcast_get`, `downcast_into_iter` and `downcast_iter` on `BinaryChunked`, `BooleanChunked` and `StringChunked`. v2 binds them for other array types |
| **replaced or renamed** | 0 confirmed | no replacement is claimed without a named equivalent |

**What the 218 are:**
- **`ChunkedArray` methods on concrete aliases.** `head`, `tail`, `limit`, `rechunk`, `chunks`, `downcast_chunks`, `layout`, `lhs_sub`/`lhs_div`/`lhs_rem`, the peak and arg-extremum helpers, `to_vec_null_aware`, and the float checks (`is_nan` and the like). 0.55.2's records 0085–0106 instantiated these, and v2's generic bucket hasn't ("no proven instantiation": 160; "generic type: ca": 44).
- **13 others,** and the user-facing ones among them matter:
  - `Expr::map`, `apply`, `map_many`, `apply_many`, and the free `map_multiple`/`apply_multiple`. All are refused at v2 by the callback audit as "unresolved". This is a callback-model gap in rnx, not a Polars removal, and **it is real lost functionality for scripts that use custom expression callbacks.**
  - `Column::agg_valid_count`;
  - the `UnknownKind` serde pair;
  - a `JoinOptions` conversion.

**The tested workflows affected: none.** All eight pass at v2 (`probes/0127`). The probe's steps use `head` and `null_count` only as `DataFrame` methods, not the dropped `ChunkedArray`/`ColumnStats` ones.

**Other costs of a switch:**
- **A git dependency on a release candidate:** `:dep` resolves it through the Git source 0067 proved, but the API may still change before release.
- **An inventory for the shipped feature set,** and a rekey: 0125's procedure.
- **The 218 lost bindings to re-earn:** mostly by extending v2's instantiation records. The callback gap needs its own record.
- **Fixed for users:** W1, because v2 has no 0.55.2 date-parsing crash, and W4, W6 and W7.

## The optional follow-up, separate from this decision: `pivot` on 0.55.2

Making W6 usable on the shipped build (priced by case C):
- **Build cost:** small. Two more resolved features, no new crates, about +0.4 MB of binary, and the build passes.
- **Binding cost:**
  - repeating 0125's procedure: a new inventory configuration (`adapter-pivot`) and a rekey, because rustdoc renumbers keys;
  - the `rows` feature may make new rows reachable, to be reconciled as `json` made `StructArray` reachable;
  - 0127's `[[arc_arguments]]` and `[[wide_bindings]]` rows re-cited at 0.55.2's source lines (`pivot` has the same signature, `frame/mod.rs:1795`).
- **Expected result:** W6 usable at 0.55.2, which would make W2, W3, W5 and W6 usable. Not proven until built.
- **W1 at 0.55.2 stays blocked** by Polars 0.55.2's own date-parsing crash, whatever rnx binds.

## Out of scope

- The Candle workflow: planned next, as its own record.
- The byte-based `JsonReader`, `rnx eval` display, and the `?` design pass, all deferred.
