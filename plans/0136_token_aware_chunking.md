# rnx 0136: token-aware chunking

Status: plan, the second of the order agreed with Codex after 0133 (0135, then 0136, then 0137). Amended after Codex's review (exact boundaries, progress and coverage after snapping and repair; UTF-8 byte offsets and atoms; exact resource accounting).

**The finding (0133 F14):** under the frozen 180-word rule, 39% of D1's passages and 34% of D2's tickets are longer than the model's 256 tokens, so their tails are silently truncated. 10.6% of D1's tokens and 24.6% of D2's never reach the model. A script has no way to count tokens, so it can't split text to fit.

**This record** gives a script two token-aware operations on the encoder it already holds, and measures what they change on 0133's U1. It doesn't promise better retrieval.

## 1. The surface

**`enc.count_tokens(texts)` returns a vector of integers:** each text's token count with the model's special tokens, untruncated.
- **Input checks:** the same preflight as `embed` (the text count, each text's bytes, the total), refused by name before anything is copied.
- **Output:** 0131's `MAX_TEXTS` counts at most.

**`enc.chunk(text, overlap)` returns a vector of strings:** the text split into passages, each of which `embed` takes **without truncation.** Amended after the plan's review, the contract is exact.

**Tokenizing:**
- **Once, with a per-call configuration:** the text is tokenized once, without truncation, padding or special tokens, by a per-call copy of the encoder's tokenizer. `embed`'s truncation and padding are never changed.
- **The panic boundary:** both operations run under the existing joined worker's panic boundary.
- **Special-token-like text** (a literal `[SEP]` in the input) is whatever the actual encoding makes of it: nothing is erased by rnx. The result is empty only when that encoding has no tokens.

**Offsets and atoms:**
- **The offsets are UTF-8 byte offsets** into the original text, from `Tokenizer::encode`.
- **Each is validated:** start ≤ end ≤ the text's length, both on character boundaries. Anything else is refused by name ("the tokenizer reported an offset outside the text or inside a character").
- **Atoms:** byte-level and subword tokenizers can put several tokens on one character, and some report empty or repeated ranges. So tokens are grouped first into **atoms:** maximal runs of consecutive tokens whose byte ranges overlap, repeat, or are empty and attach to a neighbour.
  - An atom's range is the union of its tokens' ranges. Atom boundaries are always character boundaries.
  - An atom's size is its token count. Cuts happen only between atoms.
- **Words:** consecutive atoms with the same word id form a word. A token with no word id makes its atom a word of its own.
- **The atom table** is bounded by the token count, which is checked against its own limit, `MAX_TOKENS` (2^18), and refused by name above it: a tokenizer's output count isn't bounded by the input's bytes alone.
- **The input's own bound (amended during implementation):** a chunked text is a whole document, at most `MAX_DOCUMENT` (1 MiB), not `embed`'s per-text `MAX_TEXT` (64 KiB). D1's largest record is 102,776 bytes, and the first run refused it. Each passage stays a valid `embed` input, because its bytes are checked against `MAX_TEXT` (below).

**One passage, from a cursor (an atom index) to an end:**
1. **Whole words:** take whole words while the passage's tokens stay at most W, where W is `max_seq` minus the special tokens the model adds (254 for the pinned model).
2. **A word that doesn't fit:** if the first word alone exceeds W, fall back to whole atoms within that word, at most W tokens. The cut stays UTF-8-safe because atoms end on character boundaries, and the evidence counts each such cut.
3. **An atom that doesn't fit:** if a single atom exceeds W tokens, the call is refused by name ("text has a single unbreakable run of N tokens, above the model's W"), atomically, with no partial result.
4. **Repair:** the passage is the original substring over its atoms' byte range. Its token count with specials is re-measured (untruncated). While it exceeds `max_seq`, the last word is removed (the last atom, in the fallback), and the count is re-measured.
   - Repair is bounded at 16 removals per passage.
   - Beyond that, or if nothing would remain, the call is refused by name, atomically.
   - Each repair is counted. Removed content isn't lost: it begins the next passage (below).

**The next passage's start:**
- **It's computed from the repaired end** e, not the first window.
- **Overlap is a target:** go back from e by atoms totalling at most `overlap` tokens, then snap forward to a word start.
  - **Progress:** the next start must be strictly after this passage's start, so a passage that ended early (one short word followed by a W-token word) can't stall.
  - **No gaps:** it must be at or before e, so nothing between passages is skipped.
  - **Otherwise** it's e (no overlap for that pair).
- **`overlap`** must be 0 to W/2, refused by name otherwise. The **actual** overlap of each pair, in tokens, is recorded. The evidence reports requested against actual.

**Resource bounds, exact rather than estimated** (the review struck the earlier ratio claim):
- **All ranges are planned before any string is built:** every passage's byte range, its length, the running total of output bytes, and the count are checked against named limits as they're planned.
  - the count is at most `MAX_TEXTS`;
  - each passage is at most `MAX_TEXT` bytes, checked, since the input may now be larger;
  - the cumulative output bytes are at most `MAX_TOTAL`.
- **Failure is named and atomic:** any limit, or the bounded repair work, returns a named error, and no strings are built.
- **The re-measurement work is bounded** by passages × 17 tokenizations of at most one passage each.
- **Building** happens only after the plan passes: the returned strings are the planned substrings.

**Coverage, a gate:** the passages' atom intervals cover every atom of the text's encoding, by the original token intervals (not by re-tokenized ids). With overlap 0 they cover each exactly once.

**`count_tokens`** also uses a per-call configuration (untruncated, with specials) under the same panic boundary.

## 2. The workflow

**U1′ is 0133's U1 with `enc.chunk(body, overlap)` in place of the 180-word rule.** Everything else is unchanged: the frozen five-query rubric, the per-document maximum, the top 5.

**Run for overlap 0 and one nonzero value** (32 tokens, about one sentence).

**Reported separately, never combined into one score:**

| measure | how |
|---|---|
| **Truncation** | tokens lost to truncation, from `count_tokens` against the model limit. Expected 0, a gate |
| **Coverage** | content tokens covered by some passage, which must be all of them; with overlap, the duplication factor |
| **Cost** | passage count and embedding time, against 0133's 3,658 passages and 0135's time |
| **Parity** | the exact passage strings and byte ranges against a direct-Rust twin that chunks independently (its own atoms, words, windows, repair and cursor over the tokenizers crate), and the complete U1′ output bit for bit through 0135's gate 1 harness |
| **Quality, reported and not gated** | hit@1 and hit@5 on the frozen rubric, against 0133's 3/5 and 5/5. A change in either direction is a finding about this model and corpus, not a goal |

**D2 (U2 and U3):** a ticket split into passages needs one vector per ticket again, so passage-to-document pooling is 0137's primitive. Here, D2's truncation is re-measured with `count_tokens` and reported. The U2 and U3 inputs are unchanged.

## 3. Controls

- **Boundaries:**
  - a window that ends inside a multi-piece word moves back to that word's start;
  - a single word longer than the window falls back to atoms and is counted;
  - an atom longer than the window is refused, atomically;
  - text whose encoding has no tokens returns an empty vector, while literal special-token-like text follows the tokenizer's actual encoding.
- **Adversarial, from the review:**
  - a short snapped window with nonzero overlap: one short word followed by a W-token word must advance, with no gap;
  - a repair removing an end word that's followed by later content: the removed word begins the next passage;
  - one unfit word made of multi-byte characters: the atom fallback, with every cut on a character boundary;
  - repeated, overlapping and empty offsets from a fixture tokenizer (byte-level, several tokens on one character): grouped into atoms;
  - an offset outside the text or inside a character: refused by name;
  - repair beyond its bound: refused, with no partial result.
- **Progress and bounds:**
  - overlap at W/2 and above it (refused);
  - the passage count at its bound;
  - a document above `MAX_TEXT` chunked into passages within it, a text above `MAX_DOCUMENT` refused, and a lowered per-passage limit refused;
  - a vector over `MAX_TOTAL` for `count_tokens`.
- **The guarantee:** every passage of D1, of D2 and of a generated corpus with long words, digits, punctuation and non-ASCII text re-tokenizes within `max_seq`. A forced re-tokenization overflow triggers the word-by-word shortening.
- **Offsets and coverage:** every passage is the input's substring at its planned byte range, on character boundaries. The original token intervals are all covered, exactly once at overlap 0. Requested and actual overlap are recorded per pair.
- **Resource bounds:** the count, per-passage and cumulative byte limits are each refused before any string is built.
- **Errors:** a refused argument leaves the encoder and the text usable (0131's argument-survival rule).

## Gates

- **0135's replay** passes unchanged, since U1, U2 and U3 don't use the new operations.
- **U1′,** at both overlaps, as session pastes (pushed after review: `:dep polars candle`): truncation 0, coverage complete, parity bit for bit with the twin.
- **Suites:** core, Polars, Candle (release, `test-support` and debug), project, and clippy.
- **Launch** within noise of 0135, for two more registrations.

## Out of scope

- passage-to-document pooling and the other shape, indexing and nn operations (0137);
- other chunking strategies (sentences, semantic splitting);
- changing the model or its limit;
- any promised quality gain.
