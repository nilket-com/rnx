# rnx 0031 evidence: inventory before integration

Checked by Codex on 2026-09-13. This is source-inspection evidence for a
proposed integration record, not an implementation or performance result.

## Sources and provenance

The module inventory was read from the published
[rune-modules 0.14.2 archive](https://static.crates.io/crates/rune-modules/rune-modules-0.14.2.crate),
whose SHA-256 is:

```text
fc8b73d4c6711eeb4dc57ea2cbf7b8b7ef2640edd19045f7c116441a0e713178
```

The relevant files are `Cargo.toml`, `src/lib.rs`, and the files named for
each module under `src/`. All nine optional modules listed by the record
are present: HTTP, JSON, filesystem, time, TOML, random, base64, process,
and signal. This inventory does not imply that a reported scratch benchmark
installed that exact set; its manifest and commands are needed to establish
that independently.

The async budgeting check used the locally cached published Rune 0.14.2
source, `src/runtime/budget.rs`. It implements `Future` for `Budget<T>`;
`poll` installs the stored budget, polls the wrapped future, saves the
remaining count, and restores the outer budget through its guard. Its
documentation explicitly limits enforcement to VM instructions unless a
native function cooperates.

The current host was checked in `src/runner.rs`, `src/session.rs`,
`src/json.rs`, and `src/main.rs`, together with records 0005, 0019, 0021,
0029, and 0030. No host implementation was changed during this review.

## Findings that affect the proposed record

1. **Registered HTTP verbs differ from implemented methods.** The module
   constructor registers `Client::get`, `post`, `put`, `delete`, and `head`.
   A Rust `patch` method exists but its metadata is not installed. Script
   coverage must test registration, not count Rust methods.
2. **HTTP is a starting point, not yet rnx's bounded operation.** The
   published module exposes no timeout setter or body-size-limit parameter.
   Its text and JSON response helpers delegate to reqwest, and its bytes
   helper collects chunks. This review does not establish reqwest's complete
   default policy; the host must explicitly select and test the desired
   deadline and decoded-body limits rather than assume they are supplied.
3. **Filesystem breadth is absent.** The filesystem module registers only
   async `read_to_string`; its implementation delegates to Tokio without
   rnx's existing read cap.
4. **Time is not datetime.** The time module supplies Duration, Instant,
   sleep, interval, and related operations. No wall-clock/calendar type is
   supplied there.
5. **JSON has an existing rnx contract.** `src/json.rs` bounds the recursive
   serialization walk and refuses cycles and unsupported values before
   invoking serde. Installing another serializer without that walk would
   expose a materially different behavior. Parsing remains separate work.
6. **A future-aware budget primitive exists upstream.** That reduces the
   amount of new machinery potentially needed, but does not prove rnx's
   sliced execution, diagnostics, or interrupt handling work after conversion.
7. **The session ceiling is sampled.** It checks tracked live allocation
   request bytes between commands; it is not an in-flight download cap.
   Existing accounting cannot justify unbounded new body-reading helpers.

## Preliminary adoption measurements

Claude reported the following on nano, pinned, with 100 runs:

| Scratch configuration | Reported elapsed time |
| --- | ---: |
| Default context | 3.5 ms |
| Context with selected companion modules | 3.9 ms |
| With a Tokio runtime and one async VM call | 4.3 ms |

Reported binary sizes were 9.2 and 13.9 MiB. These are attributed reports
from the team conversation. At this record's creation, the scratch crate,
runtime configuration, raw exports, binary hashes, link inspection, and
exact enabled feature set were not linked to the record. They are not
independently reproduced here. No build-time or real HTTP-request timing
was reported. The rounded figures do not establish a precise marginal
cost for an integrated rnx release.

## Validation performed

- Read the published source and checked HTTP registrations against its
  module constructor.
- Compared the proposed execution and memory requirements with rnx's
  existing synchronous paths and records.
- Checked the documentation diff for whitespace errors.

No implementation tests or new benchmark runs were performed for this
documentation-only record. Its implementation acceptance gates remain open.
