# rnx 0040 evidence: numbering without changing source identities

Implemented on nano, Linux x86_64, 2026-09-14. Plan: bdb64d6.
Bench reference: rnx-bench commit 977de83, probes/numbering and results/numbering_0040.

## Behaviour and gates

The session records boundaries in its existing admitted-input sequence. Source
indices, retained units and maps stay unchanged. A strict boundary lookup labels
old origins, including consecutive renumbers at the same boundary. The initial
boundary is implicit; an ordinary session allocates no boundary vector storage.
Reset clears both state and boundaries.

The prompt implements rustyline's Prompt interface. Result markers follow whether
the selected reader actually requested the prompt, rather than duplicating its
terminal predicate. The editor's styled request also determines marker styling.
Thus ordinary piped input stays unnumbered; dumb, EMACS and cons25 pipes have both
prompts and markers. A terminal on stdin with stdout redirected retains both.

- Gate 1: final suites pass, 304 default and 342 test-support, sequentially under
  TERM=xterm-256color. Existing test changes are limited to three exact bold-prompt
  strings in record 0039's PTY test and the two inspector command-count fixtures
  for the expanded help catalogue (6 → 7). HTTP fixtures needed no changes.
- Gates 2–3: new regular PTY tests cover admission, compile failure, unknown and
  recognized commands, unit/declaration silence, multiline input, empty input,
  abandoned unterminated string, editing/running Ctrl-C, input-cap refusal and
  allocation-ceiling refusal. New pipe tests assert complete transcripts in
  ordinary and unsupported modes. Prefix placement is source-reviewed: it is
  written once before the entire value. Gate 3 was clarified because the current
  renderer emits only single-line values; no nonexistent multiline example is
  claimed as measured.
- Gate 4: unit and subprocess tests preserve bindings, declarations, source text,
  stable source indices and history across renumbering. Retained closure errors
  cite their old numbering; new errors cite the current position only. Three
  numberings, repeated empty boundaries and reset are separate assertions.
- Gate 5: regular PTY tests assert bold numbered prompts/results. Pipe tests strip
  permitted styling and compare exactly. The emulator probe compares screen and
  cursor at 14 checkpoints under never/always, with raw captures retained. Both
  stdout-redirection modes pass. Dark/light specimens show the same session.
- Gate 6: exact successful stdout/stderr equivalence for version, help, eval,
  bare run and the 10k JSON workload precedes the measurements below. Default
  tests for existing pipe behaviour remain unchanged outside the named fixtures.

No dependencies or notices changed. Windows console execution remains unverified;
Linux PTY evidence is not a Windows claim.

## Cost

Rust 1.98.1, Rune 0.14.2, hyperfine 1.20.0; CPU 4, 10 warmups, 100 samples.
Before: accepted 0039 binary at 15289db. Raw hashes and commands are in the bench.
Measurements precede formatting-only cleanup of new blocks and a test-only PTY
wait optimization; the final full suites follow those edits.

| Command | Before mean ± σ (ms) | After mean ± σ (ms) |
| --- | --- | --- |
| version | 0.557 ± 0.022 | 0.547 ± 0.019 |
| help | 0.560 ± 0.016 | 0.551 ± 0.017 |
| eval | 4.062 ± 0.027 | 4.036 ± 0.085 |
| run | 3.718 ± 0.016 | 3.670 ± 0.019 |
| json | 11.774 ± 0.118 | 11.568 ± 0.101 |

No startup regression detected in this run; the small decreases do not establish
a speedup from numbering. Binary: 14,875,400 → 14,878,872 bytes. Session startup
allocation reference: 1,823,054 → 1,823,086 bytes. Later :memory sample:
1,831,667 → 1,831,714 bytes. These are allocation-request observations, not RSS.
Renumbering preserves contents, not allocator totals; its history entry and the
new boundary can allocate. Tests assert contents rather than byte equality.
