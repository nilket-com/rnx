# rnx 0016 evidence: a child's bytes

Implementation of `plans/0016_a_childs_bytes.md`, 2026-09-09, on Linux x86_64
against upstream Rune 0.14.1 with no patch. Status of the record stays
proposed until reviewed.

## What landed

- `src/host.rs`: the run is now separate from the decoding. `run_child`
  returns what the child left behind, `decode` turns a stream into text or
  says why it cannot, `host::process` decodes both streams, and
  `host::process_bytes` decodes neither.
- `src/method.rs`: `Bytes` joined the map, so a missing method on the new type
  is named rather than hashed.
- `src/complete.rs`, `src/inspect.rs`, `src/text.rs`: ten registrations to
  eleven, and the new path in the list whose description has to name a result.
- `tests/process_bytes.rs`: eight gates, new file.

## Gates

| Gate | Result | Where |
| --- | --- | --- |
| 1 output that is not UTF-8 is refused | pass: the message names the stream, the byte offset, and `host::process_bytes`; the same for standard error; nothing plausible-looking is handed back and no replacement character appears | `output_that_is_not_utf8_is_refused_and_says_where_to_go`, `standard_error_that_is_not_utf8_is_refused_too` |
| 2 truncation is not an encoding error | pass: a three byte character repeated past the limit, which the limit does not divide, comes back truncated with the two bytes of the cut character dropped and everything before them intact | `a_truncated_capture_ending_mid_character_is_not_an_encoding_error` |
| 3 the same tail without truncation is refused | pass: a small stream ending two bytes into a character fails, where the truncated one does not | `the_same_tail_without_truncation_is_refused` |
| 4 bytes wrong where they stand are refused even when truncated | pass: bad bytes at offset 6 followed by padding past the limit are refused, and the message names offset 6 | `bytes_wrong_where_they_stand_are_refused_even_when_truncated` |
| 5 one stream's truncation does not excuse the other | pass: a truncated standard output with a standard error the child ended mid-character is refused naming standard error, and the reverse is refused naming standard output; a stream whose own capture was cut is still excused, with the other stream whole | `one_streams_truncation_does_not_excuse_the_other`, `a_streams_own_truncation_still_excuses_its_own_tail` |
| 6 the bytes come back exactly | pass: a child emitting every value from 0 to 255 gives all 256 back, compared one by one | `process_bytes_returns_the_bytes_exactly` |
| 7 both report the same outcome fields | pass: for a child that exits 7 and a child killed by the timeout, the two produce the same line, including the unit a killed child's code arrives as | `both_report_the_same_outcome_fields_for_the_same_child` |
| 8 a missing method on `Bytes` is named | pass: `no method \`frobnicate\` on \`::std::bytes::Bytes\`` | `a_missing_method_on_bytes_is_named_rather_than_hashed` |
| 9 both are discoverable | pass: the completion candidates are the registered paths and the test compares the two lists, so the new path is a candidate by construction; its description names its result | `complete::host_source_tests`, `text::description_tests` |
| 10 the three ports are unaffected | pass: the first is still byte-identical, the second's nineteen comparisons agree, the third's eight cases agree | the three comparisons |
| 11 nothing regresses | pass: 78 unit tests, 17 runner diagnostics, 16 pseudo-terminal gates, 12 exit-status gates, 8 standard-input gates, 10 byte gates, 3 run-output, 2 upstream reproducer, formatting clean | whole suite |

## Controls

| Removed | Result |
| --- | --- |
| the decoding, going back to lossy | 5 gates fail |
| the requirement that the tail be excused only when truncated | gate 3 fails, and nothing else |
| the byte path's matching of how a code is reported | gate 7 fails, and nothing else |
| the per-stream flags, combining them again | gate 5's crossed pair fails, and nothing else |
| every excuse, never letting truncation forgive a tail | gates 2 and 5's positive half fail, and nothing else |

The second is the one worth naming. Decision 2 says the truncated and the
untruncated case must not behave alike, and dropping the `truncated`
condition makes them alike in exactly one direction: an incomplete tail is
excused everywhere. One gate fails, which is the pair that separates them.

The last two pin the same rule from opposite sides. Combining the flags is too
forgiving and fails the crossed pair; refusing every tail is too strict and
fails the cases a stream's own truncation should excuse. Neither breakage
fails anything else, so each names its own boundary.

## A defect this cut shipped: one stream's truncation excusing the other

`run_child` returned a single `truncated`, the two flags already combined, and
`process` handed that to both decoders. A child that wrote past the limit on
standard output and only the first two bytes of a character to standard error
came back with `truncated` true, `stderr` empty, and no error at all. The
standard error was captured whole; its bytes were dropped and nothing said so.

That is the same silent dropping the cut exists to remove, reintroduced one
layer up: strict decoding was in place and the flag telling it when to relax
was the wrong one. `Ran` now carries a flag per stream and `process` decodes
each against its own, while the record a script sees still reports the two
together, because a caller checking `truncated` wants to know that something
fell short rather than which half did.

Found in review. The eight gates all passed, and none of them ran a child that
wrote to both streams with only one of them truncated.

## A field the two nearly disagreed on

`host::process` builds its record through JSON, where a child killed before it
could exit has no code and arrives as unit. The first version of
`host::process_bytes` built its record directly and reported an option, so the
same child came back as `0` from one and `Some(0)` from the other.

Gate 6 is the reason that was caught rather than shipped. Two functions that
differ in one field invite exactly this, and comparing their reports for the
same child is cheaper than trusting them to stay in step.

## What this unblocks, and what it does not

The git lineage verifier in `plans/product/migration` reads raw tree objects,
where a twenty byte object id is not text. It can now read them. Whether a
byte string is enough to work with is the port's question, not this record's,
and record 0015's forward already names it.

It does not give a script a file as bytes. `host::read` still refuses what it
cannot decode, and no script has asked otherwise.

## Limits stated

- Two functions rather than one, differing in the type of two fields. Gate 7
  is what keeps the rest identical.
- The byte path decodes nothing, including standard error. A diagnostic that
  is not text is still the child's own, and a script that wants to print it
  has to decide what to do about that.
- The capture limit is unchanged, and `truncated` is still the caller's to
  check. Record 0015 is what made ignoring it fail.
- Record 0014's map now holds ten types. It is still a list rather than an
  enumeration, because Rune 0.14.1 cannot be asked.
