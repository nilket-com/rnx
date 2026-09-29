# rnx 0118 evidence: in-memory I/O, through a closed table of std facts

Scripts now read and write CSV, Parquet and IPC through bytes at both pins. v2 adds Avro and IPC stream; the JSON reader is refused. A script can go `fs::read_bytes` → reader → frame, and frame → writer → `polars::Sink` → `fs::write`, with nothing in between.

`probes/0118/verify.sh` replays the evidence. It checks every digest and recomputes, byte for byte:
- the stage-1 census at both pins, from 0117's pinned inventories and surfaces;
- the audits at both pins.

## Scoreboard

| | 0117 | 0118 |
|---|---:|---:|
| v2 available, of 6,441 applicable | 3,902 (60.6%) | **3,988 (61.9%)** |
| v2 value-tested | 2,638 | **2,642** |
| v2 to 90% | 1,895 | 1,809 |
| 0.55.2 available (0077 scoreboard) | 2,675 | **2,735** |
| 0.55.2 value-tested | 1,808 | **1,816** |
| 0.55.2 unsupported | 1,287 | 1,227 |

Every status move is unsupported → generated: 86 at v2 and 60 at 0.55.2. No binding is lost.

The generic oracle has no in-memory byte or sink fixtures, so the new I/O bindings are value-tested by the focused round trips below, which are byte-equal to Rust's. The scoreboard's value-tested count does not include them.

## Stage 1: the census, before any emission

`probes/0118/census.py` selects rows by one rule at both pins, counting only rows the pin's 0117 surface lists as not generated. A row is selected if:
- its impl head is generic over `R`/`W` with an I/O bound, or it is the target of a `SerReader`/`SerWriter` impl; or
- it takes `&mut dyn Read`/`&mut dyn Write`; or
- it has a function-level `R`/`W` with an I/O bound.

**Reconciliation with 0116's 126 `io_generic` rows at v2:**
- one row only in 0116's set: `ByteSourceReader::from_memory`;
- three rows only in the rule's set: `get_reader_bytes`, `ndjson::infer_schema` and `read_until_start_and_infer_schema`.

The census takes the union.

**How facts are checked.** A bound is proven from the plan's fact rows, or through one recorded impl whose bounds reduce to them. `MmapBytesReader for Cursor<Vec<u8>>` goes through the single `Cursor<T>` impl to the `Vec<u8>` facts. Impl where-predicates are checked (review of stage 1), and lifetime-bearing owners are refused.

| pin | rows | predicted reachable | refused, path named | out of scope |
|---|---:|---:|---:|---:|
| v2 | 129 | 115 | 13 | 1 (a file-handle constructor) |
| 0.55.2 | 84 | 81 | 2 | 1 |

**Refused by path at v2:**
- `CompressedWriter` and `JsonReader` carry a lifetime;
- `ByteSourceReader::from_memory`, `count_rows_from_reader_par` and `read_until_start_and_infer_schema` need a `ReaderSource`/`Buffer` source that this record does not instantiate.

As Codex required, a predicted path counts only once it is proven by emission. The audit reconciles each one.

## Stage 2: `Sink` and `Cursor`

`support.rs` defines:
- **`polars::Sink`:** an `Arc` over one shared state, holding an atomic operation gate and a short-locked mutex of committed and staged bytes, bounded by the materialize limit.
- **`sink_op`:** a thread-local operation scope. Generated bindings establish it inside the engine closure, so it is on the thread that writes. The first write to a sink claims its gate; no lock is held across the Polars call (Codex's implementation note). Every write checks committed + staged + new before copying. The scope commits on `Ok` and discards on `Err` for every sink it touched, then releases the gates, panics included.
- **Fail-closed writes:** a write outside any operation is an `io::Error`.
- **Error mapping:** `From<PolarsError>` maps the private `SinkLimit` marker to `MaterializeLimit` by downcast, never by message text.
- **`cursor_from_bytes`:** checks the length of script `Bytes` before its single copy.

**Direct tests** (`support.rs` `sink_tests`, 9/9), several through real Polars writers:
- writes at the limit, and one byte past it, which copies nothing;
- cumulative accounting within an operation;
- reuse after a limit error;
- a non-limit error discards its staged bytes, and staged bytes are never visible through any clone;
- the marker is found by downcast, while a message that merely says "limit" is not the marker; a real `CsvWriter` over a small sink returns it;
- clones share one committed state: two writers on one sink leave exactly the first's bytes after the second fails; `clear()` works through a clone;
- a write outside an operation fails;
- operations are serialized: a second thread's write and `clear()` are refused while the gate is held;
- (review round 1) `clear()` claims the gate atomically, so a writer admitted in the old check-to-lock window is refused. A panic inside `clear` still releases the gate. The control is `clear_holds_the_gate_through_its_change`, and restoring check-then-lock fails it. The plan's "waits" is amended to "refused by name", since waiting within one engine thread would deadlock;
- the `Cursor` bound at the limit and one past it.

Two mutations fail them: always-commit, and no limit check.

## Stage 3: the closed std-facts table

**The allowlist** (`families/std_facts.rs`) holds 11 keys in the inventory's canonical spellings, each with its Rust spelling:
- `Vec<u8>`: `AsRef<[u8]>`, `Send`, `Sync`;
- `Cursor<Vec<u8>>`: `Read`, `Seek`, `BufRead`, `Send`, `Sync`;
- `support::Sink`: `Write`, `Send`, `Sync`.

**Validation.** A release row that is unlisted, non-canonical, duplicated or uncited exits generation with 2, the row named. `holds` consults the admitted set before its core-trait rules. Nothing is inferred.

**Citations** name each toolchain's exact source line:
- stable rustc 1.98.1 (`48a229cea`): `library/std/src/io/cursor.rs:13/48/128`, `library/alloc/src/vec/mod.rs:4342`, `library/core/src/ptr/unique.rs:50/57`;
- nightly-2026-09-20 (1.100.0 `feaadeeac`): the Cursor impls moved to `alloc/src/io/cursor.rs:12/108` and `core/src/io/cursor.rs:437`; `vec/mod.rs:4450`.

**Compile proof.** One compile-time assertion per fact is emitted into `functions.rs`. They compile in the production build under stable and in the v2 build under the nightly. A control shows that the same assertion form rejects a false fact (E0277).

**Controls:**
- self-test `std_facts`: the four refusals, and every allowed key canonical;
- `applicability`: `MmapBytesReader for Cursor<Vec<u8>>` is open without the facts, proven with them, and unproven with any one removed; a neighbouring type (`Vec<u16>`) is never matched.

## Stage 4: instantiation

Readers are listed at `Cursor<Vec<u8>>` and writers at `Sink` through 0116's `dtype_instantiations`. An I/O type may be listed without an inventory record, spelled by the adapter as `std::io::Cursor<Vec<u8>>` and `super::support::Sink`.

**Changes, each with a control in the `dtype_owners` self-test:**
- **Lifetime owners.** A lifetime-bearing owner is refused by name (`JsonReader<'a, R>` broke the v2 build before this check existed).
- **The reachability gate.** A wrapper also counts as constructed when a constructed listed wrapper's method returns it (`CsvWriter<Sink>::batched` gives `BatchedWriter<Sink>`), to a fixpoint. It is never constructed through an unconstructed parent. All unreachable listings are reported together.
- **Head parameters in method where-clauses.** A method where-clause on the impl head's own parameter (`ParquetWriter::new … where W: Write`) is the head's. `census::head_bound_generics` moves it to the impl predicates the proof discharges, for the census and for `emit_instantiations`. A true function generic is untouched.
- **Sink operations.** A call on a `Sink` instantiation, or with a `Sink`/`&mut dyn Write` argument, runs in `sink_op`, with its argument conversions hoisted before it. A non-sink call is not wrapped.
- **Argument mappings:** `Cursor<Vec<u8>>` takes `Bytes` (bounded, one copy), `Sink` takes a handle clone, and `&mut dyn Read`/`Write` take the same, as a temporary the call borrows.
- **Binding ids.** Receivers whose last path segment is a shared generic argument (`CsvReader<…Vec<u8>>`, `IpcReader<…>`) collided. Only colliding ids are extended with the receiver's own type name, and no 0117 id changed (checked).

**Refused listings, by name:**
- `ByteSourceReader`: no public path;
- the IPC batched writer: `IpcWriter::batched` takes `polars_arrow`'s `IpcField`;
- `JsonReader`: it carries a lifetime.

**The approved exception (Codex, stop-rule decision (a)).** `SerWriter::new` and `SerReader::set_rechunk` carry `where Self: Sized`, which rustdoc lists as a method generic.
- **The rule.** Each generic-trait arm now proves `Self`'s bounds for its exact receiver with `holds_all`, refuses the arm by name when they are unproven, and only then drops `Self`.
- **Its effect.** The rule is general, so four methods outside I/O also generate at both pins: `ChunkFillNullValue::fill_null_with_values`, `ChunkFilter::filter`, `ChunkFull::full` and `ChunkSet::set`.
- **Controls (self-test `generic_traits`):** `Self: Sized` is proven on a sized receiver; an unproven `Self` bound is refused with no binding text; an unbound method generic stays refused. A mutation that bypasses `holds_all` fails it ("Uns must be refused").

## Stage 5: round trips, then scoring

`tests/in_memory_io.rs` passes 5/5 at 0.55.2 and 5/5 in the v2 adapter:
- **CSV** with separator `;`, **Parquet** uncompressed and **IPC** LZ4: the script's bytes equal Rust's writer output for the same frame and options, and the frame read back equals Rust's read of the same bytes.
- **Limit and reuse.** A sink bounded at exactly the header-less CSV's length refuses the full CSV with `MaterializeLimit`, committing nothing. The same sink then takes the header-less write, byte-equal to Rust's.
- **The reader bound.** A reader's `Bytes` are bounded before the copy.

The 0117 focused tests (4) and 0116's (3) pass at v2 as well.

## The audit

`probes/0118/audit.py` gives each pin three sections:
- **census:** every row's final disposition, with `prediction_missed` rows named and never counted;
- **polars_io:** every row missing in 0117, one disposition each;
- **other:** every other status move.

| pin | census: generated | prediction_missed | refused as predicted | other moves |
|---|---:|---:|---:|---:|
| v2 | 77 | 38 | 14 | 4 |
| 0.55.2 | 51 | 30 | 3 | 4 |

The missed predictions are per-method mapping refusals, each named:
- a `polars_arrow` type in the signature (IPC/Parquet `schema`, `custom_metadata`, `with_arrow_schema_projection`);
- a hidden `PlRefStr` (`with_include_file_path`);
- `NonZeroUsize` (`CsvWriter::with_batch_size`);
- Parquet internals behind a `Mutex` (`BatchedWriter::get_writer`/`new`/`write_row_group(s)`);
- the IPC batched writer (`IpcField`);
- `ByteSourceReader` (no public path).

## Oracle

| pin | vs 0117 | added | removed | moved |
|---|---|---|---|---|
| 0.55.2 | `oracle-results.json` | 56 (55 match, 1 both_error) | 0 | 0 |
| v2 | `evidence/oracle-results-v2.json` | 67 (58 match, 2 both_error, 7 fixture_failed) | 0 | 5 row-order flips under the approved join/unique/upsample policies, and 1 approved fixture move |

**The fixture move, reported separately (Codex).** `ChunkShiftFill::shift_and_fill` went from both_panic to match. The binding is byte-identical in 0117 and 0118 (the same MD5 of its generated function), and so is the Rust call. Only the oracle setup changed:
- **0117:** the `ArrayChunked` receiver came from `full_null_with_dtype("x", 2, dtype(), 2)`, an array of width 2. With the width-3 fill value (`series()`), both sides panicked: "widths of FixedSizeWidth Series are not equal".
- **0118:** it comes from the newly generated `ChunkFull::full("x", &series(), 2)`, width 3, so both sides run and match.

The 7 fixture failures are the known `i128`/`u128`/`f16` recipe gap on the newly generated `ChunkFilter`/`ChunkSet`/`ChunkFillNullValue` receivers. They mean "not value-tested", never a mismatch. There are 0 mismatches.

## Suites, builds, launch

- **Generator:** 46/46 self-tests (+1 `std_facts`; `applicability`, `dtype_owners` and `generic_traits` extended).
- **Production:** the oracle and all three suites pass, debug last.
- **v2:** every stage is `ok` (build, oracle controls, no-default); the oracle reports `cases_failed` only for 0116's 2 pre-existing panics.
- **The `generated` suite is production-only.** It reads the 0.55.2 inventory, as in every earlier record, so it is not part of the v2 run.
- **Launch.** Measured with `probes/0073/launch.py`, 60 interleaved launches per set, against 0117 (`120e37e`), with a budget of +5 ms:

| set | old median | new median | delta |
|---|---:|---:|---:|
| 1 | 21.51 ms | 22.63 ms | +1.12 |
| 2 | 21.14 ms | 22.50 ms | +1.36 |
| 3 | 20.25 ms | 20.89 ms | +0.64 |

## Replays

`probes/0118/verify.sh` passes, and so do the 0108–0117 replays (below).

## A limit worth revisiting

The sink's bound is the materialize limit when it is created, as 0116's contract specifies: 1 MiB (`1 << 20`, shared with item counts). A script that writes a large Parquet or CSV file will get `MaterializeLimit`. `fs::read_bytes` allows 8 MiB, so reading is bounded at 1 MiB by the same limit. A configurable byte bound per sink is a candidate follow-up; this record keeps the planned contract.
