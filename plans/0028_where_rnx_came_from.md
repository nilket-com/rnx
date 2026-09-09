# rnx 0028: where rnx came from

Status: proposed 2026-09-09. The twenty-eighth record of rnx. Record 0001
decision 7 holds the license undecided until one thing exists: a provenance
review of the code that moved here, which commit authorship alone does not
establish. This record is that review.

It is **a technical provenance inventory**, not a licensing conclusion. The
license is not chosen here — that is record 0001 decision 7's, the operator's
with counsel — and neither is the question of whether the obligations below
are satisfied by any particular way of shipping. A first draft of this record
said more than it had measured, in three places that review caught; each is
now corrected in place and the correction is part of the record.

## Context

The instruction this follows is narrower than the ket-wide licensing
inventory: trace the code actually exported into rnx and its dependencies,
separated from unrelated predecessor repositories. So the question is not
"what licenses exist across the estate" but "what is in this package, and
where did each part come from".

Three parts, measured separately below: code written here, code derived by
reading upstream, and code depended on.

## Decision

### 1. What the trace establishes, and what it cannot

**Code written here.** rnx's history is 57 commits with a single author of
record, `Joel Bondurant <em@joelbondurant.com>`. Every file under `src/` was
added by an rnx commit; none carries history imported from elsewhere. The one
import is documented: the feasibility spike moved out of ket on 2026-09-08
(ket commit `2edf9fb3`, whose message and the pointer at
`plans/rune-scripting/README.md` record the move), and the ket-side originals
have the same single author.

**What that cannot establish.** A trace of history bounds where files came
from as *files*; it cannot prove that no text was retyped from somewhere
else. What it does establish is that if anything entered from a predecessor
repository, it entered as text committed by the same author who wrote the
rest — which is exactly the gap record 0001 decision 7 names when it says
authorship alone is not enough. That gap closes with the operator's own
knowledge of those repositories, not with a command.

**No private product is named in what ships.** Of the 38 files the package
contains, the only internal-looking string is the GitHub organisation in the
`repository` URL. `journey.rn` — which the package excludes — does not name
`polariton` either; it takes the program's path as an argument, which is what
made the spike's "generic host modules only" claim true rather than
aspirational.

### 2. Three files are derived from upstream Rune by reading it

Not copied — read, and mirrored deliberately, each saying so at the site:

| Where | What it mirrors | Why it had to |
| --- | --- | --- |
| `src/json.rs`, `tests/json.rs` | `runtime/value/serde.rs`, arm for arm | The pre-walk must refuse exactly what the serializer would traverse |
| `src/runner.rs` | `runtime/budget.rs`'s no-budget sentinel | `usize::MAX` means "no limit" and must be refused |
| `src/session.rs` | The lexer's escape decoder | That decoder is crate-private |

Rune is `MIT OR Apache-2.0`. What that means for a mirror of an interface —
whether it is a derivative work at all, and whether it warrants attribution
beyond the citations already in the source — is a judgement about these
specific files, not something this inventory settles. It is recorded so that
a license review sees it rather than discovers it. Record 0001's
compatibility note is the natural home if attribution is the answer.

### 3. Four classes of dependency, counted separately

A count of a dependency graph is not an inventory of what a binary links, and
an earlier draft of this record conflated them: it reported "61 linked into
the Linux binary" from a graph query that included procedural macros,
their compile-time subtrees, and rnx itself. Corrected, per target:

| Class | Linux | Windows |
| --- | --- | --- |
| non-proc-macro normal dependencies | 49 | 51 |
| reached only through procedural macros (`proc-macro2`, `quote`, `syn` ×2) | 4 | 4 |
| procedural macros themselves | 7 | 7 |
| build dependencies only | 3 | 2 |
| development dependencies only | 0 | 0 |
| **participating in a build** | **63** | **64** |

`Cargo.lock` resolves **91 packages**, which is neither of those numbers: it
is a superset. Twenty-four of the 91 sit outside both measured graphs, and
that number is **23 dependency packages plus the root package**, which is
excluded from the counts rather than absent from the build — rnx plainly
participates in its own. The evidence file lists the 23 and gives the command
for each row.

**"Non-proc-macro normal dependency" is the closest of these to "in the
binary", and it is still a graph query rather than the linker's view.** This
record does not claim the stronger thing.

Every package in every class carries a license field, and all are permissive:
33 `MIT OR Apache-2.0` and 8 `MIT` dominate the Linux runtime class, with
four crates carrying terms beyond MIT-or-Apache (`unicode-ident`, `ryu`,
`zerocopy`, `memchr`) and two more on Windows (`clipboard-win`,
`error-code`). Full tallies are in the evidence file.

**`BSL-1.0` is the Boost Software License, not the Business Source License.**
The Business Source License is `BUSL-1.1` and appears nowhere in this tree.
The distinction is written here because a reviewer skimming for "BSL" would
refuse the wrong thing.

### 4. What the licenses require, read rather than characterised

An earlier draft called Boost "permissive with no notice requirement" and
treated the absence of `NOTICE` files as the end of the Apache question.
Both were wrong, and the texts say so:

- **Boost** requires its notice in all copies "unless such copies or
  derivative works are solely in the form of machine-executable object code
  generated by a source language processor". A notice requirement with an
  exception, and the exception happens to be the binary-only case — which is
  not the same as not having one.
- **Apache-2.0 §4(a)** requires giving recipients a copy of the License,
  independently of any `NOTICE`. §4(d)'s propagation duty is the part that is
  conditional on a `NOTICE` existing. So the absence of `NOTICE` files
  removes nothing.
- **MIT** requires its notice in all copies or substantial portions;
  **Unicode-3.0**, which applies to `unicode-ident` in addition to
  MIT-or-Apache, requires its notice with the copies or in the documentation.

Each dual-licensed crate lets the distributor pick a side, and the pick
changes which of these applies. That is the license decision's to make.

### 5. One third-party notice exists in the resolved set

`cfg_aliases 0.2.2` ships `NOTICES.md`, carrying MIT terms for code its macro
reuses from `tectonic_cfg_support`. It is a **build dependency of `nix`**,
reached through `rustyline` on unix targets only, and absent from the Windows
graph.

An earlier draft of this record said no package ships a notice. That scan
globbed the registry cache by crate name — 196 directories, every version this
machine has ever built, from any project — and covered normal edges only. The
corrected scan walks the 91 resolved package directories and finds four
notice-named paths, of which this is the one with third-party terms; the other
three are two `COPYRIGHT` files restating dual licensing and a directory
containing a single `.gitignore`.

**Whether a build-time dependency's attribution must travel with a distributed
binary is a separate question, and finding the file does not answer it.**

### 6. A distribution will need a third-party notice file

That much follows from decision 4 regardless of which side of each dual
license is taken: MIT, Apache and Unicode-3.0 all require notices to travel.
The classes in decision 3 **identify candidates for that review**; they do
not establish what an artifact contains, which is guardrail 3's distinction
and holds here too.

What this record does not do is enumerate the finished obligation set or
declare it satisfiable — that needs the license choice and a reading of each
text against the chosen form of distribution. Record 0001's gate 6 is where
the file belongs; it is not built here, because there is nothing to attach it
to while `publish = false`.

## Acceptance gates

Measured here, and all met. The first five are commands, reproduced with
their flags in the evidence file; the sixth is a reading of license texts,
quoted there rather than summarised.

1. **Single author of record.** `git log --format="%an <%ae>" | sort -u` in
   rnx returns one identity, over every commit in the repository — 57 of them
   when this record was written.
2. **No file imported without history.** Every `src/` file's adding commit is
   an rnx commit; the spike's move is recorded in ket `2edf9fb3` and in the
   pointer README, with the same author on both sides.
3. **Nothing private in the package.** The 38 shipped files name no product
   from the originating repository.
4. **Every package's license is known, and each class is counted
   separately.** `cargo metadata` and four `cargo tree` queries per target,
   all in the evidence file with their exact flags: 91 resolved, 63 and 64
   participating, 49 and 51 non-proc-macro normal. No missing license field
   except rnx's own deliberate absence; nothing copyleft, nothing
   source-available.
5. **The notice scan covers the resolved set.** Each of the 91 packages'
   directories, taken from its `manifest_path`, searched for `NOTICE*`,
   `AUTHORS*`, `COPYRIGHT*` and `THIRD*`. Four hits, characterised in the
   evidence file, one of which carries third-party terms.
6. **The obligations are quoted, not characterised.** Boost's object-code
   exception, Apache §4(a) and §4(d), MIT's and Unicode-3.0's notice terms are
   each reproduced from the crates' own license files.

## Guardrails and stop conditions

1. This record chooses no license and rules none out, and it does not declare
   any obligation satisfied. It says what is present and what the texts
   require.
2. A trace establishes provenance of files, not of sentences. Where the
   evidence stops, this record says so rather than rounding up to "clean".
3. **A class is named for what was measured.** "Non-proc-macro normal
   dependency" is a graph query; "linked into the binary" is the linker's
   answer and was not taken. Neither term is used for the other.
4. The inventory is per-release. A new dependency, a new version of one, or a
   `NOTICE` appearing upstream re-opens decisions 3 to 5.
5. A scan is over the resolved set, never the registry cache. The cache holds
   every version this machine has ever built, from any project, and a scan of
   it answers a different question — which is how an earlier draft reached
   196 directories and the wrong conclusion.
6. rnx's provenance is separate from the ket-wide inventory and from the
   predecessor repositories. Nothing here says anything about their licensing,
   and nothing there settles rnx's.

## Risks

- **The retyping gap.** Decision 1 states it plainly: history cannot rule out
  text brought from a predecessor repository by the same author. Only the
  operator can close it, and record 0001 decision 7 already says authorship
  alone is not enough.
- **`rustyline` is the widest surface.** It brings the Windows-only Boost
  crates and much of the transitive set. If the license review wants a
  smaller surface, the line editor is where to look — which is a design
  question for another record, not a provenance finding.
- **A notice file that is generated once rots.** Decision 6 asks for it at
  release, and guardrail 4 is what keeps it from being a one-off.
- **Build-time versus distributed material is unsettled here.** `cfg_aliases`
  is the concrete case: its notice exists, its code runs in the compiler, and
  whether its attribution must travel with an artifact is a question this
  record raises rather than answers.
- **Three claims in the first draft were wrong.** A registry-cache scan
  reported no notices; a graph count was called a linked-binary inventory;
  and Boost and Apache obligations were characterised from memory rather than
  read. All three are corrected above, and the pattern they share is worth
  more than the corrections: each was a stronger statement than the
  measurement underneath it.

## Forward

The license itself, which record 0001 decision 7 owns, and with it the
reading of each text against the chosen form of distribution. The third-party
notice file, with the first release. Whether a build dependency's attribution
travels with a binary — the `cfg_aliases` question. And the two open platform
records, Windows acceptance and macOS, which this does not touch.
