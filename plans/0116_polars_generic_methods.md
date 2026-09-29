# rnx 0116: generic methods, as one audited batch

Status: plan. Record 0115 is closed on `origin/main` at `6cea3a9`. This is the first parity record written against the family registry.

The target is the pinned Polars v2 Rust API (`da47b74`); 0.55.2 remains an active compatibility build. The starting v2 scoreboard is 3,761 available and 2,513 value-tested of 6,441 applicable callables.

This record decides every one of the **793** rows that the 0113 batch table assigns to `generic_methods`. It is one plan and one implementation, not a record per owner or per API.

## What the 793 rows are

`probes/0116/audit.py` assigns each row one family. It uses the refusal the v2 generator gives once the 0072 classifier's `generic` bucket gate is lifted: the generator's own mapping rules then decide, failing closed with a reason. The gate itself is not the blocker. Lifting it binds only 35 more rows, because the real causes lie underneath it:

| family | rows | cause |
|---|---:|---|
| `io_generic` | 126 | readers and writers generic over their source or sink (`CsvWriter<W>`, `ParquetReader<R>`, IPC, JSON, Avro, the batched writers, `ByteSourceReader`, `CompressedWriter`), plus `&mut dyn Read/Write` and `R`/`W`-bounded parameters |
| `chunked_pairs` | 109 | rows with a chunked owner (`ChunkedArray`, `Logical`, the chunked ops traits) or a chunked parameter or return, whose instantiation pairs are unproven or unemitted, or whose function-level generics sit outside the 0076 census; census only in this record (rule 5) |
| `alias_owner` | 58 | methods of a generic owner with concrete aliases, beyond `ChunkedArray`: `Schema<D>` as `Schema` (`DataType`) and `ArrowSchema`; `BinViewChunkedBuilder<T>` as `BinaryChunkedBuilder`/`StringChunkedBuilder` |
| `already` | 35 | bound by the existing mapping rules once the bucket gate is lifted |
| `dtype_owner` | 26 | builders with no alias, generic over a dtype: `PrimitiveChunkedBuilder<T>`, `ListPrimitiveChunkedBuilder<T>`, `CategoricalChunkedBuilder<T>`, `ObjectChunkedBuilder<T>`, `ObjectArray<T>` |
| `time_zone` | 24 | `chrono_tz::Tz` as a parameter or return |
| `generic_owner_other` | 8 | `NoNull<T>`, `SpecialEq<T>`, `DatetimeInfer<T>`, `BaseColumnUdf` |
| deferred to the batch table's owning rule | 366 | `trait_dispatch` 149 (no wrapped implementor); `value_grammar` 86 (lifetime types such as `AnyValue<'a>`, `chrono` naive date/time values, `Either`, `PlHashMap`/`PlHashSet`/`PlIndexSet` containers, bitflags and zip iterators); `internal_crates` 94 (unreachable or internal types, Arrow fields and arrays including `Box<dyn Array>`, plan arenas, `UnitVec`, compute sketches, object store); `callbacks` 21 (closure bounds, and output generics fixed only by a closure); `async` 16 |
| refused by name in this record | 41 | trait-object values `Box/Arc<dyn …>` with no script representation (18); generic inputs bounded only by an iterator, parallel-iterator or error trait (9); hasher state (5); the file-system boundary, `std::fs::File`/`PathBuf` (3); receiver shapes `&Arc<Self>`/`&GroupsType` (2); names already taken in the `polars::` facade (2); a `never` return (1); a `&'static str` parameter (1) |

There is no catch-all. `audit.py` fails on any row that matches no concrete cause. `probes/0116/partition.sh` replays this partition from pinned inputs:
- the 0115 generator (`6cea3a9`), run with the `generic` bucket admitted;
- the v2 inventory from 0112's bundle and `probes/0108/v2.toml`;
- 0113's batch table.

The audit's digest is `9db934a54fdb90a7`. `chunked_pairs` admits a row only when its owner, a parameter or its return is chunked, so no other generic-type refusal can fall into it.

The implementation's audit replaces "family" with each row's final disposition: generated (with its binding count), refused with its reason, or deferred to a named rule with its cause. Its totals reconcile to 793.

## Rules

Each family is one registry family, or claim, in its own module (the 0115 authoring rule). Each has one exact-shape gate and fails closed. A refusal carries its reason; nothing is refused silently.

**1. Admission past the bucket gate, isolated and measured first.** A `generic`-bucket row reaches the generator's mapping rules instead of stopping at the 0072 bucket, for inherent and trait methods and free functions. The mapping rules refuse what they cannot prove, and this is what the 35 `already` rows need. The 0072 bucket remains recorded, and `unsupported`/`unknown` stay refused.

This rule is implemented and measured **before any other rule**, at both pins, and on its own it must move no existing oracle case. Its evidence lists:
- the newly admitted rows, at v2 and in 0.55.2 production;
- their new oracle cases;
- confirmation that every existing case keeps its status.

A moved case stops this rule, and it is reported before anything else changes.

**2. Alias owners.** The 0076 instantiation census extends to every generic owner whose inventory has concrete aliases (`alias_target`). Each alias is a proven pair only when:
- its full head unifies;
- every impl bound is discharged with the existing proof;
- every associated item resolves from the impl record.

Methods are bound on the alias's wrapper, with the owner's parameters substituted, as 0076 does for `ChunkedArray`. A non-alias head is refused by name, for example a `Schema<D>` method that is only on `Schema<Field>`.

**3. Dtype owners.** A builder with no alias is instantiated per dtype from a release listing: owner, generic, types and citation, the shape of 0089's `free_instantiations`. Each instantiation gets a generated wrapper named for its dtype. The dtype list is proven like 0076's pairs; an unlisted or unproven dtype gets no wrapper. `&mut self` builder methods keep the existing mutation and move rules.

**4. I/O over in-memory buffers.** A reader generic over its source is instantiated at `std::io::Cursor<Vec<u8>>`, and a writer generic over its sink at a new adapter type, `support::Sink`. Every bound on `R`/`W`, including `MmapBytesReader`, `Seek` and `Send`, must be proven for exactly that type from the pinned sources. A row whose bound cannot be proven is refused with the bound named.

**No file-system instantiation.** A script reads or writes files through rnx's `fs` module and passes bytes, which keeps the adapter's file access where it is today. `&mut dyn Read`/`&mut dyn Write` parameters take the same two types.

**The `Sink` contract is operation-level.**
- **Handle and committed bytes.** A `Sink` is a script-visible handle, `polars::Sink`. It has a limit (the materialize limit when created) and a committed byte buffer that the script reads with `bytes()` and can `clear()`.
- **Staging.** A writer binding runs each Polars operation (`finish`, `write_batch`, and so on) against a staging area. Every `Write::write`/`write_all` adds to a cumulative count: committed plus staged plus the new bytes. It is checked before copying, and the first write that would exceed the limit returns an `io::Error` carrying a private `SinkLimit` marker, without copying.
- **Commit or discard.** When the operation returns `Ok`, the staged bytes are appended to the committed buffer in one step. When it returns `Err`, for any reason, the staged bytes are discarded. So a failed operation leaves the handle's contents exactly as they were before it: partial bytes are never observable. Bytes committed by earlier successful operations (earlier batches) stay, deliberately and visibly.
- **Error mapping.** The binding inspects the Polars error. If its I/O source is the `SinkLimit` marker (`PolarsError::IO` wrapping the marker `io::Error`, found by downcast, not by message text), the script gets `MaterializeLimit` naming the operation. Any other error keeps its Polars kind.
- **Reuse.** The handle is reusable after an error, and a later operation starts from the unchanged committed bytes.

**Cursor input.**
- The script's bytes are length-checked before any copy (0113's `vec_len_bounded` pattern), then copied exactly once into the `Vec<u8>` that the `Cursor` owns.
- The reader owns that copy. The script's source value is untouched, and no borrow escapes.
- A reader's allocations while decoding stay under Polars' own limits and are not counted against the sink.

**Direct tests.**
- **Multi-write, at the limit and one past it.** The CSV writer in batches, and one writer whose `finish` issues many small writes:
  - at exactly the limit, the operation succeeds and commits;
  - at the limit plus one, it fails with `MaterializeLimit`, and the handle's bytes equal their pre-operation value;
  - the same handle then accepts a smaller successful write.
- **Errors other than the limit.** A writer error that is not the limit (for example an invalid option) also discards the staged bytes, and keeps its own kind.
- **Cursor bound.** Exactly the bound is accepted, the bound plus one is refused before any copy, and the source value is reusable.

**5. Chunked pairs: census only.** The 109 rows get a per-pair census by cause:
- head mismatch;
- unmodelled trait argument;
- missing impl record;
- function-level generics outside the 0076 census;
- a mapping refusal.

Nothing new is generated for them in this record, except instances that an **existing exact rule already admits unchanged**, such as a pair whose only refusal was the bucket gate, admitted by rule 1. The census names each cause's candidate general proof, with its target keys, for a later record. That later record must specify the proof, its keys and its falsifying controls before generating anything.

**6. Time zones.** `chrono_tz::Tz` maps to a script string holding its IANA name. An argument is parsed with `Tz::from_str`, and an unknown name is a named error. A return is `tz.name()`. The mapping applies only where the pinned build has the type (the `timezones` feature). A 0.55.2 narrow build without it simply has no such rows.

**7. The other generic owners.** `NoNull<T>`, `SpecialEq<T>` and `DatetimeInfer<T>` are admitted under rule 2 or rule 3 when their instantiation is proven. `BaseColumnUdf` stays with callbacks.

## Proof

Each rule has a generator self-test that mutates one shape field at a time and asserts a named refusal, with zero binding text and zero registration. It covers:
- a wrong alias head;
- an unproven dtype;
- an `R` bound that is not discharged;
- an unknown time-zone name;
- a bucket-gate row the mapping refuses.

The registry order and trace controls from 0115 extend to the new families. `POLARS_GEN_TRACE` shows each family's coverage.

Focused behaviour tests compare script calls to direct Rust:
- a CSV, Parquet and IPC round trip through `Sink` and `Cursor`: bytes equal Rust's for the same frame, options respected, the bound refused at exactly the limit plus one, and the sink reusable after an error;
- `Schema` and `ArrowSchema` methods on real schemas;
- builders appending values and nulls, then `finish`;
- a time-zone round trip, including an unknown name.

The oracle covers every newly available callable that has a feasible fixture, at both pins. New fixtures are the in-memory byte sources, sinks and schemas.

**Evidence.** The 793-row audit with final dispositions, reconciled to the generated surfaces at v2 and 0.55.2. Every old oracle status movement is reported separately from the new cases. The replayable bundle follows the 0108–0115 pattern.

**Suites and budget.** The v2 build and oracle, the no-default stage, the three production suites (release, release with test-support, debug last), and three interleaved launch sets against the 0115 binary within the +5 ms budget.

## Stop rule

Stop a family when its first proof shows that the family would need:
- an unproven bound;
- an unbounded buffer;
- file-system access;
- a new trait model.

Finish the other families, and keep the stopped rows in the same audit with the exact reason. If the bucket-gate change (rule 1) makes any existing oracle case move, stop that rule and report before changing anything else.

No Markdown record per owner or per API.
