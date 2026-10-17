# Writing and spelling policy

This applies to everything people read: docs, plans, reviews, code comments, diagnostics, commit subjects and chat summaries. Code identifiers follow the API they belong to (see below).

## 1. One English: American

Use American spelling everywhere in prose.

| Use | Not |
|---|---|
| color | colour |
| behavior | behaviour |
| catalog | catalogue |
| canceled, canceling | cancelled, cancelling |
| enroll, enrollment | enrol, enrolment |
| license (noun and verb) | licence |
| analyze, initialize, normalize, serialize | analyse, initialise, normalise, serialise |
| gray | grey |
| modeling, labeled, traveled | modelling, labelled, travelled |
| center, meter | centre, metre |
| program (all senses) | programme |

If a word isn't in the table, use Merriam-Webster's first spelling.

**Identifiers are exempt.** Names in code, APIs, file names, CLI flags and data keys follow the API they belong to (Rust std, upstream Rune, crate metadata). Never rename an existing identifier just to change its spelling. New identifiers use American spelling.

**History is never rewritten for spelling.** Existing commits, frozen oracle data and receipts stay as they are. The policy applies from now on. When you touch a file anyway, you may fix prose spelling in the lines you change. Don't make spelling-only sweeps.

## 2. One name per concept

Pick the established term and keep it. Don't rotate synonyms for variety: a new word reads as a new concept. Current terms:

| Term | Meaning | Don't say |
|---|---|---|
| fresh core | the new execution engine (`experimental_core`) | new VM, v2 engine |
| stock VM | the reference Rune VM | old engine, legacy VM (until removal) |
| admit / admission | fresh compiles and runs a construct | support, enable |
| refuse / refusal | fresh declines at compile time | reject, fail, bail |
| oracle | frozen stock observations a record is checked against | baseline (that word is for measurements) |
| record | one numbered unit of work (plan + impl) | ticket, task, PR |
| gate | a required check with a pass/fail rule | test step, CI |
| counter2 | the instruction/cycle workload measurement | benchmark (alone) |

Add a term here before introducing it.

## 3. Plain technical prose

- Say what happened, with the number. "+0.16% instructions on shared_rows", not "a negligible impact".
- No filler or sales words: robust, seamless, leverage, comprehensive, cutting-edge, delve, crucial, powerful, elegant, game-changer, "it's worth noting", "in order to".
- No hedging stacks ("may potentially", "could possibly"). State confidence once, with its basis.
- Verbs over nominalizations: "we measured", not "a measurement was performed".
- Short sentences. One idea per sentence.
- Estimates are labeled as estimates, with their inputs. Computed figures say how they were computed.

## 4. Commit subjects

- The format is unchanged: `plan: rnx NNNN …` / `impl: rnx NNNN …` for rnx, `CNN oracle: …` / `CNN: …` for the Rune fork. Subject line only: no body, no trailers.
- Imperative, lower case after the prefix, American spelling, and no trailing period.
