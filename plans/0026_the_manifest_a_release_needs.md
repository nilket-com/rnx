# rnx 0026: the manifest a release needs

Status: proposed 2026-09-09. The twenty-sixth record of rnx. Record 0001's
release gates cover behaviour; none of them covers what the package says
about itself. This record decides that — and decides what the package still
refuses to say, because two of the answers are "not yet".

It is deliberately small. Windows acceptance (record 0025) and the provenance
review (record 0001 decision 7) both remain open, and nothing here moves
either.

One round of review is folded in, and it moved the shape of the gates rather
than the decisions: the first version of them read the manifest as written
and passed, while the package they produced failed them. Decision 7 is that
finding.

## Context

Measured, not assumed:

- The manifest carries four fields: `name`, `version`, `edition`,
  `publish = false`. No `description`, `repository`, `readme`, `keywords`,
  `categories` or `rust-version`.
- `cargo package --list` shipped **79 files**, including all 41 records in
  `plans/` (472 KB), `evidence.md`, and `journey.rn` — which cannot run
  without a program from another repository.
- The binary could not say which version it was, or which Rune it embeds.
  Record 0001's gate 6 asks each release to name that version; nothing in the
  build reported it, and `rune` exposes no version constant to read.
- `cargo +1.88 check --locked --all-targets` **fails**, and not in rnx: three
  errors in `rustyline`, on the unstable `file_lock` feature. Stable 1.95
  passes. No toolchain between them is installed here, so the boundary is
  unmeasured.
- The README said two things that had stopped being true: that the default
  command "runs the spike's assertions and prints observations" — record 0024
  made it a session — and that "the host process implementation is
  Linux/Unix only", which record 0025 changed. It also carried 58 lines
  describing an exercise in another repository, naming that repository's
  paths and products.

## Decision

### 1. The package describes itself, and stays unpublishable

`description`, `repository`, `readme`, `keywords` and `categories` are filled
in, because a package without them is one nobody can tell from an abandoned
name.

`publish = false` stays, and no `license` field appears. Those are **two
independent guards** on the same door: the first says no, and the second
means crates.io would refuse it anyway. Record 0001 decision 7 keeps both
until the provenance review settles, and a gate fails if either goes.

### 2. The version is a claim, so it stays at 0.0.0

`0.0.0` is not an unset field. It says no release has happened, which is
true: record 0001's gate 4 wants three platforms and two of them have never
run rnx. The number moves when the ladder earns it, and a gate fails on the
way past — which is the moment to read record 0001 again rather than to edit
the gate.

### 3. `rust-version` states the toolchain, not a floor

`rust-version = "1.95"` is what rnx is built and tested with. It is not a
measured minimum, and the record says so rather than implying one: 1.88 is
measured to fail, in a dependency rather than in rnx, and 1.89 to 1.94 have
no toolchain here to try. Stating the tested toolchain can only refuse a
build that might have worked; stating a guess can promise one that does not.

Narrowing it is a measurement, for a machine that has those toolchains.

### 4. The package ships the product, and the repository keeps the reasoning

`exclude = ["plans/", "evidence.md", "journey.rn"]`, measured at **79 files
down to 38**, with no `plans/` entry left. The records are the project's
reasoning rather than its product, and they are on the repository page for
anyone who wants them; `journey.rn` needs a program from elsewhere to run.
`src/` and `tests/` both ship, so a package that arrives can run its own
ladder.

### 5. The binary reports both versions

`rnx version`, `--version` and `-V` print two lines: this build, and the Rune
it is pinned to. Both, because either alone leaves a question open, and
record 0001's compatibility note is exactly the second one.

The Rune version is a constant in the source, because the crate exposes none
to read. That could drift from the pin, so a gate parses the manifest's `=`
pin and compares: a version bump that changes one without the other fails.

### 6. The README stops saying what is not true

Only that. The two false claims are corrected, the header says plainly that
this is not a release and which platforms have actually run, and the 58 lines
about another repository's exercise **move to `evidence.md`** — kept, because
they are what the spike was asked, and off the front page of a package,
because they are not about rnx.

A README written for someone arriving at a released tool is a different job,
and it belongs with the release, not before it.

### 7. The gates read the manifest Cargo writes, not the one we wrote

Found in review, by building the package and running these gates inside it:
four of the six failed. Cargo normalises a manifest when it packages, and
both changes broke the parsing:

- a generated comment header goes **above** `[package]`, so a parser that
  takes everything before the first `[` reads the header instead of the
  table;
- an inline dependency becomes a table of its own, `[dependencies.rune]`, so
  a pin looked for on a `rune = { ... }` line is not there at all.

The gates that mattered most were the ones that passed here and would have
failed in the published copy. So:

- the parsing is structural — a table is found by its own header line and
  ends at the next one, which works whether or not anything precedes it — and
  both dependency spellings are understood;
- every claim lives in **one** function that takes a manifest, and both gates
  call it, so neither can drift from the other;
- and the last gate runs against the extracted package rather than against a
  file list. **Counting packaged files establishes nothing about whether its
  ladder runs**, which is the other half of the same finding.

Running that ladder inside the package found one more thing, which only shows
there: **Cargo refuses to package a package.** An extracted crate contains
`Cargo.toml.orig`, and packaging a source tree that contains it is an error —
"invalid inclusion of reserved file name". A gate that always packages could
therefore never pass inside the copy it was checking. It now asks which copy
it is in: in the repository it packages and unpacks; in the package it makes
its claims against the manifest it was compiled against, which is that same
generated text without a second round trip.

### 8. The front page's links point where they still work

The README linked to `plans/` and `evidence.md` — both excluded by decision
4, so both dead on the package's page while reading fine in the repository.
They are now absolute repository URLs, and a gate checks that every relative
link in the shipped README names a file that shipped.

## Acceptance gates

All of these run here, on Linux, and all are met. Gates 6 to 8 are about the
copy that would be published rather than the copy in the repository.

1. **The package says what it is.** `description` names Rune and is long
   enough to describe something; `repository` is the public URL; `readme`
   points at the file; `keywords` and `categories` are present.
2. **It still refuses to be published.** `publish = false` and no `license`
   or `license-file`. Controlled: flipping `publish` to `true` fails the
   gate, and so does adding `license = "MIT"`.
3. **The version is the one the ladder earned.** The manifest says `0.0.0`
   and the binary's first line is `rnx 0.0.0`. Controlled: `0.1.0` fails the
   gate — and `--locked` refuses it first, because the lockfile records the
   version too.
4. **The reported Rune is the pinned Rune.** Controlled: changing the
   constant to `0.14.0` while the manifest pins `=0.14.1` fails the gate.
5. **Every spelling answers.** `version`, `--version` and `-V` give the same
   two lines and exit 0, and `rnx help` names the command — a command nothing
   lists is a command nobody finds.
6. **The packaged copy makes the same claims.** The gate packages the crate
   offline and unverified into a target directory of its own — a tenth of a
   second, no network, outside the build lock the harness holds — unpacks it,
   and asserts every claim above against the manifest **Cargo wrote**. It
   also checks that the manifest it read is the generated one, and that the
   generated one still spells the dependency as a section, so the gate cannot
   quietly stop testing what it exists to test.

   Controlled: with the parsing this cut replaced — a `[package]` table found
   by splitting on a newline and the header, and a pin looked for only in the
   inline spelling — the packaged gate fails with "the packaged manifest: no
   description", which is the comment header being read as the table.
7. **The packaged README's links work from the package.** Every relative link
   in the shipped README must name a file that shipped. Controlled: putting
   `](plans/0001_the_first_release.md)` back fails it, naming the file the
   package does not ship.
8. **The packaged crate runs its own ladder.** Not the file count — the
   ladder. Measured:

   ```sh
   cargo package --locked --offline --no-verify --allow-dirty
   tar xzf target/package/rnx-0.0.0.crate -C /tmp/ladder
   cd /tmp/ladder/rnx-0.0.0 && cargo test --locked --offline
   ```

   **232 tests, zero failures**, which is the repository's own default-feature
   total. The first run of this gate is what found the two parsing defects and
   the `Cargo.toml.orig` refusal; it is recorded here as the measurement that
   has to be repeated whenever the manifest or the excludes change.

   `cargo package` also prints `warning: manifest has no license or
   license-file` — which is decision 1's second guard, saying so.
9. **Nothing regresses.** The Linux ladder passes — 232 tests, 237 with
   `test-support`, the six new ones being the gates above — both macOS
   targets and the Windows target still check clean with and without
   `test-support`, clippy is at its eleven pre-existing warnings, and
   `cargo fmt --check` is clean. The package list is 38 files.

## Guardrails and stop conditions

1. `publish = false` and the absent license stay until record 0001 decision
   7 is settled. Neither is a formality to be tidied away.
2. No field claims a platform rnx has not run on. `rust-version` is the
   toolchain tested, and the README says which platforms are verified.
3. The version number follows the gates, never the other way round.
4. A packaging claim is measured by running the package, never by reading
   the file list it produced.
5. This record does not touch behaviour. If a release-metadata cut needs a
   behavioural change, it is the wrong cut for it — `rnx version` is here
   because it is the reporting half of the metadata itself, and nothing else
   was added.

## Risks

- **`rust-version` is conservative and unmeasured below 1.95.** It may refuse
  a toolchain that would have worked. The cost is a build that says so
  clearly; the alternative is a promise that fails at compile time in a
  dependency, which is what 1.88 does.
- **`exclude` is a denylist.** A new directory of non-product material would
  ship unless it is added. `include` would invert that risk — omitting a
  needed file — and an omission is loud where an inclusion is quiet, so this
  is a considered choice rather than a default. The 38-file list is in this
  record to compare against.
- **Moving the brief does not unpublish it.** The repository's history keeps
  every version of the README, including the paths and product names it
  carried. That is a fact for the provenance review, not something this cut
  can undo.
- **A gate can stop testing what it was written for.** The packaged gate
  asserts that the manifest it read is the generated one and that the
  dependency is still spelled as a section, because a future Cargo that
  stopped doing either would otherwise leave the gate passing while reading
  something else. That is a guard against a silent change of subject, not a
  proof against every one.
- **The description will age.** It says what rnx is today, and record 0001's
  scope is wider than what runs. It is a manifest field, not a promise, and
  the release README is where the accounting belongs.

## Forward

The release README, with the release. The license, after provenance. Windows
and macOS acceptance, on the machines. And record 0001's gate 6, which wants
`cargo install --locked` on all three — one third of which can be run here
and is not this cut's claim.
