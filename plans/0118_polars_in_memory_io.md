# rnx 0118: in-memory I/O, through a closed table of std facts

Status: plan. Record 0117 is closed on `origin/main` at `120e37e`. It stopped its I/O stage because reaching a reader or writer needs std facts the inventory does not record. The user asked Claude and Codex to agree on the next cut, and this record is that cut (Codex, 0118 thread): **bounded in-memory I/O, with one closed std-facts model and the full 0116 `Sink`/`Cursor` contract**.

The starting v2 scoreboard is 3,902 available and 2,638 value-tested of 6,441 applicable callables (60.6%). `polars_io` has 381 missing rows. About 120 of them belong to the readers and writers generic over their source or sink: `CsvWriter` 16, `ParquetReader` 13, `ParquetWriter` 10, `IpcReader` 9, `IpcStreamReader`/`IpcWriter`/`JsonReader` 7 each, `AvroReader` 5, and more. Another 17 belong to `BatchedWriter`, and 5 to the `SerReader`/`SerWriter` trait rows.

Out of this record: general std trait inference, file paths, the Arrow layer, async and cloud readers.

## The gap

0117's stage C proves `R: MmapBytesReader` for `R = Cursor<Vec<u8>>` through `impl<T> MmapBytesReader for Cursor<T> where T: AsRef<[u8]> + Send + Sync`. That bound needs three facts about `Vec<u8>`:
- `AsRef<[u8]>`, which is a core trait with an argument;
- `Send` and `Sync`, which are auto traits.

The inventory records none of them. The generator deliberately models neither core traits with arguments nor auto traits. Writers need `W: core::io::Write` for the adapter's own `Sink`, which is outside the inventory by construction. The IPC stream and Avro readers need `Read + Seek`.

## Stages

Each stage is measured at both pins before the next begins.

**Stage 1: the census, before any emission.** The census covers the 126 rows 0117 stopped, plus the reader/writer owners listed above. For each row it computes whether a script path from construction to finish would exist with the table below: `new` or a constructor, then the options, then `finish` (or `write_batch`/`finish` for batched writers).

Every row gets exactly one of four dispositions:
- reachable;
- refused, with the missing fact named (by type and trait);
- refused, with the missing path step named;
- out of scope, as a file path, async or cloud row.

The census is committed and replayable, and emission covers only rows it marks reachable. A reader or writer with no complete path is refused by name, and the rest of the record moves on.

**Stage 2: `Sink` and `Cursor`, with their tests before any binding is scored.** These types come first, so that stage 3 can compile the facts about them. 0116's contract applies in full.

`Sink` is a script-visible handle, `polars::Sink`:
- It has a limit (the materialize limit when created) and a committed byte buffer, read with `bytes()` (Rune `Bytes`, so `fs::write` takes it directly) and emptied with `clear()`. The writer owns a clone of the handle, so the script keeps its own and reads the bytes after `finish`.
- **Staging and accounting.** Each writer operation (`finish`, `write_batch`, and so on) runs against a staging area. Every `write`/`write_all` checks the cumulative count (committed plus staged plus new) before copying. The first write that would exceed the limit returns an `io::Error` carrying a private `SinkLimit` marker, and copies nothing.
- **Commit or discard.** On `Ok` the staged bytes are appended to the committed buffer in one step. On any `Err` they are discarded, so partial bytes are never observable. Bytes from earlier successful operations stay, deliberately and visibly.
- **Error mapping.** The `SinkLimit` marker is found by downcast inside `PolarsError::IO`, never by message text, and maps to `MaterializeLimit` naming the operation. Any other error keeps its Polars kind.
- **Reuse.** The handle is reusable after an error.
- **Clones share one transaction (Codex, plan review).**
  - A `Sink` handle is a shared reference to one state: the committed bytes plus at most one operation's staging area. The writer owns a clone; the script keeps its own.
  - Every operation on any clone (a writer's `finish`, `write_batch`, and so on) and every `clear()` are serialized by that state's operation gate. An operation holds its transaction from its first write to its commit or discard, so committed + staged + new is always computed against one consistent state. **Amended in review (Codex, round 1):** a second operation or `clear()` on the same state is refused by name (`SinkBusy`, or "another operation is writing"), never interleaved. It does not wait, because waiting within Rune's single engine thread would deadlock. `clear()` claims the gate atomically and releases it on every path, a panic included.
  - `bytes()` and `clear()` see and change only committed state; staged bytes are never visible through any clone.
  - A failure discards only that operation's staged bytes. Bytes committed by earlier operations, from any clone, stay.

`Cursor` input:
- The script's `Bytes` are length-checked against the materialize limit before any copy, then copied exactly once into the `Vec<u8>` the `Cursor` owns.
- The script's value is untouched and no borrow escapes. A reader's own decoding allocations stay under Polars' limits.

These direct tests pass before any binding is scored:
- multi-write at exactly the limit, and one byte past it;
- the handle's contents and its reuse after a limit error;
- the discard of a non-limit error's staged bytes;
- the downcast mapping, including a message that merely says "limit", which does not map;
- the `Cursor` length bound at the limit and one past it;
- clones: after a writer (holding a clone) finishes, the script's own handle shows exactly the committed bytes; two writers sharing one sink, the first succeeding and the second failing at the limit, leave exactly the first's bytes; `clear()`, then another write through a clone, starts from empty; `bytes()` never shows staged bytes, checked by a writer that fails mid-operation.

**Stage 3: the `std_facts` model.** It is a release table:

```toml
[[std_facts]]
type = "std::io::Cursor<alloc::vec::Vec<u8>>"
trait = "std::io::Seek"
cite = "std io/cursor.rs: impl<T> Seek for Cursor<T> where T: AsRef<[u8]> (rustc 1.98.1 48a229cea; nightly-2026-09-20 1.100.0 feaadeeac)"
```

The table's rules:
- **A fixed allowlist.** The generator carries an allowlist of exact canonical keys (type, trait, trait arguments). A row whose key is outside it, is malformed (unparseable, not canonical), is duplicated, or has an empty citation is refused before any emission, and generation exits 2 with the row named.
- **The shipped rows:**

  | type | traits |
  |---|---|
  | `Vec<u8>` | `AsRef<[u8]>`, `Send`, `Sync` |
  | `Cursor<Vec<u8>>` | `Read`, `Seek`, `BufRead`, `Send`, `Sync` |
  | `support::Sink` | `Write`, `Send`, `Sync` |

- **Consulted only here.** `holds` uses the table only for these types, and only when no inventory impl decides. A fact outside the table stays unresolved exactly as today. Nothing is inferred from auto-trait structure, blanket std impls or derives.
- **Citations name the checked toolchains.** A std row cites the std source file and item as shipped in the two toolchains that build the adapter: stable rustc 1.98.1 (`48a229cea`) for the 0.55.2 production adapter, and `nightly-2026-09-20` (rustc 1.100.0-nightly `feaadeeac`) for the v2 build (`probes/0108/build.sh`). A `Sink` row cites its impl in the adapter's `support.rs`.
- **Every shipped fact is compiled at both pins, and it is said exactly when.** For each row the generator emits a compile-time assertion into the generated code, such as `const _: () = { fn holds<T: ?Sized + AsRef<[u8]>>() {} let _ = holds::<Vec<u8>>; };`. The facts are compile-proven when the adapter is built:
  - the production build under stable 1.98.1, the first time in stage 3 and again in every suite;
  - the v2 build under the pinned nightly, in `probes/0108/build.sh`.

  A table row that is false fails that build. `Sink`'s own `Write`/`Send`/`Sync` impls exist from stage 2, so every fact is compilable when stage 3 starts.
- **Controls:**
  - an unlisted key;
  - a malformed key;
  - an uncited row;
  - a duplicate row;

  each refused by name with no binding text; a fact omitted from the table leaves its bound unresolved; a mutation that consults the table for an unlisted type is caught.

**Stage 4: instantiation.** Each reader and writer the census marks reachable is instantiated as a synthetic wrapper, readers at `Cursor<Vec<u8>>` and writers at `Sink`, through a release listing like 0116's `dtype_instantiations`. 0116's reachability gate applies unchanged: a listing without a generated constructor and a result refuses generation.

Binding and proof:
- `SerReader::new`/`finish` and `SerWriter::new`/`finish` bind through 0117's stage B arms, and every `R`/`W` bound is proven by 0117's stage C, with the unique-candidate rule.
- `&mut dyn Read`/`&mut dyn Write` parameters take the same two types, or are refused by name.

Refused by name:
- a path- or file-taking constructor;
- an `Option<PathBuf>` option;
- an object store;
- an async reader.

**Stage 5: round trips, then scoring.** Focused tests take a frame through `Sink` bytes and back through a `Cursor` to a frame, for CSV, Parquet and IPC. JSON joins if the census marks it reachable. For each format:
- the bytes equal what Rust writes for the same frame and options;
- the read-back frame equals Rust's read of the same bytes;
- one option per format is respected (the CSV separator, Parquet compression, IPC compression).

Only then are the new bindings scored: oracle, audit, scoreboard.

## Proof

**Audit.** Every missing `polars_io` row at v2 gets one disposition from the 0118 surface:
- generated, with its binding count;
- refused, with its reason;
- out of scope: async 34, cloud, object store, file path;
- deferred to its owner.

The audit reconciles to the census. It is replayable (`probes/0118/verify.sh`), following 0117's pattern.

**Oracle** at both pins: every old status movement is reported separately, and new fixtures are in-memory byte sources, sinks and schemas.

**Suites and launch.** The usual suites run, debug last. Launch is measured against 0117 (`120e37e`) with a budget of +5 ms; the new wrapper types add registrations, and that cost is measured, not assumed.

## Stop rules

- Stop and report before emission if the census finds no reader or writer with a complete path.
- A reader or writer that needs a fact outside the table, or has no complete path, is refused by name, and the rest of the record continues.
- A compile-time fact assertion that fails at either pin removes that row from the table and refuses what depended on it, by name. It is never forced.
- Any movement of an existing entry's status, or of an oracle case outside the approved row-order policies, stops and is reported.
- Nothing here infers a std trait, opens a file path, or reaches the Arrow layer. A proof that needs any of them stops by name.
