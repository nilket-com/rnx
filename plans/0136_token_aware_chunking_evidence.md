# rnx 0136 evidence: token-aware chunking

**The result:**
- **What a script can do now:** a script can count tokens (`enc.count_tokens(texts)`) and split a document into passages the model takes whole (`enc.chunk(text, overlap)`).
- **D1 and D2:** passage byte ranges are **identical to an independent twin's,** coverage is complete, and **no passage is truncated** (0133 F14).
- **U1′:** 0133's U1 with `enc.chunk` in place of the 180-word rule, run as a session paste. Its complete output equals the twin's bit for bit at both overlaps.
- **Quality on the frozen rubric (reported, not gated):** hit@1 rises from 0133's 3 of 5 to **5 of 5 at overlap 0** and 4 of 5 at overlap 32. hit@5 stays 5 of 5.
  - **What this doesn't show:** five queries on one corpus show that the change helped here, not that it generalizes.

## 1. The surface

**The operations** (`adapters/candle/src/text/chunk.rs`, registered on `TextEncoder`):
- **`count_tokens(texts)`:** each text's count with the model's special tokens, untruncated, under `embed`'s preflight.
- **`chunk(text, overlap)`:** the plan's amended contract (`plans/0136_token_aware_chunking.md` §1):
  - **Tokenizing:** one per-call tokenizer copy with no truncation or padding (`embed`'s configuration is untouched), inside the joined worker's panic boundary.
  - **Atoms:** UTF-8 byte offsets, validated, grouped into atoms (repeated, overlapping or empty ranges join one atom).
  - **Passages:** words by word id; whole words within W = `max_seq` − 2 = 254 content tokens; an atom fallback for a word that can't fit; a named atomic refusal for an atom above W.
  - **Repair:** re-measure the borrowed substring with specials and drop the last word, at most 16 times.
  - **The next start:** from the repaired end, strictly advancing, with no gaps. Overlap is a target, and the actual overlap is recorded.
  - **Bounds:** every byte range is checked against named limits before any string is built: count `MAX_TEXTS`, each passage `MAX_TEXT`, total `MAX_TOTAL`, tokens `MAX_TOKENS`.

**One departure from the plan, found by the first real run and amended into the plan:**
- **The finding:** D1's largest record is 102,776 bytes, and `chunk`, bounded by `embed`'s per-text 64 KiB, refused it.
- **The change:** a chunked text is a whole document, so it has its own bound, `MAX_DOCUMENT` (1 MiB), and the token table is bounded at 2^18. Each passage is checked against `MAX_TEXT`, so it remains a valid `embed` input.
- **`count_tokens` keeps `embed`'s preflight,** as the plan says. The whole-document truncation numbers below count a text above 64 KiB by its chunk plan (content tokens plus the 2 specials).

**Review round 1, two findings, both fixed:**
- **R1, `count_tokens` built its tables before checking the list's length.** At the Rune boundary, 131,072 shared strings allocated 5,242,960 bytes before the refusal.
  - **The fix:** the count is checked first, as in `embed`.
  - **The control** (`tests/count_alloc.rs`, a single test through a real Rune VM so no parallel test moves the allocator peak): the refusal allocates under 64 KiB, and an empty list is refused the same way. It failed before the fix, at 5.2 MB.
- **R2, tokens with no span were silently dropped.** Under a byte-level tokenizer with trimmed offsets, three spaces encode to three Ġ tokens at empty ranges, and `chunk` returned an empty, successful plan.
  - **The fix:** such tokens attach to an atom when one exists. When none does, the call is a named atomic refusal, because a passage is a substring and can't hold them.
  - **The atom table must hold every token** of the encoding (the atoms' token total is compared with the encoding's length), so coverage can't credit discarded tokens.
  - **The twin** has the same correction and prints a refusal row instead of omitting a text.
  - **The control** (`tokens_with_no_span_are_kept_or_refused_never_dropped`, on review round 1's tokenizer): three spaces are refused by name, and `"a   a"`, `"a a a"` and `"a "` each hold every token of their encoding.
- **On real data, unchanged:** D1 and D2's ranges at both overlaps are identical to round 1's and to the corrected twin, with no refusals. So U1′'s passages, and its outputs, are unchanged.

**Review round 2, the rest of R2:** attached empty-span tokens were counted but not placed.
- **The bug:** under the trimmed tokenizer, `'a '` encodes to `(0,1)` and `(2,2)`. rnx returned `"a"` while crediting two tokens, and the twin's range `(0,2)` disagreed. A leading `' a'` (Ġ at `(1,1)`, then `(1,2)`) returned `"a"`, because the pending count was consumed when the first atom was created.
- **The rule now, in rnx and the twin:** an empty-span token is placed in the gap it was trimmed from.
  - After an atom, the atom extends to the token's offset.
  - Before the first atom, the first atom starts at the text's start.
  - With no atom at all, it's the named refusal.
- **The controls are on the actual passages,** not the token sum:
  - `'a '`, `'a  '`, `' a'`, `'a   a'` and `' a a '` each return exactly their text;
  - across several passages (`" a"×20`, `"a "×20`, `" a  a "×8` at W = 16), each passage re-tokenizes to exactly the tokens credited to it, and they rejoin to the text.
- **Twin parity on this tokenizer** (`probes/0136/trimmed_parity.sh`, `out/trimmed-parity.txt`): the pinned weights with review round 1's tokenizer, and a small D1 and D2 of these edge cases plus long runs. 32 and 33 passage ranges at overlap 0 and 32 are identical to the twin, and every plan check passes. Before the fix, the `'a '` case alone differed.
- **MiniLM's real-data ranges** are unchanged (its WordPiece reports no empty spans).

**Controls** (`text::tests::chunking`, 12 tests on four fixture tokenizers: whole words; WordPiece with multi-piece and two-byte pieces; byte-level BPE whose unknown characters are one token per byte on the same character's offsets; review round 1's byte-level tokenizer with trimmed offsets), and `tests/count_alloc.rs`:
- **Boundaries:**
  - a window never ends inside a word that fits;
  - an unfit word, ASCII or multi-byte, falls back to atoms, every cut on a character boundary;
  - an atom of 4 tokens against W = 3 is refused atomically;
  - empty and whitespace-only text gives an empty vector;
  - `[SEP]`, a vocabulary word in the fixture, is content and kept as written.
- **Adversarial, from the review:**
  - a short snapped window with nonzero overlap (one short word, then a 14-piece word) advances with no gap;
  - repair forced by an inflated re-measure removes words that begin the next passage, gapless at overlap 0;
  - repeated, overlapping and empty offsets from the byte-level fixture group into fewer atoms than tokens.
- **Bounds and arguments:**
  - repair beyond 16 removals, and "nothing fits", are each refused with no result;
  - each of the token, count, per-passage, total and document limits is refused before any string is built;
  - overlap −1, 8 (W/2 = 7) and `i64::MAX` are refused by name, and the encoder stays usable;
  - a document above `MAX_TEXT` is chunked into passages within it.
- **`count_tokens`:** it counts specials untruncated, shares the preflight, and leaves `embed`'s truncation in place.
- **Invariants:** every successful plan in the controls is checked for substrings on character boundaries, strict progress, no gaps, the last atom covered, each passage within `max_seq` once re-tokenized, and actual overlap at most the requested.

## 2. Real data (`probes/0136/probe`, `out/chunks.txt`)

**Every passage is checked by `probe0136 chunks`,** which exits 1 on any failure, with the same invariants as the controls.

| set | overlap | texts | passages | complete coverage | longest, with specials | over the limit | repairs | atom-fallback cuts | actual overlap (requested) |
|---|---:|---:|---:|---|---:|---:|---:|---:|---|
| D1 bodies | 0 | 321 | 3,461 | 321 of 321 | 256 | 0 | 0 | 0 | 0 |
| D2 tickets | 0 | 252 | 458 | 252 of 252 | 256 | 0 | 0 | 0 | 0 |
| D1 bodies | 32 | 321 | 3,870 | 321 of 321 | 256 | 0 | 0 | 0 | mean 31.8 (32); 352 of 3,549 pairs below the request |
| D2 tickets | 32 | 252 | 480 | 252 of 252 | 256 | 0 | 0 | 0 | mean 31.5 (32); 40 of 228 pairs below the request |

- **Requested against actual overlap:** below the request where snapping to a word start shortens it. It's never above.
- **Parity of the passages themselves:** `twin0133 chunks` implements the contract independently over the tokenizers crate: its own atoms, words, windows, repair and cursor. Its byte ranges are **identical** to rnx's: 3,919 ranges at overlap 0 and 4,350 at overlap 32 (`out/ranges-o0.tsv`, `out/ranges-o32.tsv`).
- **The generated adversarial corpus** (17 texts at both overlaps):
  - the texts: long words, digits, punctuation runs, CJK, emoji with modifiers, combining accents, mixed scripts, whitespace runs, literal `[CLS]` and `[SEP]` text, URLs, code, only spaces, empty, and one character;
  - complete plans: 17 of 17, with 113 and 125 passages, nothing over the limit;
  - named refusals: 0, reported separately from complete coverage.
- **Truncation, re-measured** (`count_tokens`):
  - whole D1 bodies would lose 90.2% of their tokens to the limit unchunked, and whole D2 tickets 48.0%;
  - U2's ticket inputs, unchanged by this record, lose 24.6%, reproducing 0133's figure exactly. Pooling a ticket's passages is 0137's.

## 3. U1′ (`probes/0136/workflow-u1c.rn`, `replay.sh`, `out/replay.txt`)

**U1′** is 0133's U1 with `enc.chunk(body, overlap)` in place of the frozen rule; the encoder is loaded before the documents. It's pasted into a session on `runner0134` with this tree's adapters, and passes on its marker with a final `Ok(())`.

| overlap | passages | chunk and load | embed | session run | gate 1 (twin, chunking on its own, at C = 32) | hit@1 | hit@5 |
|---|---:|---:|---:|---:|---|---:|---:|
| 0133's rule | 3,658 | | | 60.1 s (0135) | | 3 of 5 | 5 of 5 |
| 0 | 3,461 | 5.2 s | 63.5 s | 68.8 s | 25 rows, bit-equal | **5 of 5** | 5 of 5 |
| 32 | 3,870 | 5.2 s | 76.3 s | 81.6 s | 25 rows, bit-equal | 4 of 5 | 5 of 5 |

- **Cost:** chunking takes about 5 s for 321 documents, a per-call tokenizer copy per document and a re-measure per passage. Overlap 32 adds 12% more passages and 20% more embedding.
- **The rubric** (`score.py`; it reproduces 0133's 3 and 5 from 0133's saved output):
  - at overlap 0, Q1 (0063) and Q4 (0129), 0133's two near misses, now rank first;
  - at overlap 32, Q4 ranks second, behind 0082.
- **Quality is reported, not gated,** and no gain was promised. A change in either direction is a finding about this model and corpus.

## Gates

- **Truncation 0, and coverage complete:** pass, on D1, D2 and the generated corpus.
- **Parity:** the passage ranges are identical to the twin's, and U1′ is bit-equal at both overlaps.
- **Launch** (`probes/0136/launch.py`, 3 interleaved rounds of 60 launches against 0135's `59e3063`): deltas −1.36, −0.24 and +0.46 ms, within noise for two more registrations. The absolute times (6.4 to 10.8 ms) drifted across rounds; the interleaved deltas are the measure.
- **Suites:**

| suite | passed |
|---|---:|
| Candle, release | 54 (+13) |
| Candle, release `test-support` | 54 (+13) |
| Candle, debug `test-support` | 54 (+13) |
| Polars, release | 41 |
| project tool | 83 |
| `rnx` core | 392 |

  Clippy is clean on Candle with and without `test-support`. The release build without `test-support` first failed to compile, because the tests used a helper gated on that feature; it's now available under `cfg(test)` as well.
- **0135's replay** (`probes/0135/replay.sh` at C = 32 on this record's adapter, `out/replay-0135.txt`): passes unchanged. U1, U2 and U3 pass as sessions. Gate 1 is bit-equal to the twins (25, 459 and 254 rows). Gate 2 is unchanged, with a largest score change of 1.2e-7. All 12 controls are ok.
- **`:dep polars candle`, after push** (`out/dep/`): a clean worktree binary at the pushed `d3376d8`, a fresh cache, and ordinary `:dep polars candle` sessions (`probes/0136/replay.sh`, with the main tree's `twin0133`, whose source is committed at `d3376d8`).
  - **At overlap 0:** U1′ passes on its marker. Its run takes 70.2 s, of which embedding is 65.0 s and chunking with load 5.1 s. It's bit-equal to the twin, with hit@1 5 of 5 and hit@5 5 of 5.
  - **At overlap 32:** 80.6 s, bit-equal, with hit@1 4 of 5 and hit@5 5 of 5.
  - **The first `:dep`** built the adapters in 389.7 s, and the second attached in 16.9 s.

## Not done

- passage-to-document pooling for D2 tickets, and the other shape, indexing and nn operations (0137);
- other chunking strategies;
- any claim that the quality change generalizes.
